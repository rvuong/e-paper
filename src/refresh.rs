//! Refresh on change only.

use std::collections::BTreeMap;

use crate::state::{Person, PersonState};

/// State of the whole household, as shown on the display.
pub type HouseholdState = BTreeMap<Person, PersonState>;

/// Whether the display must be redrawn. An unknown previous state (first run,
/// lost state file) forces a refresh.
pub fn needs_refresh(previous: Option<&HouseholdState>, current: &HouseholdState) -> bool {
    previous != Some(current)
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
