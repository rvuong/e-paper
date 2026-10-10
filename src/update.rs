//! Which display update a cycle needs: none, a full refresh, or a partial
//! refresh of the slots that changed.

use crate::layout::{Window, layout, slot_window};
use crate::refresh::HouseholdState;

/// After this many partial refreshes in a row, the next update is a full one:
/// partial refreshes leave a faint trace of the previous image (ghosting).
pub const PARTIALS_BEFORE_FULL: u32 = 10;

/// What to do with the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    /// Nothing changed: the display is not touched.
    None,
    /// Full refresh: the panel flashes.
    Full,
    /// Partial refresh of this window: only the changed pixels move.
    Partial(Window),
}

/// Decides the update.
///
/// - `previous`: the state shown on the display, if known.
/// - `previous_frame`: whether the image shown on the display was saved (a
///   partial refresh needs it, as the panel forgets it between runs).
/// - `full_required`: a full refresh is asked for (`--force`, daily refresh).
/// - `partials_since_full`: partial refreshes since the last full one.
/// - `partial_enabled`: false to always use full refreshes.
pub fn plan_update(
    previous: Option<&HouseholdState>,
    current: &HouseholdState,
    previous_frame: bool,
    full_required: bool,
    partials_since_full: u32,
    partial_enabled: bool,
) -> UpdateKind {
    // Without the image shown, a partial refresh is impossible: redraw fully
    // (unless partial refresh is disabled, which never needs that image).
    let Some(previous) =
        previous.filter(|_| !full_required && (previous_frame || !partial_enabled))
    else {
        return UpdateKind::Full;
    };
    match changed_window(previous, current) {
        None => UpdateKind::None,
        Some(_) if !partial_enabled || partials_since_full >= PARTIALS_BEFORE_FULL => {
            UpdateKind::Full
        }
        Some(window) => UpdateKind::Partial(window),
    }
}

/// The window holding every slot that differs between two states, if any.
pub fn changed_window(previous: &HouseholdState, current: &HouseholdState) -> Option<Window> {
    // `layout` draws a person missing from the state as away: compare slots.
    layout(current)
        .into_iter()
        .zip(layout(previous))
        .filter(|(now, then)| now != then)
        .map(|(now, _)| slot_window(&now))
        .reduce(Window::union)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PEOPLE;
    use crate::state::PersonState::{self, Away, HomeActive, HomePassive};

    fn household(states: [PersonState; 4]) -> HouseholdState {
        PEOPLE.into_iter().zip(states).collect()
    }

    fn window_of(index: usize) -> Window {
        slot_window(&layout(&household([Away; 4]))[index])
    }

    const BEFORE: [PersonState; 4] = [HomePassive, Away, HomePassive, Away];

    #[test]
    fn identical_state_needs_no_update() {
        let state = household(BEFORE);
        assert_eq!(
            plan_update(Some(&state), &state, true, false, 0, true),
            UpdateKind::None
        );
    }

    #[test]
    fn one_person_changing_gives_a_partial_refresh_of_their_slot() {
        let current = household([HomePassive, HomePassive, HomePassive, Away]);
        assert_eq!(
            plan_update(Some(&household(BEFORE)), &current, true, false, 0, true),
            UpdateKind::Partial(window_of(1))
        );
    }

    #[test]
    fn two_people_changing_give_a_partial_refresh_of_both_slots() {
        let current = household([Away, Away, HomeActive, Away]);
        assert_eq!(
            plan_update(Some(&household(BEFORE)), &current, true, false, 0, true),
            UpdateKind::Partial(window_of(0).union(window_of(2)))
        );
    }

    #[test]
    fn required_full_refresh_happens_even_without_change() {
        let state = household(BEFORE);
        assert_eq!(
            plan_update(Some(&state), &state, true, true, 0, true),
            UpdateKind::Full
        );
    }

    #[test]
    fn no_previous_state_or_frame_gives_a_full_refresh() {
        let state = household(BEFORE);
        assert_eq!(
            plan_update(None, &state, true, false, 0, true),
            UpdateKind::Full
        );
        assert_eq!(
            plan_update(Some(&state), &state, false, false, 0, true),
            UpdateKind::Full
        );
    }

    #[test]
    fn too_many_partials_give_a_full_refresh() {
        let current = household([HomePassive, HomePassive, HomePassive, Away]);
        let previous = household(BEFORE);
        assert!(matches!(
            plan_update(
                Some(&previous),
                &current,
                true,
                false,
                PARTIALS_BEFORE_FULL - 1,
                true
            ),
            UpdateKind::Partial(_)
        ));
        assert_eq!(
            plan_update(
                Some(&previous),
                &current,
                true,
                false,
                PARTIALS_BEFORE_FULL,
                true
            ),
            UpdateKind::Full
        );
    }

    #[test]
    fn disabled_partial_refresh_gives_full_refreshes_on_change_only() {
        let previous = household(BEFORE);
        let current = household([HomePassive, HomePassive, HomePassive, Away]);
        assert_eq!(
            plan_update(Some(&previous), &current, true, false, 0, false),
            UpdateKind::Full
        );
        assert_eq!(
            plan_update(Some(&previous), &previous, true, false, 0, false),
            UpdateKind::None
        );
    }
}
