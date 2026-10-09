//! Download rates computed from the Freebox byte counters.

use crate::state::Person;

/// Bytes received by a device between two polls.
///
/// The counter restarts at 0 when the device reconnects, so a counter lower
/// than the previous value is taken as the delta itself. A device without a
/// previous value starts from 0 the same way. Never negative.
pub fn counter_delta(previous: Option<u64>, current: u64) -> u64 {
    match previous {
        Some(previous) if current >= previous => current - previous,
        _ => current,
    }
}

/// Download rate in bits per second, 0 when the elapsed time is not positive.
pub fn rate_bps(delta_bytes: u64, elapsed_secs: i64) -> u64 {
    match u64::try_from(elapsed_secs) {
        Ok(elapsed) if elapsed > 0 => delta_bytes * 8 / elapsed,
        _ => 0,
    }
}

/// Download rate of one device during the last interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceRate {
    /// `None` for shared devices, which count for nobody.
    pub owner: Option<Person>,
    pub bps: u64,
}

/// Sum of the rates of the person's devices seen during the last interval.
pub fn person_rate_bps(person: Person, devices: &[DeviceRate]) -> u64 {
    devices
        .iter()
        .filter(|device| device.owner == Some(person))
        .map(|device| device.bps)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Person = Person('R');
    const H: Person = Person('H');

    #[test]
    fn counter_increase_gives_the_difference() {
        assert_eq!(counter_delta(Some(1_000), 1_500), 500);
    }

    #[test]
    fn unchanged_counter_gives_zero() {
        assert_eq!(counter_delta(Some(1_000), 1_000), 0);
    }

    #[test]
    fn counter_reset_gives_the_current_value() {
        assert_eq!(counter_delta(Some(1_000), 300), 300);
    }

    #[test]
    fn device_without_previous_value_starts_from_zero() {
        assert_eq!(counter_delta(None, 1_500), 1_500);
    }

    #[test]
    fn rate_is_in_bits_per_second() {
        // 562.5 MB over a 15-minute interval is exactly 5 Mb/s.
        assert_eq!(rate_bps(562_500_000, 900), 5_000_000);
    }

    #[test]
    fn zero_or_negative_elapsed_time_gives_zero_rate() {
        assert_eq!(rate_bps(1_000, 0), 0);
        assert_eq!(rate_bps(1_000, -60), 0);
    }

    #[test]
    fn person_rate_sums_their_devices() {
        let devices = [
            DeviceRate {
                owner: Some(H),
                bps: 3_000_000,
            },
            DeviceRate {
                owner: Some(H),
                bps: 3_000_000,
            },
            DeviceRate {
                owner: Some(R),
                bps: 1_000_000,
            },
        ];
        assert_eq!(person_rate_bps(H, &devices), 6_000_000);
    }

    #[test]
    fn shared_devices_count_for_nobody() {
        let devices = [DeviceRate {
            owner: None,
            bps: 20_000_000,
        }];
        assert_eq!(person_rate_bps(R, &devices), 0);
    }

    #[test]
    fn person_without_device_seen_has_zero_rate() {
        let devices = [DeviceRate {
            owner: Some(H),
            bps: 8_000_000,
        }];
        assert_eq!(person_rate_bps(R, &devices), 0);
    }
}
