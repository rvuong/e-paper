# Guide 14: phase 2 cycle

`e-paper run` switches to the phase 2 screen (guides 11 to 13). Each minute
it:

1. computes the presence, as before;
2. if a PRIM poll is due (every 5 minutes 07:30–09:30 on weekdays, every 15
   minutes otherwise), polls the two home poles and updates their answer and
   failure count, kept in `state.json`; otherwise keeps the previous ones;
3. builds the board: Paris clock, then times, moon or wrench;
4. draws the frame, compares it with the one shown and picks the refresh:
   none, partial around the changed pixels, or full (first cycle of each
   hour, `--force`, no previous image).

| Module | Content | Tests |
|---|---|---|
| `update` | display update from two frames, hourly full refresh | E (14–18) |
| `departures` | `in_paris`; answers saved as JSON | existing |
| `bus` (new) | cadence, polls, board | A, B, D (1–10, 13) |
| `saved` | the bus state in `state.json` | C (11–12) |
| `cycle`, `update`, `usage` | phase 1 refresh logic removed | 15 removed, 2 rewritten |
| `main`, `systemd/` | wiring, PRIM key and home stops | by hand (section 6, 7) |

Approved tests: 18 (2026-10-14), with the removal of the phase 1 refresh
tests. Approved choices: full refresh on the first cycle of each hour;
partial refresh from a pixel comparison; 30 s tolerance on the cadence; a
PRIM failure never fails the run; home stops in `PRIM_STOP_RUNGIS` and
`PRIM_STOP_ANTONY` (clap `env` feature), `/etc/e-paper/prim.env` on the Pi;
stop `bus-log` before deploying (quota).

Checked while writing this guide: it was replayed step by step on a copy of
the project with guide 13 applied (each red step fails as described, each
green step passes; 187 library tests at the end, clippy clean), and a real
run on the dev PC drew the phase 2 screen from the Freebox and PRIM.

**Apply guide 13 first.** Run every command from the project root; edit the
files in VS Code.

## 1. Dependencies

```bash
cargo add jiff --features serde
```

```bash
cargo add clap --features env
```

`serde` lets the departures (UTC instants) be saved in `state.json`; `env` lets clap read an option from an environment variable.

## 2. `update`: refresh from two frames (tests 14–18)

**E. Display update**

14. `no_previous_frame_gives_a_full_refresh`.
15. `identical_frames_need_no_update`.
16. `clock_change_gives_a_partial_refresh_of_the_clock`: the phase 2 screen
    at 8:04 then 8:05 gives a partial refresh whose window has `x` and width
    multiples of 8, and stays within the clock area (above y 92, left of
    x 632).
17. `full_refresh_is_due_once_an_hour`: last full refresh at 08:59, now
    09:00:02: due; last at 09:00:02, now 09:59: not due; now 10:00: due; no
    full refresh yet: due.
18. `forced_or_full_only_refreshes`: `full_required` gives a full refresh
    even without change; with partial refresh disabled, a change gives a
    full refresh and no change gives none.

### Red

In `src/update.rs`:

add `use crate::frame::Frame;` before the `use crate::layout::…` line:

```rust
use crate::frame::Frame;
use crate::layout::{Window, layout, slot_window};
```

before `/// The window holding every slot that differs between two states`, add the three new functions, to be written:

```rust
/// Decides the update from the image shown and the new one.
///
/// - `previous`: the image shown on the display, if saved (a partial refresh
///   sends it again, as the panel forgets it between runs).
/// - `full_required`: a full refresh is asked for (`--force`, hourly
///   refresh against ghosting).
/// - `partial_enabled`: false to always use full refreshes.
pub fn plan_display(
    previous: Option<&Frame>,
    current: &Frame,
    full_required: bool,
    partial_enabled: bool,
) -> UpdateKind {
    todo!()
}

/// Whether the hourly full refresh is due: none happened since the start of
/// the current hour.
pub fn hourly_full_due(now: i64, last_full_refresh: Option<i64>) -> bool {
    todo!()
}

/// The window holding every pixel that differs between two frames, widened
/// to multiples of 8 horizontally. None if they are identical.
pub fn frame_window(previous: &Frame, current: &Frame) -> Option<Window> {
    todo!()
}
```

In the tests, replace the `use` lines with:

```rust
    use super::*;
    use crate::board::{Block, Board, Time, board_layout};
    use crate::layout::PEOPLE;
    use crate::render::{atkinson_bold, render_board};
    use crate::state::Person;
    use crate::state::PersonState::{self, Away, HomeActive, HomePassive};
```

after `const BEFORE…`, add:

```rust

    /// The phase 2 screen at this clock time, with the mockup's departures.
    fn screen(hour: u8, minute: u8) -> Frame {
        let at = |hour, minute| Time { hour, minute };
        let board = Board {
            clock: at(hour, minute),
            rungis: Block::Times(vec![at(8, 8), at(8, 31)]),
            antony: Block::Times(vec![at(8, 12), at(8, 21)]),
        };
        let household: HouseholdState = [(Person('R'), HomePassive)].into_iter().collect();
        let font = atkinson_bold();
        let mut frame = Frame::new();
        render_board(&mut frame, &board_layout(&board, &household, &font), &font).unwrap();
        frame
    }

    /// 2026-10-14 09:00:00 UTC, the start of an hour.
    const NINE: i64 = 1_791_968_400;
```

and at the end of the tests, before their closing `}`, add:

```rust

    // B. Phase 2 display update

    #[test]
    fn no_previous_frame_gives_a_full_refresh() {
        assert_eq!(
            plan_display(None, &screen(8, 4), false, true),
            UpdateKind::Full
        );
    }

    #[test]
    fn identical_frames_need_no_update() {
        assert_eq!(
            plan_display(Some(&screen(8, 4)), &screen(8, 4), false, true),
            UpdateKind::None
        );
    }

    #[test]
    fn clock_change_gives_a_partial_refresh_of_the_clock() {
        let UpdateKind::Partial(window) =
            plan_display(Some(&screen(8, 4)), &screen(8, 5), false, true)
        else {
            panic!("expected a partial refresh");
        };
        assert_eq!((window.x % 8, window.width % 8), (0, 0));
        // Within the clock area: above the horizontal line, left of the
        // vertical one.
        assert!(
            window.y >= 14 && window.y + window.height <= 92,
            "{window:?}"
        );
        assert!(window.x + window.width <= 632, "{window:?}");
    }

    #[test]
    fn full_refresh_is_due_once_an_hour() {
        assert!(hourly_full_due(NINE + 2, Some(NINE - 60)));
        assert!(!hourly_full_due(NINE + 59 * 60, Some(NINE + 2)));
        assert!(hourly_full_due(NINE + 60 * 60, Some(NINE + 2)));
        assert!(hourly_full_due(NINE, None));
    }

    #[test]
    fn forced_or_full_only_refreshes() {
        let (before, after) = (screen(8, 4), screen(8, 5));
        assert_eq!(
            plan_display(Some(&before), &before, true, true),
            UpdateKind::Full
        );
        assert_eq!(
            plan_display(Some(&before), &after, false, false),
            UpdateKind::Full
        );
        assert_eq!(
            plan_display(Some(&before), &before, false, false),
            UpdateKind::None
        );
    }
```

```bash
cargo test --lib update::
```

Expected: warnings `unused variable` (the parameters of the three `todo!()`); the 5 new tests fail with `not yet implemented`, the 7 others pass.

### Green

Replace the `use crate::layout::…` line with:

```rust
use crate::layout::{HEIGHT, WIDTH, Window, layout, slot_window};
```

In `plan_display`, replace `todo!()` with:

```rust
pub fn plan_display(
    previous: Option<&Frame>,
    current: &Frame,
    full_required: bool,
    partial_enabled: bool,
) -> UpdateKind {
    let Some(previous) = previous.filter(|_| !full_required) else {
        return UpdateKind::Full;
    };
    match frame_window(previous, current) {
        None => UpdateKind::None,
        Some(_) if !partial_enabled => UpdateKind::Full,
        Some(window) => UpdateKind::Partial(window),
    }
```

In `hourly_full_due`, replace `todo!()` with:

```rust
pub fn hourly_full_due(now: i64, last_full_refresh: Option<i64>) -> bool {
    let hour_start = now - now.rem_euclid(3_600);
    last_full_refresh.is_none_or(|last| last < hour_start)
```

In `frame_window`, replace `todo!()` with:

```rust
pub fn frame_window(previous: &Frame, current: &Frame) -> Option<Window> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if previous.is_black(x as i32, y as i32) != current.is_black(x as i32, y as i32) {
                let (left, top, right, bottom) = bounds.unwrap_or((x, y, x, y));
                bounds = Some((left.min(x), top.min(y), right.max(x), bottom.max(y)));
            }
        }
    }
    bounds.map(|(left, top, right, bottom)| {
        let x = left / 8 * 8;
        Window {
            x,
            y: top,
            width: (right + 1).div_ceil(8) * 8 - x,
            height: bottom + 1 - top,
        }
    })
```

```bash
cargo test --lib update::
```

Expected: `12 passed`, no warning.

## 3. `bus`: cadence, polls, board (tests 1–10, 13)

**A. Cadence**

1. `first_run_polls`: no previous poll, the poll is due.
2. `peak_polls_every_five_minutes`: Wednesday 08:00, 5 minutes after the
   previous poll: due; 4 minutes: not due.
3. `off_peak_polls_every_fifteen_minutes`: Wednesday 11:00, 15 minutes: due;
   14: not due.
4. `saturday_morning_is_off_peak`: Saturday 08:00 polls every 15 minutes.
5. `peak_runs_from_07_30_to_09_30`: 07:29 off-peak, 07:30 and 09:29 peak,
   09:30 off-peak (Paris time).
6. `a_run_a_few_seconds_early_still_polls`: 4 min 58 s after the previous
   poll, at peak: due (30 s tolerance).

**B. Polls**

7. `http_error_is_a_failure`: a valid answer with HTTP 503 is a failure; the
   same with HTTP 200 is read.
8. `success_replaces_the_answer_and_resets_the_failures`.
9. `failure_keeps_the_previous_answer_and_counts`: one failure more, the
   previous answer kept; a first failure without previous answer gives an
   empty answer and 1 failure.
10. `poles_are_counted_separately`: Rungis fails, Antony answers: 1 and 0
    failures.

**D. Board**

13. `board_shows_the_paris_clock_and_each_block`: at 20:41:30 Paris time,
    the clock shows 20:41; the Rungis answer of 20:40 (end of service) gives
    the moon; an Antony departure at 20:52 gives that time.

### Refactor: the Paris instant

The cadence needs the weekday in Paris, not only the time. In
`src/departures.rs`, replace `use jiff::{SignedDuration, Timestamp};` and
`use serde::Deserialize;` with:

```rust
use jiff::{SignedDuration, Timestamp, Zoned};
use serde::{Deserialize, Serialize};
```

Then replace the start of `paris_time`, from its comment to `let local = at.to_zoned(paris);`, with:

```rust
/// The instant in Paris, daylight saving included.
pub fn in_paris(at: Timestamp) -> Zoned {
    let paris = TimeZone::get("Europe/Paris").expect("time zone database bundled with jiff");
    at.to_zoned(paris)
}

/// The time of day in Paris, truncated to the minute: a bus is never shown
/// later than it leaves.
pub fn paris_time(at: Timestamp) -> Time {
    let local = in_paris(at);
```

The answers are saved between runs: add `Serialize, Deserialize` to the
`derive` of `ServiceStatus`, `Departure` and `Answer`:

```rust
/// Service status of a line at a pole, when it has no departure to give.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
```

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Departure {
```

```rust
/// What one answer says about its direction.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
```

```bash
cargo test --lib departures::
```

Expected: everything still passes (`21 passed`).

### Red

Create the file `src/bus.rs` with this content:

```rust
//! Phase 2 bus cycle: when to poll PRIM, what each poll leaves for the next
//! runs, what the board shows. Pure: the caller makes the HTTP calls and
//! reads the clock.
//!
//! Peak window and quota: guide 14.

use jiff::civil::Weekday;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::board::Board;
use crate::departures::{Answer, Direction, block, in_paris, paris_time, parse};

/// Peak window, weekdays only, Paris time, in minutes since midnight
/// (07:30–09:30, end excluded).
const PEAK_MINUTES: std::ops::Range<i32> = 7 * 60 + 30..9 * 60 + 30;
pub const PEAK_PERIOD: SignedDuration = SignedDuration::from_mins(5);
pub const OFF_PEAK_PERIOD: SignedDuration = SignedDuration::from_mins(15);
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
    todo!()
}

/// Whether PRIM must be polled now. Always on the first run.
pub fn poll_due(now: Timestamp, last_poll: Option<Timestamp>) -> bool {
    todo!()
}

/// The answer of a pole, or why there is none.
pub fn read_answer(status: u16, body: &str, direction: Direction) -> Result<Answer, String> {
    todo!()
}

/// What a poll leaves for a pole: the new answer, or the previous one with
/// one more failure.
pub fn record(previous: Option<&Pole>, result: Result<Answer, String>) -> Pole {
    todo!()
}

/// The bus state after polling both poles.
pub fn after_poll(
    previous: Option<&BusState>,
    now: Timestamp,
    rungis: Result<Answer, String>,
    antony: Result<Answer, String>,
) -> BusState {
    todo!()
}

/// The bus part of the screen at `now`.
pub fn board(now: Timestamp, bus: &BusState) -> Board {
    todo!()
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

    /// Wednesday 2026-10-14, 08:00 in Paris (06:00 UTC): peak.
    const PEAK: &str = "2026-10-14T06:00:00Z";
    /// The same Wednesday, 11:00 in Paris: off-peak.
    const OFF_PEAK: &str = "2026-10-14T09:00:00Z";

    // A. Cadence

    #[test]
    fn first_run_polls() {
        assert!(poll_due(at(PEAK), None));
    }

    #[test]
    fn peak_polls_every_five_minutes() {
        let now = at(PEAK);
        assert!(poll_due(now, Some(now - minutes(5))));
        assert!(!poll_due(now, Some(now - minutes(4))));
    }

    #[test]
    fn off_peak_polls_every_fifteen_minutes() {
        let now = at(OFF_PEAK);
        assert!(poll_due(now, Some(now - minutes(15))));
        assert!(!poll_due(now, Some(now - minutes(14))));
    }

    #[test]
    fn saturday_morning_is_off_peak() {
        // Saturday 2026-10-17, 08:00 in Paris.
        assert_eq!(poll_period(at("2026-10-17T06:00:00Z")), OFF_PEAK_PERIOD);
    }

    #[test]
    fn peak_runs_from_07_30_to_09_30() {
        assert_eq!(poll_period(at("2026-10-14T05:29:00Z")), OFF_PEAK_PERIOD);
        assert_eq!(poll_period(at("2026-10-14T05:30:00Z")), PEAK_PERIOD);
        assert_eq!(poll_period(at("2026-10-14T07:29:00Z")), PEAK_PERIOD);
        assert_eq!(poll_period(at("2026-10-14T07:30:00Z")), OFF_PEAK_PERIOD);
    }

    #[test]
    fn a_run_a_few_seconds_early_still_polls() {
        let now = at(PEAK);
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
            at(PEAK),
            Err("timeout".into()),
            Ok(answer(LINE_297, "2026-10-14T06:12:00Z")),
        );
        assert_eq!((bus.rungis.failures, bus.antony.failures), (1, 0));
        assert_eq!(bus.polled_at, at(PEAK));
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
```

In `src/lib.rs`, after `pub mod board;`, add:

```rust
pub mod bus;
```

```bash
cargo test --lib bus::
```

Expected: warnings (unused imports and parameters, the unused `PEAK_MINUTES`); the 11 tests fail with `not yet implemented`.

### Green

In `poll_period`, replace `todo!()` with:

```rust
pub fn poll_period(now: Timestamp) -> SignedDuration {
    let local = in_paris(now);
    let weekday = !matches!(local.weekday(), Weekday::Saturday | Weekday::Sunday);
    let minutes = i32::from(local.hour()) * 60 + i32::from(local.minute());
    if weekday && PEAK_MINUTES.contains(&minutes) {
        PEAK_PERIOD
    } else {
        OFF_PEAK_PERIOD
    }
```

In `poll_due`, replace `todo!()` with:

```rust
pub fn poll_due(now: Timestamp, last_poll: Option<Timestamp>) -> bool {
    last_poll.is_none_or(|last| now.duration_since(last) >= poll_period(now) - TOLERANCE)
```

In `read_answer`, replace `todo!()` with:

```rust
pub fn read_answer(status: u16, body: &str, direction: Direction) -> Result<Answer, String> {
    if status != 200 {
        return Err(format!("HTTP {status}"));
    }
    parse(body, direction)
```

In `record`, replace `todo!()` with:

```rust
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
```

In `after_poll`, replace `todo!()` with:

```rust
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
```

In `board`, replace `todo!()` with:

```rust
pub fn board(now: Timestamp, bus: &BusState) -> Board {
    Board {
        clock: paris_time(now),
        rungis: block(&bus.rungis.answer, bus.rungis.failures, now),
        antony: block(&bus.antony.answer, bus.antony.failures, now),
    }
```

```bash
cargo test --lib bus::
```

Expected: `11 passed`, no warning.

## 4. `saved`: the bus state in `state.json` (tests 11–12)

**C. Saved state**

11. `saved_state_with_bus_answers_reads_back_identical`: a state with a
    departure (UTC instant), a service status and failure counts is written
    then read back unchanged.
12. `phase_1_state_file_reads_back_without_bus_answers`: the state file of
    the running phase 1 (with its `partials_since_full`, without bus data)
    still reads, with no bus state: the first phase 2 run polls PRIM.

### Red

In `src/saved.rs`, in the tests, after `use super::*;`, add:

```rust
    use crate::bus::Pole;
    use crate::departures::{Answer, Departure, LINE_297, LINE_319, ServiceStatus};
```

and at the end of the tests, before their closing `}`, add:

```rust

    #[test]
    fn saved_state_with_bus_answers_reads_back_identical() {
        let at = |time: &str| time.parse().unwrap();
        let state = SavedState {
            bus: Some(BusState {
                polled_at: at("2026-10-14T06:00:00Z"),
                rungis: Pole {
                    answer: Answer {
                        departures: vec![Departure {
                            line: LINE_319.into(),
                            at: at("2026-10-14T06:08:36Z"),
                        }],
                        statuses: vec![],
                    },
                    failures: 1,
                },
                antony: Pole {
                    answer: Answer {
                        departures: vec![],
                        statuses: vec![(LINE_297.into(), ServiceStatus::AfterLastJourney)],
                    },
                    failures: 0,
                },
            }),
            ..example()
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        save(&path, &state).unwrap();
        assert_eq!(load(&path), (Some(state), None));
    }

    #[test]
    fn phase_1_state_file_reads_back_without_bus_answers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(
            &path,
            r#"{"polled_at": 1760000000, "household": {"R": "Away"}, "counters": {},
                "last_full_refresh": 1759990000, "partials_since_full": 3, "earlier": []}"#,
        )
        .unwrap();
        let (state, warning) = load(&path);
        assert_eq!(warning, None);
        assert_eq!(state.unwrap().bus, None);
    }
```

```bash
cargo test --lib saved::
```

Expected: it does not compile: `SavedState` has no field `bus`, and `BusState` is not found.

### Green

In `src/saved.rs`:

add `use crate::bus::BusState;` before `use crate::refresh::HouseholdState;`:

```rust
use crate::bus::BusState;
use crate::refresh::HouseholdState;
```

after the `earlier` field of `SavedState`, add the `bus` field:

```rust
    #[serde(default)]
    pub earlier: Vec<Snapshot>,
    /// Last PRIM answers and failed polls (phase 2). Absent from state files
    /// written before it existed.
    #[serde(default)]
    pub bus: Option<BusState>,
}
```

Every `SavedState { … }` now needs the field. In the test `example()` of `src/saved.rs`, after the `earlier: vec![…],` field:

```rust
                    .collect(),
            }],
            bus: None,
        }
    }
```

In `src/cycle.rs`, `cycle` carries the bus state over (the caller replaces it): after `earlier: recent_snapshots(…),`, add:

```rust
            earlier: recent_snapshots(input.previous, input.now),
            bus: input.previous.and_then(|previous| previous.bus.clone()),
        },
```

In its test helper `previous`, after `earlier: Vec::new(),`, add `bus: None,`:

```rust
            partials_since_full: 0,
            earlier: Vec::new(),
            bus: None,
        }
```

Same in the test helper `previous` of `src/usage.rs`:

```rust
            partials_since_full: 0,
            earlier: Vec::new(),
            bus: None,
        }
```

```bash
cargo test --lib saved::
```

Expected: `6 passed`.

## 5. Refactor: phase 1 refresh logic removed

The display update is now decided from the frames (`plan_display`), so the
phase 1 plan from the household states goes: `plan_update`,
`PARTIALS_BEFORE_FULL`, the daily refresh (`daily_refresh_due`, replaced by
the hourly one), `partials_since_full`, and the display fields of
`CycleInput` / `CycleOutput`. `changed_window` stays for `partial-test`.

### `update`

In `src/update.rs`, replace the module comment with:

```rust
//! Which display update a cycle needs: none, a full refresh, or a partial
//! refresh of the pixels that changed.
```

Delete the constant `PARTIALS_BEFORE_FULL` (and its comment), and the whole function `plan_update` (and its comment).

Replace the comment of `changed_window` with:

```rust
/// The window holding every slot that differs between two states, if any
/// (phase 1 row, used by `partial-test`).
```

In the tests, delete the 7 tests that call `plan_update` (from `identical_state_needs_no_update` to `disabled_partial_refresh_gives_full_refreshes_on_change_only`), and put these two in their place, before `// B. Phase 2 display update`:

```rust
    // A. Phase 1 row (partial-test)

    #[test]
    fn one_person_changing_gives_their_slot() {
        let current = household([HomePassive, HomePassive, HomePassive, Away]);
        assert_eq!(
            changed_window(&household(BEFORE), &current),
            Some(window_of(1))
        );
    }

    #[test]
    fn two_people_changing_give_both_slots() {
        let current = household([Away, Away, HomeActive, Away]);
        assert_eq!(
            changed_window(&household(BEFORE), &current),
            Some(window_of(0).union(window_of(2)))
        );
    }
```

### `cycle`

The changes are spread over the file (the daily refresh, three fields of `CycleInput`, the `update` of `CycleOutput`, 8 tests, the `force` argument of every test call): 

replace the whole content of `src/cycle.rs` with:

```rust
//! One cycle of the presence part: from the Freebox data and the previous
//! state to each person's state. Pure: the caller polls the Freebox, reads
//! the clock and the files. The bus part and the display update are decided
//! by the caller (`bus`, `update`).

use std::collections::BTreeMap;

use crate::devices::Mapping;
use crate::freebox::Host;
use crate::layout::PEOPLE;
use crate::presence::is_home;
use crate::rate::{DeviceRate, counter_delta, person_rate_bps, rate_bps};
use crate::refresh::HouseholdState;
use crate::saved::{SavedState, Snapshot};
use crate::state::{Person, person_state};

/// Rates are averaged over this window: the oldest reading of the counters
/// within it is the reference. With a cycle every minute, a one-minute rate
/// would follow every pause of a video.
pub const RATE_WINDOW_SECS: i64 = 900;

/// Everything a cycle needs.
pub struct CycleInput<'a> {
    /// Unix timestamp in seconds.
    pub now: i64,
    /// Presence window: the smartphone must have been active this recently.
    /// Independent of the cycle period.
    pub interval_secs: i64,
    pub threshold_bps: u64,
    pub mapping: &'a Mapping,
    /// Devices known to the Freebox (LAN browser), for the last activity.
    pub hosts: &'a [Host],
    /// Download counter of every Wi-Fi station, by MAC.
    pub counters: &'a BTreeMap<String, u64>,
    /// State left by the previous run, if any.
    pub previous: Option<&'a SavedState>,
}

/// What a cycle decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleOutput {
    pub household: HouseholdState,
    /// State to save for the next run. The caller sets its display and bus
    /// fields (`last_full_refresh`, `bus`), carried over from the previous
    /// state here.
    pub saved: SavedState,
}

pub fn cycle(input: &CycleInput) -> CycleOutput {
    let rates = device_rates(input, rate_reference(input.previous, input.now));
    let household: HouseholdState = PEOPLE
        .into_iter()
        .map(|person| {
            let home = is_home(
                smartphone_activity(input, person),
                input.now,
                input.interval_secs,
            );
            let rate = person_rate_bps(person, &rates);
            (person, person_state(home, rate, input.threshold_bps))
        })
        .collect();
    let last_full_refresh = input
        .previous
        .and_then(|previous| previous.last_full_refresh);
    let counters = input
        .counters
        .iter()
        .filter(|(mac, _)| input.mapping.contains_key(*mac))
        .map(|(mac, &counter)| (mac.clone(), counter))
        .collect();
    CycleOutput {
        saved: SavedState {
            polled_at: input.now,
            household: household.clone(),
            counters,
            last_full_refresh,
            earlier: recent_snapshots(input.previous, input.now),
            bus: input.previous.and_then(|previous| previous.bus.clone()),
        },
        household,
    }
}

/// The readings of the previous runs still within the rate window, oldest
/// first: the earlier ones the previous run kept, then its own.
fn recent_snapshots(previous: Option<&SavedState>, now: i64) -> Vec<Snapshot> {
    let Some(previous) = previous else {
        return Vec::new();
    };
    previous
        .earlier
        .iter()
        .cloned()
        .chain([Snapshot {
            polled_at: previous.polled_at,
            counters: previous.counters.clone(),
        }])
        .filter(|snapshot| now - snapshot.polled_at <= RATE_WINDOW_SECS)
        .collect()
}

/// The reading rates are measured from: the oldest within the rate window, or
/// the previous run's one if all are older (after a downtime). None on the
/// first run.
fn rate_reference(previous: Option<&SavedState>, now: i64) -> Option<Snapshot> {
    let previous = previous?;
    recent_snapshots(Some(previous), now)
        .into_iter()
        .next()
        .or_else(|| {
            Some(Snapshot {
                polled_at: previous.polled_at,
                counters: previous.counters.clone(),
            })
        })
}

/// Rate of each assigned device seen during this poll, since the reference
/// reading. All zero on the first run, as there is nothing to measure from.
fn device_rates(input: &CycleInput, reference: Option<Snapshot>) -> Vec<DeviceRate> {
    let Some(reference) = reference else {
        return Vec::new();
    };
    let elapsed = input.now - reference.polled_at;
    input
        .mapping
        .iter()
        .filter_map(|(mac, assignment)| {
            let current = *input.counters.get(mac)?;
            let delta = counter_delta(reference.counters.get(mac).copied(), current);
            Some(DeviceRate {
                owner: Some(assignment.person),
                bps: rate_bps(delta, elapsed),
            })
        })
        .collect()
}

/// Last activity of the person's smartphone, if configured and known to the
/// Freebox.
fn smartphone_activity(input: &CycleInput, person: Person) -> Option<i64> {
    let (mac, _) = input
        .mapping
        .iter()
        .find(|(_, assignment)| assignment.person == person && assignment.smartphone)?;
    input
        .hosts
        .iter()
        .find(|host| &host.mac == mac)?
        .last_activity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::Assignment;
    use crate::state::PersonState::{self, Away, HomeActive, HomePassive};

    const NOW: i64 = 1_760_000_000;
    const INTERVAL: i64 = 900;
    const THRESHOLD: u64 = 5_000_000;

    const R_PHONE: &str = "02:00:00:00:00:01";
    const R_LAPTOP: &str = "02:00:00:00:00:02";
    const H_PHONE: &str = "02:00:00:00:00:03";
    const H_CONSOLE: &str = "02:00:00:00:00:04";
    const UNMAPPED_TV: &str = "02:00:00:00:00:05";

    /// R and H each have a smartphone and another device; M and L have none.
    fn mapping() -> Mapping {
        let assign = |person, smartphone| Assignment {
            person: Person(person),
            name: String::new(),
            smartphone,
        };
        [
            (R_PHONE, assign('R', true)),
            (R_LAPTOP, assign('R', false)),
            (H_PHONE, assign('H', true)),
            (H_CONSOLE, assign('H', false)),
        ]
        .into_iter()
        .map(|(mac, assignment)| (mac.to_string(), assignment))
        .collect()
    }

    fn host(mac: &str, last_activity: i64) -> Host {
        Host {
            mac: mac.into(),
            name: String::new(),
            vendor: String::new(),
            active: true,
            last_activity: Some(last_activity),
            access_point: None,
        }
    }

    fn counters(values: &[(&str, u64)]) -> BTreeMap<String, u64> {
        values
            .iter()
            .map(|&(mac, counter)| (mac.to_string(), counter))
            .collect()
    }

    /// A previous poll 900 s ago with these counters, everybody away.
    fn previous(values: &[(&str, u64)]) -> SavedState {
        SavedState {
            polled_at: NOW - 900,
            household: PEOPLE.into_iter().map(|person| (person, Away)).collect(),
            counters: counters(values),
            last_full_refresh: Some(NOW - 900),
            earlier: Vec::new(),
            bus: None,
        }
    }

    fn run(
        hosts: &[Host],
        now_counters: &[(&str, u64)],
        previous: Option<&SavedState>,
    ) -> CycleOutput {
        cycle(&CycleInput {
            now: NOW,
            interval_secs: INTERVAL,
            threshold_bps: THRESHOLD,
            mapping: &mapping(),
            hosts,
            counters: &counters(now_counters),
            previous,
        })
    }

    fn state_of(output: &CycleOutput, initial: char) -> PersonState {
        output.household[&Person(initial)]
    }

    /// 6 Mb/s over 900 s, in bytes.
    const SIX_MBPS: u64 = 6_000_000 / 8 * 900;
    /// 3 Mb/s over 900 s, in bytes.
    const THREE_MBPS: u64 = 3_000_000 / 8 * 900;

    #[test]
    fn recently_active_smartphone_means_home() {
        let output = run(&[host(R_PHONE, NOW - 120)], &[], None);
        assert_eq!(state_of(&output, 'R'), HomePassive);
    }

    #[test]
    fn person_without_smartphone_is_away() {
        let output = run(&[host(R_PHONE, NOW - 120)], &[], None);
        assert_eq!(state_of(&output, 'M'), Away);
    }

    #[test]
    fn smartphone_unknown_to_the_freebox_is_away() {
        let output = run(&[host(R_PHONE, NOW - 120)], &[], None);
        assert_eq!(state_of(&output, 'H'), Away);
    }

    #[test]
    fn device_rate_uses_the_time_since_the_previous_poll() {
        // 6 Mb/s on one device of R, R at home: above the threshold.
        let before = previous(&[(R_LAPTOP, 1_000)]);
        let output = run(
            &[host(R_PHONE, NOW - 60)],
            &[(R_LAPTOP, 1_000 + SIX_MBPS)],
            Some(&before),
        );
        assert_eq!(state_of(&output, 'R'), HomeActive);
    }

    #[test]
    fn first_run_has_zero_rates() {
        let output = run(
            &[host(R_PHONE, NOW - 60)],
            &[(R_LAPTOP, 50_000_000_000)],
            None,
        );
        assert_eq!(state_of(&output, 'R'), HomePassive);
    }

    #[test]
    fn person_rate_sums_their_devices_and_ignores_unmapped_ones() {
        // H: 3 + 3 Mb/s on two devices, above the threshold together.
        let before = previous(&[(H_PHONE, 0), (H_CONSOLE, 0)]);
        let output = run(
            &[host(H_PHONE, NOW - 60), host(R_PHONE, NOW - 60)],
            &[
                (H_PHONE, THREE_MBPS),
                (H_CONSOLE, THREE_MBPS),
                (UNMAPPED_TV, 10 * SIX_MBPS),
            ],
            Some(&before),
        );
        assert_eq!(state_of(&output, 'H'), HomeActive);
        // The unmapped TV counts for nobody: R stays passive.
        assert_eq!(state_of(&output, 'R'), HomePassive);
    }

    #[test]
    fn repeater_device_gets_a_rate_in_the_cycle() {
        // H's console is on the repeater: no Wi-Fi station counter, only the
        // LAN browser's.
        let mut console = host(H_CONSOLE, NOW - 60);
        console.access_point = Some(crate::freebox::HostAccessPoint {
            kind: Some("repeater".into()),
            connectivity: Some("wifi".into()),
            tx_bytes: Some(SIX_MBPS),
            fields: Vec::new(),
        });
        let hosts = [host(H_PHONE, NOW - 60), console];
        let counters = crate::freebox::download_counters(BTreeMap::new(), &hosts);
        let before = previous(&[(H_CONSOLE, 0)]);
        let output = cycle(&CycleInput {
            now: NOW,
            interval_secs: INTERVAL,
            threshold_bps: THRESHOLD,
            mapping: &mapping(),
            hosts: &hosts,
            counters: &counters,
            previous: Some(&before),
        });
        assert_eq!(state_of(&output, 'H'), HomeActive);
    }

    #[test]
    fn heavy_download_with_smartphone_away_is_away() {
        let before = previous(&[(R_LAPTOP, 0)]);
        let output = run(
            &[host(R_PHONE, NOW - 3_600)],
            &[(R_LAPTOP, SIX_MBPS)],
            Some(&before),
        );
        assert_eq!(state_of(&output, 'R'), Away);
    }

    #[test]
    fn saved_counters_are_the_assigned_devices_seen_with_the_poll_time() {
        let output = run(
            &[],
            &[(R_LAPTOP, 42), (H_CONSOLE, 7), (UNMAPPED_TV, 99)],
            None,
        );
        assert_eq!(output.saved.polled_at, NOW);
        assert_eq!(
            output.saved.counters,
            counters(&[(R_LAPTOP, 42), (H_CONSOLE, 7)])
        );
        assert_eq!(output.saved.household, output.household);
    }

    #[test]
    fn rate_is_averaged_from_the_oldest_reading_in_the_window() {
        // R's laptop downloaded at 6 Mb/s on average over the last 14 minutes,
        // but nothing during the last minute: still active.
        let total = 6_000_000 / 8 * 840;
        let mut before = previous(&[(R_LAPTOP, total)]);
        before.polled_at = NOW - 60;
        before.earlier = vec![Snapshot {
            polled_at: NOW - 840,
            counters: counters(&[(R_LAPTOP, 0)]),
        }];
        let output = run(
            &[host(R_PHONE, NOW - 60)],
            &[(R_LAPTOP, total)],
            Some(&before),
        );
        assert_eq!(state_of(&output, 'R'), HomeActive);
    }

    #[test]
    fn readings_older_than_the_window_are_dropped() {
        let mut before = previous(&[(R_LAPTOP, 500)]);
        before.polled_at = NOW - 60;
        before.earlier = vec![
            Snapshot {
                polled_at: NOW - 1_200,
                counters: counters(&[(R_LAPTOP, 100)]),
            },
            Snapshot {
                polled_at: NOW - 600,
                counters: counters(&[(R_LAPTOP, 300)]),
            },
        ];
        let output = run(&[], &[(R_LAPTOP, 600)], Some(&before));
        assert_eq!(
            output.saved.earlier,
            [
                Snapshot {
                    polled_at: NOW - 600,
                    counters: counters(&[(R_LAPTOP, 300)]),
                },
                Snapshot {
                    polled_at: NOW - 60,
                    counters: counters(&[(R_LAPTOP, 500)]),
                },
            ]
        );
    }

    #[test]
    fn after_a_downtime_the_previous_reading_is_the_reference() {
        // Previous run an hour ago, nothing kept: 6 Mb/s over that hour.
        let mut before = previous(&[(R_LAPTOP, 0)]);
        before.polled_at = NOW - 3_600;
        let output = run(
            &[host(R_PHONE, NOW - 60)],
            &[(R_LAPTOP, 6_000_000 / 8 * 3_600)],
            Some(&before),
        );
        assert_eq!(state_of(&output, 'R'), HomeActive);
        assert!(output.saved.earlier.is_empty());
    }
}
```

### `saved` and `usage`

In `src/saved.rs`, delete the field `partials_since_full` (its 3 lines), and the line `partials_since_full: 3,` of the test `example()`. Replace the comment of `last_full_refresh`, `/// Last time the display was redrawn, Unix timestamp in seconds. Absent`, with:

```rust
    /// Last full refresh of the display, Unix timestamp in seconds. Absent
```

In `src/usage.rs`, in the tests: delete the line `partials_since_full: 0,` of `previous`, and the three lines `previous_frame: true,`, `force: false,`, `partial_enabled: true,` of the `CycleInput` in `lines`.

### `main`: wiring

In `src/main.rs`, replace the `use` lines from `use e_paper::cycle::…` to
`use e_paper::usage;` with (new: `board`, `bus`, `departures`,
`render_board`, `hourly_full_due`, `plan_display`, `jiff::Timestamp`):

```rust
use e_paper::board::{Block, Time, board_layout};
use e_paper::bus::{self, BusState};
use e_paper::cycle::{CycleInput, cycle};
use e_paper::departures::Direction;
use e_paper::devices;
use e_paper::diagnose;
use e_paper::display::{Display, PngDisplay, Update};
use e_paper::epd;
use e_paper::frame::Frame;
use e_paper::freebox;
use e_paper::freebox_client::Client;
use e_paper::layout::{PEOPLE, layout};
use e_paper::mockups::phase1_states;
use e_paper::prim::{self, Stop};
use e_paper::prim_client;
use e_paper::refresh::HouseholdState;
use e_paper::render::{atkinson_bold, render, render_board};
use e_paper::saved;
use e_paper::state::PersonState::{Away, HomeActive, HomePassive};
use e_paper::state::{ACTIVITY_THRESHOLD_BPS, PersonState};
use e_paper::update::{UpdateKind, changed_window, hourly_full_due, plan_display};
use e_paper::usage;
use jiff::Timestamp;
```

Replace the comment of `Run` with:

```rust
    /// Runs one cycle: polls the Freebox (and PRIM on its own cadence),
    /// computes the state, redraws if needed.
    Run {
```

In `Run`, after `output: Output,`, add the PRIM options:

```rust
        #[command(flatten)]
        output: Output,
        #[command(flatten)]
        prim: Prim,
        /// Presence window in seconds
```

Before `/// Where to show the display.`, add:

```rust
/// The PRIM API, for the bus part.
#[derive(clap::Args)]
struct Prim {
    /// The PRIM API key (a secret).
    #[arg(long = "prim-key", default_value = "secrets/prim_api_key")]
    key: PathBuf,
    /// SIRI reference of the home pole towards Rungis, e.g.
    /// `STIF:StopPoint:Q:12345:`. Kept out of git: set it in the environment.
    #[arg(long, env = "PRIM_STOP_RUNGIS")]
    stop_rungis: String,
    /// SIRI reference of the home pole towards Antony RER.
    #[arg(long, env = "PRIM_STOP_ANTONY")]
    stop_antony: String,
}
```

In `run`, pass them on: in the `Command::Run` arm, add `prim,` after `output,` in the pattern and `&prim,` after `&output,` in the call:

```rust
            output,
            prim,
            interval,
            force,
            full_only,
            usage_log,
        } => run_cycle(
            &token,
            &devices,
            &state,
            &output,
            &prim,
            interval,
```

Replace the whole function `run_cycle` with this one, followed by the new `poll_prim`:

```rust
#[allow(clippy::too_many_arguments)]
fn run_cycle(
    token_path: &Path,
    devices_path: &Path,
    state_path: &Path,
    output_target: &Output,
    prim: &Prim,
    interval_secs: i64,
    force: bool,
    partial_enabled: bool,
    usage_log: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(devices_path)?;
    let app_token = read_token(token_path)?;
    let (previous, warning) = saved::load(state_path);
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }
    // The image on the display, kept next to the state: a partial refresh
    // sends it again, as the panel forgets it between runs.
    let frame_path = state_path.with_file_name("frame.png");
    let previous_frame = fs::read(&frame_path)
        .ok()
        .and_then(|bytes| Frame::from_png(&bytes).ok());

    let mut client = Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
    // Devices on the repeater only have the LAN browser's counter.
    let counters = freebox::download_counters(client.wifi_counters()?, &hosts);
    let now_at = Timestamp::now();
    let now = now_at.as_second();

    let input = CycleInput {
        now,
        interval_secs,
        threshold_bps: ACTIVITY_THRESHOLD_BPS,
        mapping: &mapping,
        hosts: &hosts,
        counters: &counters,
        previous: previous.as_ref(),
    };
    let output = cycle(&input);
    // The display matters more than the log: a write failure is only a warning.
    if let Some(path) = usage_log
        && let Err(error) = append_usage(path, &usage::usage_lines(&input, &output.household))
    {
        eprintln!("warning: cannot write {}: {error}", path.display());
    }
    let states: Vec<String> = PEOPLE
        .iter()
        .map(|person| format!("{} {}", person.0, describe(output.household[person])))
        .collect();
    println!("{}", states.join(", "));

    // PRIM is polled on its own cadence; a failed poll only shows as the
    // wrench, the presence part is still drawn.
    let previous_bus = previous.as_ref().and_then(|state| state.bus.as_ref());
    let bus = match previous_bus.filter(|bus| !bus::poll_due(now_at, Some(bus.polled_at))) {
        Some(bus) => bus.clone(),
        None => poll_prim(prim, previous_bus, now_at)?,
    };
    let board = bus::board(now_at, &bus);
    println!(
        "Rungis: {}; Antony: {}",
        describe_block(&board.rungis),
        describe_block(&board.antony)
    );

    let font = atkinson_bold();
    let mut frame = Frame::new();
    render_board(
        &mut frame,
        &board_layout(&board, &output.household, &font),
        &font,
    )?;
    let mut saved = output.saved;
    let full_required = force || hourly_full_due(now, saved.last_full_refresh);
    let plan = plan_display(
        previous_frame.as_ref(),
        &frame,
        full_required,
        partial_enabled,
    );
    let update = match (plan, &previous_frame) {
        (UpdateKind::None, _) => None,
        (UpdateKind::Partial(window), Some(previous)) => Some(Update::Partial { previous, window }),
        _ => Some(Update::Full),
    };
    match update {
        None => println!("No change: display not touched"),
        Some(update) => {
            output_target.open()?.show(&frame, update)?;
            fs::write(&frame_path, frame.to_png())
                .map_err(|error| format!("cannot write {}: {error}", frame_path.display()))?;
            let kind = match update {
                Update::Full => {
                    saved.last_full_refresh = Some(now);
                    "full"
                }
                Update::Partial { .. } => "partial",
            };
            println!("Display redrawn ({kind}): {}", output_target.describe());
        }
    }
    saved.bus = Some(bus);
    saved::save(state_path, &saved)
        .map_err(|error| format!("cannot write {}: {error}", state_path.display()))?;
    Ok(())
}

/// Polls both home poles. A failed poll is a warning, counted in the state.
fn poll_prim(
    prim: &Prim,
    previous: Option<&BusState>,
    now: Timestamp,
) -> Result<BusState, Box<dyn Error>> {
    let client = prim_client::Client::new(read_prim_key(&prim.key)?);
    let poll = |label: &str, stop: &str, direction| {
        let result = match client.stop_monitoring(stop) {
            Ok(answer) => bus::read_answer(answer.status, &answer.body, direction),
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = &result {
            eprintln!("warning: PRIM {label}: {error}");
        }
        result
    };
    let rungis = poll("rungis", &prim.stop_rungis, Direction::Rungis);
    let antony = poll("antony", &prim.stop_antony, Direction::Antony);
    Ok(bus::after_poll(previous, now, rungis, antony))
}
```

Before `fn describe`, add:

```rust
fn describe_block(block: &Block) -> String {
    match block {
        Block::Times(times) if times.is_empty() => "no departure".into(),
        Block::Times(times) => times
            .iter()
            .map(Time::to_string)
            .collect::<Vec<_>>()
            .join(" "),
        Block::Moon => "service over (moon)".into(),
        Block::Wrench => "bus data unavailable (wrench)".into(),
    }
}

fn describe(state: PersonState) -> &'static str {
```

Before `fn read_token`, add:

```rust
fn read_prim_key(path: &Path) -> Result<String, Box<dyn Error>> {
    let key = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(prim::api_key(&key)?)
}

fn read_token(path: &Path) -> Result<String, Box<dyn Error>> {
```

And in `bus_log`, replace its first three lines (reading the key, creating the client) with:

```rust
    let client = prim_client::Client::new(read_prim_key(key_path)?);
```

### systemd

In `systemd/e-paper.service`, the service gets the PRIM key as a credential and the home stops from a file kept out of git:

add `--prim-key %d/prim_api_key \` after the `--devices` line of `ExecStart`:

```ini
    --devices %d/devices.toml \
    --prim-key %d/prim_api_key \
    --state
```

and after the `LoadCredential=devices.toml…` line:

```ini
LoadCredential=devices.toml:/etc/e-paper/devices.toml
LoadCredential=prim_api_key:/etc/e-paper/prim_api_key
# The home stops (PRIM_STOP_RUNGIS, PRIM_STOP_ANTONY): personal data, kept
# out of git.
EnvironmentFile=/etc/e-paper/prim.env
```

```bash
cargo fmt
```

```bash
cargo test
```

Expected: `187 passed` for the library (182 + 18 new − 15 removed + 2 rewritten), `2 passed` for the golden images.

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: no warning.

## 6. Try it on the dev PC

The home stop references are the ones of your `bus-log` command (Antony
pole, then Rungis pole). One run polls PRIM twice (quota: fine).

```bash
PRIM_STOP_RUNGIS='STIF:StopPoint:Q:…:' PRIM_STOP_ANTONY='STIF:StopPoint:Q:…:' cargo run -- run
```

Expected: the presence line, then `Rungis: … ; Antony: …` with two times
each (or a moon), `Display redrawn (full): target/e-paper.png`. Open
`target/e-paper.png`: the phase 2 screen. Run the same command again in the
same minute: no PRIM poll, `No change: display not touched`. A minute later:
`Display redrawn (partial)` (the clock).

## 7. Deploy on the Pi

**Not before the end of the `bus-log` observation** (Monday 19 October in
the evening): together they exceed the PRIM quota.

1. Stop `bus-log` and fetch its logs:

```bash
ssh epaper sudo systemctl stop bus-log.timer
```

```bash
scp -r epaper:bus-log .
```

2. The PRIM key, already on the Pi in `~/secrets/`, goes next to the Freebox token:

```bash
ssh epaper 'sudo install -m 600 secrets/prim_api_key /etc/e-paper/prim_api_key'
```

3. The home stops: create `/etc/e-paper/prim.env` on the Pi

```bash
ssh -t epaper 'sudo nano /etc/e-paper/prim.env'
```

with these two lines, your real references in place of `…`:

```ini
PRIM_STOP_RUNGIS=STIF:StopPoint:Q:…:
PRIM_STOP_ANTONY=STIF:StopPoint:Q:…:
```

then make it readable by root only:

```bash
ssh epaper 'sudo chmod 600 /etc/e-paper/prim.env'
```

4. The usage log (guide 10) overrides `ExecStart` in a drop-in, which would
   hide the new `--prim-key`. If its week is over, fetch `usage.csv` (guide
   10, "After a week") and remove the drop-in:

```bash
ssh epaper 'sudo systemctl revert e-paper.service'
```

   If you keep it, edit it (`ssh -t epaper 'sudo systemctl edit e-paper.service'`) and add `--prim-key %d/prim_api_key` to its `ExecStart`.

5. Deploy and watch:

```bash
make deploy
```

```bash
make logs
```

Expected on the panel: a full refresh with the phase 2 screen, then each
minute a partial one (the clock), without flashing; a full refresh on the
first cycle of each hour. In the logs: `Rungis: …; Antony: …` each minute,
a PRIM poll every 5 or 15 minutes. A `warning: PRIM …` line means a failed
poll: the wrench shows after two in a row.

## 8. Checks and commit

```bash
cargo zigbuild --release --target aarch64-unknown-linux-musl
```

```bash
git add -A
```

```bash
git status
```

Expected files: `Cargo.toml`, `Cargo.lock`, `src/bus.rs`, `src/lib.rs`,
`src/update.rs`, `src/departures.rs`, `src/saved.rs`, `src/cycle.rs`,
`src/usage.rs`, `src/main.rs`, `systemd/e-paper.service`,
`docs/guide-14-phase2-cycle.md`. Never `prim.env`, `bus-log/`, `secrets/`.

```bash
git commit -m "feat: phase 2 cycle: PRIM polling, Paris clock, frame-based refresh"
```

## Report

The output of sections 5 and 6, the logs after deployment, and how the
panel looks after an hour (ghosting).
