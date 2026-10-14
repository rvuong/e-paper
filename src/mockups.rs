//! The states shown by the mockups (`docs/mockups/*.svg`), shared by the
//! `mockups` example and the golden image tests.

use crate::board::{Block, Board, Time};
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

/// Name, board and household of each phase 2 mockup.
pub fn phase2_states() -> [(&'static str, Board, HouseholdState); 3] {
    let at = |hour, minute| Time { hour, minute };
    let household = || -> HouseholdState {
        [(Person('R'), HomePassive), (Person('H'), HomeActive)]
            .into_iter()
            .collect()
    };
    [
        (
            "phase2-buses-presence",
            Board {
                clock: at(8, 4),
                rungis: Block::Times(vec![at(8, 8), at(8, 31)]),
                antony: Block::Times(vec![at(8, 12), at(8, 21)]),
            },
            household(),
        ),
        (
            "phase2-end-of-service",
            Board {
                clock: at(21, 15),
                rungis: Block::Moon,
                antony: Block::Times(vec![at(21, 22), at(21, 40)]),
            },
            household(),
        ),
        (
            "phase2-bus-data-unavailable",
            Board {
                clock: at(8, 4),
                rungis: Block::Wrench,
                antony: Block::Wrench,
            },
            household(),
        ),
    ]
}
