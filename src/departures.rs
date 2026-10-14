//! Phase 2 departures: from a PRIM `stop-monitoring` answer to the next
//! catchable departures of a direction, without any network access.
//!
//! Times are UTC instants until `block` turns them into Paris times.
//! See CLAUDE.md, "Phase 2".

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;

use crate::board::{Block, Time};

/// A departure less than this away is hidden: time to reach the stop.
pub const MIN_LEAD: SignedDuration = SignedDuration::from_mins(5);

/// After this many failed polls in a row, the block shows the wrench.
pub const FAILURES_BEFORE_WRENCH: u32 = 2;

/// A block of the board. Each home pole serves one direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Rungis,
    Antony,
}

/// The lines, as PRIM names them (`LineRef`).
pub const LINE_297: &str = "STIF:Line::C01270:";
pub const LINE_319: &str = "STIF:Line::C01282:";
pub const LINE_401: &str = "STIF:Line::C00339:";

/// Service status of a line at a pole, when it has no departure to give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    AfterLastJourney,
    BeforeFirstJourney,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Departure {
    pub line: String,
    pub at: Timestamp,
}

/// What one answer says about its direction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    pub departures: Vec<Departure>,
    pub statuses: Vec<(String, ServiceStatus)>,
}

/// What a direction shows: up to two times (dashes for the missing ones),
/// or the moon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    Times(Vec<Timestamp>),
    ServiceOver,
}

/// The (line, destination) pairs kept for each direction; every other
/// departure is dropped (319 to Massy, 401 to Villemilan, data errors).
const SELECTION: [(Direction, &str, &str); 3] = [
    (Direction::Rungis, LINE_319, "STIF:StopPoint:Q:415870:"),
    (Direction::Antony, LINE_297, "STIF:StopPoint:Q:20422:"),
    (Direction::Antony, LINE_401, "STIF:StopPoint:Q:12070:"),
];

// The few SIRI Lite fields we read. SIRI wraps every value in
// `{"value": …}`.

#[derive(Deserialize)]
struct Wrapped<T> {
    value: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Body {
    siri: Siri,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Siri {
    service_delivery: ServiceDelivery,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServiceDelivery {
    stop_monitoring_delivery: Vec<StopMonitoringDelivery>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StopMonitoringDelivery {
    #[serde(default)]
    monitored_stop_visit: Vec<Visit>,
    #[serde(default)]
    service_exception: Vec<ServiceException>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Visit {
    monitored_vehicle_journey: Journey,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Journey {
    line_ref: Wrapped<String>,
    destination_ref: Option<Wrapped<String>>,
    monitored_call: Call,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Call {
    expected_departure_time: Option<String>,
    aimed_departure_time: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServiceException {
    line_ref: Wrapped<String>,
    service_status: Wrapped<String>,
}

fn selected(direction: Direction, line: &str, destination: &str) -> bool {
    SELECTION
        .iter()
        .any(|&(d, l, dest)| d == direction && l == line && dest == destination)
}

fn serves(direction: Direction, line: &str) -> bool {
    SELECTION
        .iter()
        .any(|&(d, l, _)| d == direction && l == line)
}

/// Reads the answer of the pole serving `direction`: the selected
/// departures, and the service status of the direction's lines.
pub fn parse(body: &str, direction: Direction) -> Result<Answer, String> {
    let body: Body = serde_json::from_str(body).map_err(|e| format!("unexpected answer: {e}"))?;
    let mut answer = Answer::default();
    for delivery in body.siri.service_delivery.stop_monitoring_delivery {
        for visit in delivery.monitored_stop_visit {
            let journey = visit.monitored_vehicle_journey;
            let Some(destination) = journey.destination_ref else {
                continue;
            };
            if !selected(direction, &journey.line_ref.value, &destination.value) {
                continue;
            }
            let call = journey.monitored_call;
            let time = call.expected_departure_time.or(call.aimed_departure_time);
            let Some(at) = time.and_then(|t| t.parse::<Timestamp>().ok()) else {
                continue;
            };
            answer.departures.push(Departure {
                line: journey.line_ref.value,
                at,
            });
        }
        for exception in delivery.service_exception {
            let status = match exception.service_status.value.as_str() {
                "afterLastJourney" => ServiceStatus::AfterLastJourney,
                "beforeFirstJourney" => ServiceStatus::BeforeFirstJourney,
                _ => continue,
            };
            if serves(direction, &exception.line_ref.value) {
                answer.statuses.push((exception.line_ref.value, status));
            }
        }
    }
    Ok(answer)
}

/// The next two catchable departures (at least `min_lead` from `now`,
/// bound included), or the moon when there is none and one of the
/// direction's lines is out of service.
pub fn next(answer: &Answer, now: Timestamp, min_lead: SignedDuration) -> Next {
    let earliest = now + min_lead;
    let mut times: Vec<Timestamp> = answer
        .departures
        .iter()
        .map(|d| d.at)
        .filter(|&at| at >= earliest)
        .collect();
    times.sort();
    times.truncate(2);
    if times.is_empty() && !answer.statuses.is_empty() {
        Next::ServiceOver
    } else {
        Next::Times(times)
    }
}

/// The time of day in Paris, daylight saving included, truncated to the
/// minute: a bus is never shown later than it leaves.
pub fn paris_time(at: Timestamp) -> Time {
    let paris = TimeZone::get("Europe/Paris").expect("time zone database bundled with jiff");
    let local = at.to_zoned(paris);
    Time {
        hour: local.hour() as u8,
        minute: local.minute() as u8,
    }
}

/// What a direction block shows, from the last answer of its pole and the
/// number of failed polls since.
pub fn block(answer: &Answer, failures: u32, now: Timestamp) -> Block {
    if failures >= FAILURES_BEFORE_WRENCH {
        return Block::Wrench;
    }
    match next(answer, now, MIN_LEAD) {
        Next::ServiceOver => Block::Moon,
        Next::Times(times) => Block::Times(times.into_iter().map(paris_time).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANTONY_PEAK: &str = include_str!("../tests/fixtures/prim/antony-20261014-0800.json");
    const ANTONY_AFTERNOON: &str = include_str!("../tests/fixtures/prim/antony-20261013-1530.json");
    const ANTONY_BEFORE_FIRST: &str =
        include_str!("../tests/fixtures/prim/antony-20261014-0410.json");
    const RUNGIS_EVENING: &str = include_str!("../tests/fixtures/prim/rungis-20261013-2040.json");
    const RUNGIS_SAINT_MARC: &str =
        include_str!("../tests/fixtures/prim/rungis-20261014-0645.json");
    const RUNGIS_GONE: &str = include_str!("../tests/fixtures/prim/rungis-20261014-0730.json");

    const FIVE_MINUTES: SignedDuration = SignedDuration::from_mins(5);

    fn at(time: &str) -> Timestamp {
        time.parse().unwrap()
    }

    fn lines(answer: &Answer) -> Vec<&str> {
        answer.departures.iter().map(|d| d.line.as_str()).collect()
    }

    fn departures(times: &[(&str, &str)]) -> Answer {
        Answer {
            departures: times
                .iter()
                .map(|&(line, time)| Departure {
                    line: line.into(),
                    at: at(time),
                })
                .collect(),
            statuses: vec![],
        }
    }

    // A. Reading an answer

    #[test]
    fn peak_answer_gives_the_297_and_401_departures_to_antony() {
        let answer = parse(ANTONY_PEAK, Direction::Antony).unwrap();
        let lines = lines(&answer);
        assert_eq!(lines.iter().filter(|&&l| l == LINE_297).count(), 2);
        assert_eq!(lines.iter().filter(|&&l| l == LINE_401).count(), 13);
        assert_eq!(lines.len(), 15);
    }

    #[test]
    fn departures_to_massy_and_villemilan_are_dropped() {
        let answer = parse(ANTONY_AFTERNOON, Direction::Antony).unwrap();
        assert_eq!(lines(&answer), [LINE_297, LINE_297]);
    }

    #[test]
    fn departure_to_avenue_saint_marc_is_dropped() {
        let answer = parse(RUNGIS_SAINT_MARC, Direction::Rungis).unwrap();
        assert_eq!(lines(&answer), [LINE_319]);
    }

    #[test]
    fn times_are_utc_instants() {
        let answer = parse(RUNGIS_SAINT_MARC, Direction::Rungis).unwrap();
        assert_eq!(answer.departures[0].at, at("2026-10-14T04:58:36Z"));
    }

    #[test]
    fn aimed_time_is_used_without_expected_time_and_no_time_is_skipped() {
        let body = r#"{"Siri": {"ServiceDelivery": {"StopMonitoringDelivery": [{
            "MonitoredStopVisit": [
                {"MonitoredVehicleJourney": {
                    "LineRef": {"value": "STIF:Line::C00339:"},
                    "DestinationRef": {"value": "STIF:StopPoint:Q:12070:"},
                    "MonitoredCall": {"AimedDepartureTime": "2026-10-14T06:13:00.000Z"}}},
                {"MonitoredVehicleJourney": {
                    "LineRef": {"value": "STIF:Line::C01270:"},
                    "DestinationRef": {"value": "STIF:StopPoint:Q:20422:"},
                    "MonitoredCall": {}}}
            ]}]}}}"#;
        let answer = parse(body, Direction::Antony).unwrap();
        assert_eq!(
            answer.departures,
            [Departure {
                line: LINE_401.into(),
                at: at("2026-10-14T06:13:00Z"),
            }]
        );
    }

    #[test]
    fn end_of_service_gives_no_departure_and_the_status() {
        let answer = parse(RUNGIS_EVENING, Direction::Rungis).unwrap();
        assert!(answer.departures.is_empty());
        assert_eq!(
            answer.statuses,
            [(LINE_319.to_string(), ServiceStatus::AfterLastJourney)]
        );
    }

    #[test]
    fn before_the_first_297_the_401_is_already_announced() {
        let answer = parse(ANTONY_BEFORE_FIRST, Direction::Antony).unwrap();
        assert!(
            answer
                .statuses
                .contains(&(LINE_297.to_string(), ServiceStatus::BeforeFirstJourney))
        );
        assert_eq!(lines(&answer), [LINE_401; 6]);
    }

    #[test]
    fn unexpected_body_is_an_error() {
        assert!(parse("<html>Bad Gateway</html>", Direction::Rungis).is_err());
        assert!(parse(r#"{"Siri": {}}"#, Direction::Rungis).is_err());
    }

    // B. Next departures of a direction

    #[test]
    fn departures_are_sorted_whatever_the_line() {
        let answer = departures(&[
            (LINE_297, "2026-10-14T06:20:43Z"),
            (LINE_401, "2026-10-14T06:13:00Z"),
        ]);
        assert_eq!(
            next(&answer, at("2026-10-14T06:00:00Z"), FIVE_MINUTES),
            Next::Times(vec![at("2026-10-14T06:13:00Z"), at("2026-10-14T06:20:43Z")])
        );
    }

    #[test]
    fn departure_within_five_minutes_is_hidden_bound_included() {
        let answer = departures(&[
            (LINE_319, "2026-10-14T06:04:59Z"),
            (LINE_319, "2026-10-14T06:05:00Z"),
        ]);
        assert_eq!(
            next(&answer, at("2026-10-14T06:00:00Z"), FIVE_MINUTES),
            Next::Times(vec![at("2026-10-14T06:05:00Z")])
        );
    }

    #[test]
    fn departure_already_gone_is_hidden() {
        let answer = parse(RUNGIS_GONE, Direction::Rungis).unwrap();
        assert_eq!(
            next(&answer, at("2026-10-14T05:30:00Z"), FIVE_MINUTES),
            Next::Times(vec![at("2026-10-14T05:36:22Z")])
        );
    }

    #[test]
    fn at_most_two_departures_are_kept() {
        let answer = parse(ANTONY_PEAK, Direction::Antony).unwrap();
        assert_eq!(
            next(&answer, at("2026-10-14T06:00:00Z"), FIVE_MINUTES),
            Next::Times(vec![at("2026-10-14T06:07:19Z"), at("2026-10-14T06:13:00Z")])
        );
    }

    #[test]
    fn no_departure_and_service_over_shows_the_moon() {
        let answer = parse(RUNGIS_EVENING, Direction::Rungis).unwrap();
        assert_eq!(
            next(&answer, at("2026-10-13T18:40:00Z"), FIVE_MINUTES),
            Next::ServiceOver
        );
    }

    #[test]
    fn no_departure_without_status_shows_dashes() {
        let answer = departures(&[(LINE_319, "2026-10-14T06:02:00Z")]);
        assert_eq!(
            next(&answer, at("2026-10-14T06:00:00Z"), FIVE_MINUTES),
            Next::Times(vec![])
        );
    }

    #[test]
    fn a_catchable_401_wins_over_the_297_out_of_service() {
        let answer = Answer {
            statuses: vec![(LINE_297.into(), ServiceStatus::AfterLastJourney)],
            ..departures(&[(LINE_401, "2026-10-14T19:12:00Z")])
        };
        assert_eq!(
            next(&answer, at("2026-10-14T19:00:00Z"), FIVE_MINUTES),
            Next::Times(vec![at("2026-10-14T19:12:00Z")])
        );
    }

    #[test]
    fn a_status_counts_only_for_the_direction_of_its_pole() {
        // At the Antony pole, the 319 (to Massy) is out of service too: it
        // is not an Antony line, so it must not give the moon.
        let answer = parse(ANTONY_BEFORE_FIRST, Direction::Antony).unwrap();
        assert_eq!(
            answer.statuses,
            [(LINE_297.to_string(), ServiceStatus::BeforeFirstJourney)]
        );
    }

    // C. Paris time

    fn hm(hour: u8, minute: u8) -> Time {
        Time { hour, minute }
    }

    #[test]
    fn paris_time_is_truncated_to_the_minute() {
        assert_eq!(paris_time(at("2026-10-14T06:08:36Z")), hm(8, 8));
    }

    #[test]
    fn paris_time_follows_daylight_saving() {
        // Summer time ends on 2026-10-25 at 01:00 UTC (03:00 becomes 02:00).
        assert_eq!(paris_time(at("2026-10-25T00:30:00Z")), hm(2, 30));
        assert_eq!(paris_time(at("2026-10-25T01:30:00Z")), hm(2, 30));
        assert_eq!(paris_time(at("2026-12-01T12:00:00Z")), hm(13, 0));
    }

    // D. Block of a direction

    #[test]
    fn next_departures_become_paris_times() {
        let answer = departures(&[
            (LINE_297, "2026-10-14T06:20:43Z"),
            (LINE_401, "2026-10-14T06:13:00Z"),
        ]);
        assert_eq!(
            block(&answer, 0, at("2026-10-14T06:00:00Z")),
            Block::Times(vec![hm(8, 13), hm(8, 20)])
        );
    }

    #[test]
    fn service_over_gives_the_moon() {
        let answer = parse(RUNGIS_EVENING, Direction::Rungis).unwrap();
        assert_eq!(block(&answer, 0, at("2026-10-13T18:40:00Z")), Block::Moon);
    }

    #[test]
    fn two_failed_polls_give_the_wrench() {
        let answer = departures(&[(LINE_319, "2026-10-14T06:13:00Z")]);
        let now = at("2026-10-14T06:00:00Z");
        let times = Block::Times(vec![hm(8, 13)]);
        assert_eq!(block(&answer, 0, now), times);
        assert_eq!(block(&answer, 1, now), times);
        assert_eq!(block(&answer, 2, now), Block::Wrench);
    }
}
