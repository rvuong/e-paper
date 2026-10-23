# Guide 02: presence logic

The core of phase 1: from the Freebox data (byte counters, smartphone
activity) to the state of each person (`Away`, `HomePassive`, `HomeActive`),
and the decision to redraw the display. Only pure functions: no Freebox, no
display, no clock, no file. The rules: a person is home if their smartphone
was active within the presence window; the download rates of their devices,
from byte counter deltas, are summed, and above 5 Mb/s the activity mark
shows (never while the smartphone is away); the display is redrawn only
when the state changes.

Approved tests: 21 unit tests in 4 modules (the approved test 16 is split in
two: "below" and "exactly at" the threshold; approved tests 13 and 14 are one
test at this level, see section 3).

| Module | Content | Tests |
|---|---|---|
| `state` | `Person`, `PersonState`, threshold, state of a person | E |
| `rate` | counter delta, rate, rate of a person | A, B, C |
| `presence` | presence rule | D |
| `refresh` | refresh on change only | F |

## The TDD loop in Rust

For each module:

1. **Red.** Create the file with the types, the tests, and the functions whose
   body is `todo!()`. The code compiles (with "unused variable" warnings, which
   are expected at this stage), and every new test fails with
   `not yet implemented`.
2. **Green.** Replace each `todo!()` with the implementation given below. All
   tests pass.
3. **Refactor.** `cargo clippy` and `cargo fmt` are clean. Improve the code if
   needed, tests still green.

Run every command from the project root. Files are created in VS Code.

## 0. Library entry point

The logic lives in a library (`src/lib.rs`), next to the binary
(`src/main.rs`), which stays the "Hello, world!" for now. The binary will use
the library in a later guide.

Create the file `src/lib.rs` with this content:

```rust
//! Household e-paper dashboard.
```

Each section below adds one `pub mod` line to it.

## 1. `state`: the state of a person

- `Person`: a household member, identified by the initial shown on the display.
- `PersonState`: the three possible states. "Away with the mark" cannot be
  written, so the privacy rule holds by construction.
- `person_state`: the mark needs the person at home **and** a rate strictly
  above the threshold.

**Red.** Create the file `src/state.rs` with this content:

```rust
//! Logical state of each household member, as shown on the display.

/// A household member, identified by the initial shown on the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Person(pub char);

/// What the display shows for one person.
///
/// "Away with the activity mark" cannot be represented: the privacy rule is
/// enforced by the type itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonState {
    Away,
    HomePassive,
    HomeActive,
}

/// Activity mark threshold: 5 Mb/s.
pub const ACTIVITY_THRESHOLD_BPS: u64 = 5_000_000;

/// State of a person. The activity mark needs a rate strictly above the
/// threshold, and the person at home.
pub fn person_state(home: bool, rate_bps: u64, threshold_bps: u64) -> PersonState {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLD: u64 = ACTIVITY_THRESHOLD_BPS;

    #[test]
    fn away_with_high_rate_is_away() {
        assert_eq!(
            person_state(false, 50_000_000, THRESHOLD),
            PersonState::Away
        );
    }

    #[test]
    fn home_below_threshold_is_passive() {
        assert_eq!(
            person_state(true, 1_000_000, THRESHOLD),
            PersonState::HomePassive
        );
    }

    #[test]
    fn home_exactly_at_threshold_is_passive() {
        assert_eq!(
            person_state(true, THRESHOLD, THRESHOLD),
            PersonState::HomePassive
        );
    }

    #[test]
    fn home_above_threshold_is_active() {
        assert_eq!(
            person_state(true, THRESHOLD + 1, THRESHOLD),
            PersonState::HomeActive
        );
    }
}
```

Add this line to `src/lib.rs`:

```rust
pub mod state;
```

```bash
cargo test
```

Expected: `4 failed`, each with `not yet implemented`.

**Green.** In `src/state.rs`, replace the body of `person_state` (the
`todo!()` line) with:

```rust
    match (home, rate_bps > threshold_bps) {
        (false, _) => PersonState::Away,
        (true, false) => PersonState::HomePassive,
        (true, true) => PersonState::HomeActive,
    }
```

```bash
cargo test
```

Expected: `4 passed`.

The `match` on a tuple lists every case, and the compiler checks that none is
missing: `(false, _)` covers "away", whatever the rate.

## 2. `rate`: counters and rates

- `counter_delta`: bytes received since the previous poll, with the reset guard
  (tests A).
- `rate_bps`: rate in bits per second, 0 when the elapsed time is not positive
  (tests B).
- `person_rate_bps`: sum over the person's devices; shared devices
  (`owner: None`) count for nobody (tests C).

**Red.** Create the file `src/rate.rs` with this content:

```rust
//! Download rates computed from the Freebox byte counters.

use crate::state::Person;

/// Bytes received by a device between two polls.
///
/// The counter restarts at 0 when the device reconnects, so a counter lower
/// than the previous value is taken as the delta itself. A device without a
/// previous value starts from 0 the same way. Never negative.
pub fn counter_delta(previous: Option<u64>, current: u64) -> u64 {
    todo!()
}

/// Download rate in bits per second, 0 when the elapsed time is not positive.
pub fn rate_bps(delta_bytes: u64, elapsed_secs: i64) -> u64 {
    todo!()
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
    todo!()
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
```

Add this line to `src/lib.rs`:

```rust
pub mod rate;
```

```bash
cargo test
```

Expected: the 4 `state` tests pass, the 9 `rate` tests fail.

**Green.** In `src/rate.rs`, replace the body of `counter_delta` with:

```rust
    match previous {
        Some(previous) if current >= previous => current - previous,
        _ => current,
    }
```

The body of `rate_bps` with:

```rust
    match u64::try_from(elapsed_secs) {
        Ok(elapsed) if elapsed > 0 => delta_bytes * 8 / elapsed,
        _ => 0,
    }
```

`u64::try_from` fails on a negative number: no unsafe cast, and both "zero"
and "negative" end in the `_ => 0` branch.

The body of `person_rate_bps` with:

```rust
    devices
        .iter()
        .filter(|device| device.owner == Some(person))
        .map(|device| device.bps)
        .sum()
```

```bash
cargo test
```

Expected: `13 passed`.

## 3. `presence`: the presence rule

`is_home` says whether the smartphone was active during the last interval,
bound included. Timestamps are Unix seconds, passed as parameters: the function
never reads the clock, so the tests control time.

Approved tests 13 ("smartphone unknown to the Freebox") and 14 ("no smartphone
configured") reach this function the same way, as `None`: one test covers
both here. The difference between the two cases belongs to the device mapping
lookup, tested in its own guide.

**Red.** Create the file `src/presence.rs` with this content:

```rust
//! Presence rule: a person is home if their smartphone was active recently.

/// Whether the smartphone was active during the last interval, bound included.
///
/// `last_activity` and `now` are Unix timestamps in seconds. `None` means no
/// known smartphone activity: smartphone unknown to the Freebox, or no
/// smartphone configured for the person.
pub fn is_home(last_activity: Option<i64>, now: i64, interval_secs: i64) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000;
    const INTERVAL: i64 = 15 * 60;

    #[test]
    fn active_two_minutes_ago_is_home() {
        assert!(is_home(Some(NOW - 2 * 60), NOW, INTERVAL));
    }

    #[test]
    fn active_sixteen_minutes_ago_is_away() {
        assert!(!is_home(Some(NOW - 16 * 60), NOW, INTERVAL));
    }

    #[test]
    fn active_exactly_one_interval_ago_is_home() {
        assert!(is_home(Some(NOW - INTERVAL), NOW, INTERVAL));
    }

    #[test]
    fn no_known_smartphone_activity_is_away() {
        assert!(!is_home(None, NOW, INTERVAL));
    }
}
```

Add this line to `src/lib.rs`:

```rust
pub mod presence;
```

```bash
cargo test
```

Expected: 13 passed, 4 failed.

**Green.** Replace the body of `is_home` with:

```rust
    last_activity.is_some_and(|last| now - last <= interval_secs)
```

```bash
cargo test
```

Expected: `17 passed`.

## 4. `refresh`: refresh on change only

`HouseholdState` maps each person to their state. `BTreeMap` keeps the people
sorted and compares two states with `==`, so `needs_refresh` is a single
comparison; `None` (unknown previous state) never equals a known state.

**Red.** Create the file `src/refresh.rs` with this content:

```rust
//! Refresh on change only.

use std::collections::BTreeMap;

use crate::state::{Person, PersonState};

/// State of the whole household, as shown on the display.
pub type HouseholdState = BTreeMap<Person, PersonState>;

/// Whether the display must be redrawn. An unknown previous state (first run,
/// lost state file) forces a refresh.
pub fn needs_refresh(previous: Option<&HouseholdState>, current: &HouseholdState) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use PersonState::{Away, HomeActive, HomePassive};

    /// Household state in the display order R, M, H, L.
    fn household(states: [PersonState; 4]) -> HouseholdState {
        ['R', 'M', 'H', 'L']
            .into_iter()
            .map(Person)
            .zip(states)
            .collect()
    }

    #[test]
    fn unknown_previous_state_forces_refresh() {
        assert!(needs_refresh(None, &household([Away; 4])));
    }

    #[test]
    fn identical_state_does_not_refresh() {
        let state = household([HomePassive, Away, Away, Away]);
        assert!(!needs_refresh(Some(&state), &state));
    }

    #[test]
    fn one_person_changing_refreshes() {
        let previous = household([HomePassive, Away, Away, Away]);
        let current = household([HomePassive, Away, HomePassive, Away]);
        assert!(needs_refresh(Some(&previous), &current));
    }

    #[test]
    fn activity_mark_change_refreshes() {
        let previous = household([HomePassive, Away, Away, Away]);
        let current = household([HomeActive, Away, Away, Away]);
        assert!(needs_refresh(Some(&previous), &current));
    }
}
```

Add this line to `src/lib.rs`:

```rust
pub mod refresh;
```

```bash
cargo test
```

Expected: 17 passed, 4 failed.

**Green.** Replace the body of `needs_refresh` with:

```rust
    previous != Some(current)
```

```bash
cargo test
```

Expected: `21 passed`.

## 5. Refactor and check

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Expected: no clippy warning. `cargo fmt --check` reports one difference in
`src/lib.rs`: rustfmt wants the `pub mod` lines in alphabetical order. Fix it
and check again:

```bash
cargo fmt
cargo fmt --check
```

`src/lib.rs` now reads:

```rust
//! Household e-paper dashboard.

pub mod presence;
pub mod rate;
pub mod refresh;
pub mod state;
```

## 6. Commit

```bash
git add -A
git status
git commit -m "feat: presence logic (state, rates, presence rule, refresh)"
```

Expected new files: `docs/guide-02-presence-logic.md`, `src/lib.rs`,
`src/presence.rs`, `src/rate.rs`, `src/refresh.rs`, `src/state.rs`.

## Report

Share the output of the last `cargo test`, of section 5, and `git status`.

## Not in this guide

- Wiring these functions together (one cycle: Freebox data in, household state
  out) and the persisted state between runs.
- The daily forced refresh.
- The display: guide 03 (first render).
