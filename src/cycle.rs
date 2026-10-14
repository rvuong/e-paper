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
