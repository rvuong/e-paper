//! The states shown by the mockups (`docs/mockups/*.svg`), shared by the
//! `mockups` example and the golden image tests.

use crate::board::{Board, Time};
use crate::refresh::HouseholdState;
use crate::state::Person;
use crate::state::PersonState::{HomeActive, HomePassive};

/// Name and state of each phase 1 mockup. People not listed are away.
pub fn phase1_states() -> [(&'static str, HouseholdState); 3] {
    [
        (
            "phase1-r-home",
            [(Person('R'), HomePassive)].into_iter().collect(),
        ),
        ("phase1-all-away", HouseholdState::new()),
        (
            "phase1-activity-ring",
            [(Person('R'), HomePassive), (Person('H'), HomeActive)]
                .into_iter()
                .collect(),
        ),
    ]
}

/// Name, board and household of the phase 2 mockup.
pub fn phase2_state() -> (&'static str, Board, HouseholdState) {
    let at = |hour, minute| Time { hour, minute };
    (
        "phase2-buses-presence",
        Board {
            clock: at(8, 4),
            rungis: vec![at(8, 8), at(8, 31)],
            antony: vec![at(8, 12), at(8, 21)],
        },
        [(Person('R'), HomePassive), (Person('H'), HomeActive)]
            .into_iter()
            .collect(),
    )
}
