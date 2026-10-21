//! Phase 2 bus cycle: when to poll PRIM, what each poll leaves for the next
//! runs, what the board shows. Pure: the caller makes the HTTP calls and
//! reads the clock.
//!
//! See CLAUDE.md, "Phase 2" (polling window, quota) and guide 14.

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::board::Board;
use crate::departures::{Answer, Direction, block, in_paris, paris_time, parse};

/// Day window, every day, Paris time, in minutes since midnight (06:00–21:00,
/// end excluded): PRIM only announces two real-time departures per line, so
/// they must be refreshed often.
const DAY_MINUTES: std::ops::Range<i32> = 6 * 60..21 * 60;
pub const DAY_PERIOD: SignedDuration = SignedDuration::from_mins(5);
pub const NIGHT_PERIOD: SignedDuration = SignedDuration::from_mins(15);
/// A run a few seconds early still polls: the timer fires every minute, and
/// a slow run must not push the poll back by a whole minute.
pub const TOLERANCE: SignedDuration = SignedDuration::from_secs(30);

/// What a pole's last polls left: its last good answer, and the failed
/// polls since.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pole {
    pub answer: Answer,
    pub failures: u32,
}

/// Bus data kept between runs, in the state file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusState {
    pub polled_at: Timestamp,
    pub rungis: Pole,
    pub antony: Pole,
}

/// Time between two PRIM polls at this instant.
pub fn poll_period(now: Timestamp) -> SignedDuration {
    let local = in_paris(now);
    let minutes = i32::from(local.hour()) * 60 + i32::from(local.minute());
    if DAY_MINUTES.contains(&minutes) {
        DAY_PERIOD
    } else {
        NIGHT_PERIOD
    }
}

/// Whether PRIM must be polled now. Always on the first run.
pub fn poll_due(now: Timestamp, last_poll: Option<Timestamp>) -> bool {
    last_poll.is_none_or(|last| now.duration_since(last) >= poll_period(now) - TOLERANCE)
}

/// The answer of a pole, or why there is none.
pub fn read_answer(status: u16, body: &str, direction: Direction) -> Result<Answer, String> {
    if status != 200 {
        return Err(format!("HTTP {status}"));
    }
    parse(body, direction)
}

/// What a poll leaves for a pole: the new answer, or the previous one with
/// one more failure.
pub fn record(previous: Option<&Pole>, result: Result<Answer, String>) -> Pole {
    match result {
        Ok(answer) => Pole {
            answer,
            failures: 0,
        },
        Err(_) => {
            let previous = previous.cloned().unwrap_or_default();
            Pole {
                failures: previous.failures + 1,
                ..previous
            }
        }
    }
}

/// The bus state after polling both poles.
pub fn after_poll(
    previous: Option<&BusState>,
    now: Timestamp,
    rungis: Result<Answer, String>,
    antony: Result<Answer, String>,
) -> BusState {
    BusState {
        polled_at: now,
        rungis: record(previous.map(|bus| &bus.rungis), rungis),
        antony: record(previous.map(|bus| &bus.antony), antony),
    }
}

/// The bus part of the screen at `now`.
pub fn board(now: Timestamp, bus: &BusState) -> Board {
    Board {
        clock: paris_time(now),
        rungis: block(&bus.rungis.answer, bus.rungis.failures, now),
        antony: block(&bus.antony.answer, bus.antony.failures, now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Block, Time};
    use crate::departures::{Departure, LINE_297, LINE_319, ServiceStatus};

    const RUNGIS_EVENING: &str = include_str!("../tests/fixtures/prim/rungis-20261013-2040.json");

    fn at(time: &str) -> Timestamp {
        time.parse().unwrap()
    }

    fn minutes(minutes: i64) -> SignedDuration {
        SignedDuration::from_mins(minutes)
    }

    fn answer(line: &str, time: &str) -> Answer {
        Answer {
            departures: vec![Departure {
                line: line.into(),
                at: at(time),
            }],
            statuses: vec![],
        }
    }

    /// Wednesday 2026-10-14, 08:00 in Paris (06:00 UTC): day.
    const DAY: &str = "2026-10-14T06:00:00Z";
    /// The same Wednesday, 22:00 in Paris: night.
    const NIGHT: &str = "2026-10-14T20:00:00Z";

    // A. Cadence

    #[test]
    fn first_run_polls() {
        assert!(poll_due(at(DAY), None));
    }

    #[test]
    fn day_polls_every_five_minutes() {
        let now = at(DAY);
        assert!(poll_due(now, Some(now - minutes(5))));
        assert!(!poll_due(now, Some(now - minutes(4))));
    }

    #[test]
    fn night_polls_every_fifteen_minutes() {
        let now = at(NIGHT);
        assert!(poll_due(now, Some(now - minutes(15))));
        assert!(!poll_due(now, Some(now - minutes(14))));
    }

    #[test]
    fn saturday_follows_the_same_window() {
        // Saturday 2026-10-17, 08:00 and 22:00 in Paris.
        assert_eq!(poll_period(at("2026-10-17T06:00:00Z")), DAY_PERIOD);
        assert_eq!(poll_period(at("2026-10-17T20:00:00Z")), NIGHT_PERIOD);
    }

    #[test]
    fn day_runs_from_06_00_to_21_00() {
        assert_eq!(poll_period(at("2026-10-14T03:59:00Z")), NIGHT_PERIOD);
        assert_eq!(poll_period(at("2026-10-14T04:00:00Z")), DAY_PERIOD);
        assert_eq!(poll_period(at("2026-10-14T18:59:00Z")), DAY_PERIOD);
        assert_eq!(poll_period(at("2026-10-14T19:00:00Z")), NIGHT_PERIOD);
    }

    #[test]
    fn a_run_a_few_seconds_early_still_polls() {
        let now = at(DAY);
        assert!(poll_due(
            now,
            Some(now - SignedDuration::from_secs(4 * 60 + 58))
        ));
    }

    // B. Polls

    #[test]
    fn http_error_is_a_failure() {
        assert!(read_answer(503, RUNGIS_EVENING, Direction::Rungis).is_err());
        assert!(read_answer(200, RUNGIS_EVENING, Direction::Rungis).is_ok());
    }

    #[test]
    fn success_replaces_the_answer_and_resets_the_failures() {
        let previous = Pole {
            answer: answer(LINE_319, "2026-10-14T06:08:00Z"),
            failures: 1,
        };
        let new = answer(LINE_319, "2026-10-14T06:31:00Z");
        assert_eq!(
            record(Some(&previous), Ok(new.clone())),
            Pole {
                answer: new,
                failures: 0
            }
        );
    }

    #[test]
    fn failure_keeps_the_previous_answer_and_counts() {
        let previous = Pole {
            answer: answer(LINE_319, "2026-10-14T06:08:00Z"),
            failures: 1,
        };
        assert_eq!(
            record(Some(&previous), Err("HTTP 503".into())),
            Pole {
                failures: 2,
                ..previous
            }
        );
        assert_eq!(
            record(None, Err("HTTP 503".into())),
            Pole {
                answer: Answer::default(),
                failures: 1
            }
        );
    }

    #[test]
    fn poles_are_counted_separately() {
        let bus = after_poll(
            None,
            at(DAY),
            Err("timeout".into()),
            Ok(answer(LINE_297, "2026-10-14T06:12:00Z")),
        );
        assert_eq!((bus.rungis.failures, bus.antony.failures), (1, 0));
        assert_eq!(bus.polled_at, at(DAY));
    }

    // C. Board

    #[test]
    fn board_shows_the_paris_clock_and_each_block() {
        let rungis = parse(RUNGIS_EVENING, Direction::Rungis).unwrap();
        let antony = Answer {
            departures: answer(LINE_297, "2026-10-13T18:52:00Z").departures,
            statuses: vec![(LINE_297.into(), ServiceStatus::AfterLastJourney)],
        };
        let bus = after_poll(None, at("2026-10-13T18:40:00Z"), Ok(rungis), Ok(antony));
        let board = board(at("2026-10-13T18:41:30Z"), &bus);
        assert_eq!(
            board.clock,
            Time {
                hour: 20,
                minute: 41
            }
        );
        assert_eq!(board.rungis, Block::Moon);
        assert_eq!(
            board.antony,
            Block::Times(vec![Time {
                hour: 20,
                minute: 52
            }])
        );
    }
}
