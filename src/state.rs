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
    match (home, rate_bps > threshold_bps) {
        (false, _) => PersonState::Away,
        (true, false) => PersonState::HomePassive,
        (true, true) => PersonState::HomeActive,
    }
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
