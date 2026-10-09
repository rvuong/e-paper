//! Presence rule: a person is home if their smartphone was active recently.

/// Whether the smartphone was active during the last interval, bound included.
///
/// `last_activity` and `now` are Unix timestamps in seconds. `None` means no
/// known smartphone activity: smartphone unknown to the Freebox, or no
/// smartphone configured for the person.
pub fn is_home(last_activity: Option<i64>, now: i64, interval_secs: i64) -> bool {
    last_activity.is_some_and(|last| now - last <= interval_secs)
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
