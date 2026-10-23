# Guide 12: PRIM departures (phase 2)

Turns a PRIM `stop-monitoring` answer into what a direction block shows:
the next two catchable departures, dashes, or the moon (departure selection
by line and destination, 5-minute filter, no-departure pictograms). Pure
functions, no network, no display: the phase 2 cycle (polling, Paris time,
drawing, wrench on API failure) comes in the next guide.

| Item | Content | Tests |
|---|---|---|
| `jiff` (new dependency) | reads the ISO 8601 times of PRIM; Paris time later | |
| `tests/fixtures/prim/` (new) | six real answers from the `bus-log` logs | |
| `departures` (new module) | `parse` an answer, `next` departures of a direction | A (1–8), B (9–16) |

Design:

- Each home pole serves one direction: the cycle reads the Rungis pole's
  answer with `Direction::Rungis` and the Antony pole's with
  `Direction::Antony`.
- `parse(body, direction)` keeps the departures whose (line, destination)
  pair belongs to the direction (319 → Marché International de Rungis;
  297 and 401 → their own "Gare d'Antony" stop), and the service status
  (`afterLastJourney`, `beforeFirstJourney`) of the direction's lines only.
  The time is `ExpectedDepartureTime`, else `AimedDepartureTime`; a
  departure without either is skipped.
- `next(answer, now, min_lead)` drops the departures less than `min_lead`
  (5 minutes) away, bound included, sorts the rest, keeps two. None left
  and a line out of service: `Next::ServiceOver` (the moon). None left and
  no status: `Next::Times` empty (dashes).
- Times stay UTC instants (`jiff::Timestamp`). They become Paris times, with
  daylight saving, in the cycle guide.

Approved tests: 16 (2026-10-14). Approved choices: `jiff`; fixtures created
by Claude with fake home stop references; a departure exactly 5 minutes
away is shown. Changed while writing: test 16 checks that a status counts
only for the direction of its pole (at 04:10 the Antony pole also reports
the 319 to Massy out of service; it must not give the moon).

Checked while writing this guide, on a copy of the project: red then green
step by step, 173 library tests, clippy clean, cross-build for the Pi fine.

Run every command from the project root; create files in VS Code.

## 1. Dependency and fixtures

```bash
cargo add jiff
```

The fixtures are already in `tests/fixtures/prim/`, created by Claude from
the logs (the SIRI answer only, pretty-printed). The references of the two
home poles are replaced by `STIF:StopPoint:Q:00001:` (Antony) and
`STIF:StopPoint:Q:00002:` (Rungis); the destination terminals, public, are
kept.

| File | Pole, local time | Used for |
|---|---|---|
| `antony-20261014-0800.json` | Antony, peak | 297 and 401 to Antony, 319 to Massy |
| `antony-20261013-1530.json` | Antony, afternoon | 401 to Villemilan only |
| `antony-20261014-0410.json` | Antony, night | 297 `beforeFirstJourney`, 401 announced |
| `rungis-20261013-2040.json` | Rungis, evening | 319 `afterLastJourney`, no departure |
| `rungis-20261014-0645.json` | Rungis, morning | the "Avenue Saint-Marc" data error |
| `rungis-20261014-0730.json` | Rungis, morning | a departure already gone |

Check that only the fake references are there (each file prints a count
above 0):

```bash
grep -c "STIF:StopPoint:Q:0000[12]:" tests/fixtures/prim/*.json
```

## 2. Red: the tests (1–16)

### What the tests check

**A. Reading an answer** (real fixtures)

1. `peak_answer_gives_the_297_and_401_departures_to_antony`: at 08:00 the
   Antony pole gives 2 departures of the 297 and 13 of the 401, nothing else
   (the 319 to Massy is dropped).
2. `departures_to_massy_and_villemilan_are_dropped`: at 15:30 only the two
   297 departures are left.
3. `departure_to_avenue_saint_marc_is_dropped`: the 319 "Avenue Saint-Marc"
   departure at the Rungis pole is dropped.
4. `times_are_utc_instants`: `2026-10-14T04:58:36.000Z` is read as that
   exact instant.
5. `aimed_time_is_used_without_expected_time_and_no_time_is_skipped`: a 401
   with only an aimed time is kept at that time; a 297 with no time at all is
   skipped (JSON written in the test).
6. `end_of_service_gives_no_departure_and_the_status`: Rungis at 20:40, no
   departure, the 319 in `AfterLastJourney`.
7. `before_the_first_297_the_401_is_already_announced`: Antony at 04:10, the
   297 in `BeforeFirstJourney`, six 401 departures.
8. `unexpected_body_is_an_error`: an HTML page, or JSON without
   `ServiceDelivery`, gives an error.

**B. Next departures of a direction**

9. `departures_are_sorted_whatever_the_line`: a 401 before a 297 comes first.
10. `departure_within_five_minutes_is_hidden_bound_included`: at 06:00, a
    departure at 06:04:59 is hidden, one at 06:05:00 is shown.
11. `departure_already_gone_is_hidden`: Rungis at 07:30 lists a departure
    16 s in the past; only the next one is shown.
12. `at_most_two_departures_are_kept`: at the 08:00 peak, only 08:07 (297)
    and 08:13 (401) out of 15.
13. `no_departure_and_service_over_shows_the_moon`: Rungis at 20:40 gives
    `ServiceOver`.
14. `no_departure_without_status_shows_dashes`: a single departure 2 minutes
    away, no status: `Times` empty.
15. `a_catchable_401_wins_over_the_297_out_of_service`: the 297 is over but a
    401 leaves in 12 minutes: its time, not the moon.
16. `a_status_counts_only_for_the_direction_of_its_pole`: at 04:10 the Antony
    pole also reports the 319 (to Massy) as over; only the 297 status is kept.

### Code

Create the file `src/departures.rs` with this content:

```rust
//! Phase 2 departures: from a PRIM `stop-monitoring` answer to the next
//! catchable departures of a direction, without any network access.
//!
//! Times stay UTC instants here; the cycle turns them into Paris times.
//! Selection rules: guide 12.

use jiff::{SignedDuration, Timestamp};

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

/// Reads the answer of the pole serving `direction`: the selected
/// departures, and the service status of the direction's lines.
pub fn parse(body: &str, direction: Direction) -> Result<Answer, String> {
    todo!()
}

/// The next two catchable departures (at least `min_lead` from `now`,
/// bound included), or the moon when there is none and one of the
/// direction's lines is out of service.
pub fn next(answer: &Answer, now: Timestamp, min_lead: SignedDuration) -> Next {
    todo!()
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
}
```

In `src/lib.rs`, after `pub mod cycle;`, add:

```rust
pub mod departures;
```

```bash
cargo test --lib departures::
```

Expected: it compiles with 5 warnings `unused variable` (the parameters of
the two `todo!()`), and the 16 tests fail with `not yet implemented`.

## 3. Green

In `src/departures.rs`, after `use jiff::{SignedDuration, Timestamp};`, add:

```rust
use serde::Deserialize;
```

After the `Next` enum, add the selection and the SIRI fields we read:

```rust
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
```

In `parse`, replace `todo!()` with:

```rust
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
```

In `next`, replace `todo!()` with:

```rust
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
```

```bash
cargo test --lib departures::
```

Expected: `16 passed`, no warning.

## 4. Checks

```bash
cargo fmt
```

```bash
cargo test
```

```bash
cargo clippy --all-targets -- -D warnings
```

```bash
cargo zigbuild --release --target aarch64-unknown-linux-musl
```

Expected: `173 passed` for the library, `2 passed` for the golden images, no
clippy warning, and the Pi build succeeds (only the known linker warning).
Nothing to deploy: no command uses the module yet.

## 5. Commit

```bash
git add -A
```

```bash
git status
```

Expected files: `Cargo.toml`, `Cargo.lock`, `src/lib.rs`,
`src/departures.rs`, the six `tests/fixtures/prim/*.json`,
`docs/guide-12-prim-departures.md`. Never `bus-log/` nor `secrets/`.

```bash
git commit -m "feat: phase 2 departures from PRIM stop-monitoring answers"
```

## Report

The output of section 4.
