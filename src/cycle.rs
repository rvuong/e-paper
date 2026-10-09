//! One cycle: from the Freebox data and the previous state to what the display
//! shows. Pure: the caller polls the Freebox, reads the clock and the files.

use std::collections::BTreeMap;

use crate::devices::Mapping;
use crate::freebox::Host;
use crate::layout::PEOPLE;
use crate::presence::is_home;
use crate::rate::{DeviceRate, counter_delta, person_rate_bps, rate_bps};
use crate::refresh::{HouseholdState, needs_refresh};
use crate::saved::SavedState;
use crate::state::{Person, person_state};

/// Everything a cycle needs.
pub struct CycleInput<'a> {
    /// Unix timestamp in seconds.
    pub now: i64,
    /// Presence window: the smartphone must have been active this recently.
    pub interval_secs: i64,
    pub threshold_bps: u64,
    pub mapping: &'a Mapping,
    /// Devices known to the Freebox (LAN browser), for the last activity.
    pub hosts: &'a [Host],
    /// Download counter of every Wi-Fi station, by MAC.
    pub counters: &'a BTreeMap<String, u64>,
    /// State left by the previous run, if any.
    pub previous: Option<&'a SavedState>,
    /// Redraw even if nothing changed.
    pub force: bool,
}

/// What a cycle decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleOutput {
    pub household: HouseholdState,
    /// Whether the display must be redrawn.
    pub refresh: bool,
    /// State to save for the next run.
    pub saved: SavedState,
}

pub fn cycle(input: &CycleInput) -> CycleOutput {
    let rates = device_rates(input);
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
    let refresh = input.force
        || needs_refresh(
            input.previous.map(|previous| &previous.household),
            &household,
        );
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
        },
        household,
        refresh,
    }
}

/// Rate of each assigned device seen during this poll. All zero on the first
/// run, as there is no previous poll to measure from.
fn device_rates(input: &CycleInput) -> Vec<DeviceRate> {
    let Some(previous) = input.previous else {
        return Vec::new();
    };
    let elapsed = input.now - previous.polled_at;
    input
        .mapping
        .iter()
        .filter_map(|(mac, assignment)| {
            let current = *input.counters.get(mac)?;
            let delta = counter_delta(previous.counters.get(mac).copied(), current);
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
        }
    }

    fn run(
        hosts: &[Host],
        now_counters: &[(&str, u64)],
        previous: Option<&SavedState>,
        force: bool,
    ) -> CycleOutput {
        cycle(&CycleInput {
            now: NOW,
            interval_secs: INTERVAL,
            threshold_bps: THRESHOLD,
            mapping: &mapping(),
            hosts,
            counters: &counters(now_counters),
            previous,
            force,
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
        let output = run(&[host(R_PHONE, NOW - 120)], &[], None, false);
        assert_eq!(state_of(&output, 'R'), HomePassive);
    }

    #[test]
    fn person_without_smartphone_is_away() {
        let output = run(&[host(R_PHONE, NOW - 120)], &[], None, false);
        assert_eq!(state_of(&output, 'M'), Away);
    }

    #[test]
    fn smartphone_unknown_to_the_freebox_is_away() {
        let output = run(&[host(R_PHONE, NOW - 120)], &[], None, false);
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
            false,
        );
        assert_eq!(state_of(&output, 'R'), HomeActive);
    }

    #[test]
    fn first_run_has_zero_rates() {
        let output = run(
            &[host(R_PHONE, NOW - 60)],
            &[(R_LAPTOP, 50_000_000_000)],
            None,
            false,
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
            false,
        );
        assert_eq!(state_of(&output, 'H'), HomeActive);
        // The unmapped TV counts for nobody: R stays passive.
        assert_eq!(state_of(&output, 'R'), HomePassive);
    }

    #[test]
    fn heavy_download_with_smartphone_away_is_away() {
        let before = previous(&[(R_LAPTOP, 0)]);
        let output = run(
            &[host(R_PHONE, NOW - 3_600)],
            &[(R_LAPTOP, SIX_MBPS)],
            Some(&before),
            false,
        );
        assert_eq!(state_of(&output, 'R'), Away);
    }

    #[test]
    fn first_run_refreshes() {
        assert!(run(&[], &[], None, false).refresh);
    }

    #[test]
    fn unchanged_state_does_not_refresh_unless_forced() {
        let before = previous(&[]);
        assert!(!run(&[], &[], Some(&before), false).refresh);
        assert!(run(&[], &[], Some(&before), true).refresh);
    }

    #[test]
    fn saved_counters_are_the_assigned_devices_seen_with_the_poll_time() {
        let output = run(
            &[],
            &[(R_LAPTOP, 42), (H_CONSOLE, 7), (UNMAPPED_TV, 99)],
            None,
            false,
        );
        assert_eq!(output.saved.polled_at, NOW);
        assert_eq!(
            output.saved.counters,
            counters(&[(R_LAPTOP, 42), (H_CONSOLE, 7)])
        );
        assert_eq!(output.saved.household, output.household);
    }
}
