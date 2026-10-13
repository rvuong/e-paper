//! Usage log (dev feature): each assigned device's download at every cycle,
//! as CSV lines, to check the activity threshold against real usage. Device
//! names only, never MAC addresses. Never commit the log.

use crate::cycle::CycleInput;
use crate::rate::counter_delta;
use crate::refresh::HouseholdState;
use crate::state::PersonState;

/// First line of a new log file.
pub const HEADER: &str = "timestamp,elapsed_secs,person,device,home,delta_bytes,counter";

/// One line per assigned device seen by the Freebox during this cycle.
///
/// - `elapsed_secs`: time since the previous run, empty on the first run.
/// - `delta_bytes`: download since the previous run, empty when the device has
///   no previous reading (the counter alone would show a fake peak).
/// - `counter`: the raw counter, to spot resets afterwards.
pub fn usage_lines(input: &CycleInput, household: &HouseholdState) -> Vec<String> {
    let elapsed = input
        .previous
        .map(|previous| (input.now - previous.polled_at).to_string())
        .unwrap_or_default();
    input
        .mapping
        .iter()
        .filter_map(|(mac, assignment)| {
            let counter = *input.counters.get(mac)?;
            let delta = input
                .previous
                .and_then(|previous| previous.counters.get(mac))
                .map(|&before| counter_delta(Some(before), counter).to_string())
                .unwrap_or_default();
            let home = household
                .get(&assignment.person)
                .is_some_and(|&state| state != PersonState::Away);
            Some(format!(
                "{},{elapsed},{},{},{home},{delta},{counter}",
                input.now,
                assignment.person.0,
                csv_field(&assignment.name),
            ))
        })
        .collect()
}

/// A CSV field, quoted when it holds a comma, a quote or a line break.
fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::devices::{Assignment, Mapping};
    use crate::layout::PEOPLE;
    use crate::saved::SavedState;
    use crate::state::Person;
    use crate::state::PersonState::{Away, HomeActive, HomePassive};

    const NOW: i64 = 1_760_000_000;
    const R_PHONE: &str = "02:00:00:00:00:01";
    const R_LAPTOP: &str = "02:00:00:00:00:02";
    const H_CONSOLE: &str = "02:00:00:00:00:03";
    const UNMAPPED_TV: &str = "02:00:00:00:00:04";

    fn mapping() -> Mapping {
        let assign = |person, name: &str, smartphone| Assignment {
            person: Person(person),
            name: name.into(),
            smartphone,
        };
        [
            (R_PHONE, assign('R', "Pixel R", true)),
            (R_LAPTOP, assign('R', "Laptop, R", false)),
            (H_CONSOLE, assign('H', "PS5", false)),
        ]
        .into_iter()
        .map(|(mac, assignment)| (mac.to_string(), assignment))
        .collect()
    }

    fn counters(values: &[(&str, u64)]) -> BTreeMap<String, u64> {
        values
            .iter()
            .map(|&(mac, counter)| (mac.to_string(), counter))
            .collect()
    }

    /// A previous run 60 s ago with these counters.
    fn previous(values: &[(&str, u64)]) -> SavedState {
        SavedState {
            polled_at: NOW - 60,
            household: PEOPLE.into_iter().map(|person| (person, Away)).collect(),
            counters: counters(values),
            last_full_refresh: Some(NOW - 60),
            partials_since_full: 0,
            earlier: Vec::new(),
        }
    }

    /// R home and active, everybody else away.
    fn household() -> HouseholdState {
        PEOPLE
            .into_iter()
            .map(|person| {
                let state = if person == Person('R') {
                    HomeActive
                } else {
                    Away
                };
                (person, state)
            })
            .collect()
    }

    fn lines(
        now_counters: &[(&str, u64)],
        previous: Option<&SavedState>,
        household: &HouseholdState,
    ) -> Vec<String> {
        let mapping = mapping();
        let counters = counters(now_counters);
        usage_lines(
            &CycleInput {
                now: NOW,
                interval_secs: 300,
                threshold_bps: 5_000_000,
                mapping: &mapping,
                hosts: &[],
                counters: &counters,
                previous,
                previous_frame: true,
                force: false,
                partial_enabled: true,
            },
            household,
        )
    }

    #[test]
    fn one_line_per_assigned_device_seen() {
        let previous = previous(&[]);
        let lines = lines(
            &[(R_PHONE, 10), (H_CONSOLE, 20), (UNMAPPED_TV, 30)],
            Some(&previous),
            &household(),
        );
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Pixel R"));
        assert!(lines[1].contains("PS5"));
    }

    #[test]
    fn delta_is_measured_since_the_previous_run() {
        let previous = previous(&[(H_CONSOLE, 1_000)]);
        let lines = lines(&[(H_CONSOLE, 4_000)], Some(&previous), &household());
        assert_eq!(lines, [format!("{NOW},60,H,PS5,false,3000,4000")]);
    }

    #[test]
    fn counter_reset_gives_the_current_counter() {
        let previous = previous(&[(H_CONSOLE, 9_000)]);
        let lines = lines(&[(H_CONSOLE, 500)], Some(&previous), &household());
        assert_eq!(lines, [format!("{NOW},60,H,PS5,false,500,500")]);
    }

    #[test]
    fn no_previous_reading_leaves_the_delta_empty() {
        let previous = previous(&[]);
        let lines = lines(&[(H_CONSOLE, 4_000)], Some(&previous), &household());
        assert_eq!(lines, [format!("{NOW},60,H,PS5,false,,4000")]);
    }

    #[test]
    fn home_column_follows_presence() {
        let mut household = household();
        let previous = previous(&[(R_PHONE, 0), (R_LAPTOP, 0), (H_CONSOLE, 0)]);
        let now = [(R_PHONE, 1), (R_LAPTOP, 2), (H_CONSOLE, 3)];
        // Lines in MAC order: R phone, R laptop, H console. The column is read
        // from the end: the laptop's quoted name holds a comma.
        let homes_from_end = |lines: Vec<String>| -> Vec<String> {
            lines
                .iter()
                .map(|line| line.rsplit(',').nth(2).unwrap().to_string())
                .collect()
        };
        assert_eq!(
            homes_from_end(lines(&now, Some(&previous), &household)),
            ["true", "true", "false"]
        );
        household.insert(Person('R'), HomePassive);
        household.insert(Person('H'), HomePassive);
        assert_eq!(
            homes_from_end(lines(&now, Some(&previous), &household)),
            ["true", "true", "true"]
        );
    }

    #[test]
    fn elapsed_is_the_gap_since_the_previous_run() {
        let lines_first = lines(&[(H_CONSOLE, 4_000)], None, &household());
        assert_eq!(lines_first, [format!("{NOW},,H,PS5,false,,4000")]);
        let mut previous = previous(&[(H_CONSOLE, 1_000)]);
        previous.polled_at = NOW - 3_600;
        let lines_after_downtime = lines(&[(H_CONSOLE, 4_000)], Some(&previous), &household());
        assert_eq!(
            lines_after_downtime,
            [format!("{NOW},3600,H,PS5,false,3000,4000")]
        );
    }

    #[test]
    fn csv_header_and_quoting() {
        assert_eq!(
            HEADER,
            "timestamp,elapsed_secs,person,device,home,delta_bytes,counter"
        );
        let previous = previous(&[(R_LAPTOP, 0)]);
        let lines = lines(&[(R_LAPTOP, 8)], Some(&previous), &household());
        assert_eq!(lines, [format!("{NOW},60,R,\"Laptop, R\",true,8,8")]);
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("PS5"), "PS5");
    }
}
