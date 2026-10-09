//! The household states shown by the mockups (`docs/mockups/phase1-*.svg`),
//! shared by the `mockups` example and the golden image test.

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
