//! Which display update a cycle needs: none, a full refresh, or a partial
//! refresh of the pixels that changed.

use crate::frame::Frame;
use crate::layout::{HEIGHT, WIDTH, Window, layout, slot_window};
use crate::refresh::HouseholdState;

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

/// Decides the update from the image shown and the new one.
///
/// - `previous`: the image shown on the display, if saved (a partial refresh
///   sends it again, as the panel forgets it between runs).
/// - `full_required`: a full refresh is asked for (`--force`, hourly
///   refresh against ghosting).
/// - `partial_enabled`: false to always use full refreshes.
pub fn plan_display(
    previous: Option<&Frame>,
    current: &Frame,
    full_required: bool,
    partial_enabled: bool,
) -> UpdateKind {
    let Some(previous) = previous.filter(|_| !full_required) else {
        return UpdateKind::Full;
    };
    match frame_window(previous, current) {
        None => UpdateKind::None,
        Some(_) if !partial_enabled => UpdateKind::Full,
        Some(window) => UpdateKind::Partial(window),
    }
}

/// Whether the hourly full refresh is due: none happened since the start of
/// the current hour.
pub fn hourly_full_due(now: i64, last_full_refresh: Option<i64>) -> bool {
    let hour_start = now - now.rem_euclid(3_600);
    last_full_refresh.is_none_or(|last| last < hour_start)
}

/// The window holding every pixel that differs between two frames, widened
/// to multiples of 8 horizontally. None if they are identical.
pub fn frame_window(previous: &Frame, current: &Frame) -> Option<Window> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if previous.is_black(x as i32, y as i32) != current.is_black(x as i32, y as i32) {
                let (left, top, right, bottom) = bounds.unwrap_or((x, y, x, y));
                bounds = Some((left.min(x), top.min(y), right.max(x), bottom.max(y)));
            }
        }
    }
    bounds.map(|(left, top, right, bottom)| {
        let x = left / 8 * 8;
        Window {
            x,
            y: top,
            width: (right + 1).div_ceil(8) * 8 - x,
            height: bottom + 1 - top,
        }
    })
}

/// The window holding every slot that differs between two states, if any
/// (phase 1 row, used by `partial-test`).
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
    use crate::board::{Block, Board, Time, board_layout};
    use crate::layout::PEOPLE;
    use crate::render::{atkinson_bold, render_board};
    use crate::state::Person;
    use crate::state::PersonState::{self, Away, HomeActive, HomePassive};

    fn household(states: [PersonState; 4]) -> HouseholdState {
        PEOPLE.into_iter().zip(states).collect()
    }

    fn window_of(index: usize) -> Window {
        slot_window(&layout(&household([Away; 4]))[index])
    }

    const BEFORE: [PersonState; 4] = [HomePassive, Away, HomePassive, Away];

    /// The phase 2 screen at this clock time, with the mockup's departures.
    fn screen(hour: u8, minute: u8) -> Frame {
        let at = |hour, minute| Time { hour, minute };
        let board = Board {
            clock: at(hour, minute),
            rungis: Block::Times(vec![at(8, 8), at(8, 31)]),
            antony: Block::Times(vec![at(8, 12), at(8, 21)]),
        };
        let household: HouseholdState = [(Person('R'), HomePassive)].into_iter().collect();
        let font = atkinson_bold();
        let mut frame = Frame::new();
        render_board(&mut frame, &board_layout(&board, &household, &font), &font).unwrap();
        frame
    }

    /// 2026-10-14 09:00:00 UTC, the start of an hour.
    const NINE: i64 = 1_791_968_400;

    // A. Phase 1 row (partial-test)

    #[test]
    fn one_person_changing_gives_their_slot() {
        let current = household([HomePassive, HomePassive, HomePassive, Away]);
        assert_eq!(
            changed_window(&household(BEFORE), &current),
            Some(window_of(1))
        );
    }

    #[test]
    fn two_people_changing_give_both_slots() {
        let current = household([Away, Away, HomeActive, Away]);
        assert_eq!(
            changed_window(&household(BEFORE), &current),
            Some(window_of(0).union(window_of(2)))
        );
    }

    // B. Phase 2 display update

    #[test]
    fn no_previous_frame_gives_a_full_refresh() {
        assert_eq!(
            plan_display(None, &screen(8, 4), false, true),
            UpdateKind::Full
        );
    }

    #[test]
    fn identical_frames_need_no_update() {
        assert_eq!(
            plan_display(Some(&screen(8, 4)), &screen(8, 4), false, true),
            UpdateKind::None
        );
    }

    #[test]
    fn clock_change_gives_a_partial_refresh_of_the_clock() {
        let UpdateKind::Partial(window) =
            plan_display(Some(&screen(8, 4)), &screen(8, 5), false, true)
        else {
            panic!("expected a partial refresh");
        };
        assert_eq!((window.x % 8, window.width % 8), (0, 0));
        // Within the clock area: above the horizontal line, left of the
        // vertical one.
        assert!(
            window.y >= 14 && window.y + window.height <= 92,
            "{window:?}"
        );
        assert!(window.x + window.width <= 632, "{window:?}");
    }

    #[test]
    fn full_refresh_is_due_once_an_hour() {
        assert!(hourly_full_due(NINE + 2, Some(NINE - 60)));
        assert!(!hourly_full_due(NINE + 59 * 60, Some(NINE + 2)));
        assert!(hourly_full_due(NINE + 60 * 60, Some(NINE + 2)));
        assert!(hourly_full_due(NINE, None));
    }

    #[test]
    fn forced_or_full_only_refreshes() {
        let (before, after) = (screen(8, 4), screen(8, 5));
        assert_eq!(
            plan_display(Some(&before), &before, true, true),
            UpdateKind::Full
        );
        assert_eq!(
            plan_display(Some(&before), &after, false, false),
            UpdateKind::Full
        );
        assert_eq!(
            plan_display(Some(&before), &before, false, false),
            UpdateKind::None
        );
    }
}
