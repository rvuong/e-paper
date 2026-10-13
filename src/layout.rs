//! Phase 1 layout: what to draw for each person, without drawing it.
//!
//! Geometry from the mockups (`docs/mockups/phase1-*.svg`, CLAUDE.md "Layout").
//! Every diameter is the outer size of the shape: its stroke is drawn inside.
//! Centres are points between pixels, as in the SVG: x = 109 is the boundary
//! between pixel columns 108 and 109.

use embedded_graphics::prelude::Point;

use crate::refresh::HouseholdState;
use crate::state::{Person, PersonState};

/// Display size in pixels.
pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 480;

/// Display order, left to right.
pub const PEOPLE: [Person; 4] = [Person('R'), Person('M'), Person('H'), Person('L')];

const SLOT: u32 = 178;
const GAP: u32 = 16;
const PHASE1_SIZES: SlotSizes = SlotSizes {
    core_diameter: 130,
    core_stroke: 12,
    ring_diameters: [154, 178],
    ring_stroke: 6,
    letter_size: 88,
};

/// Sizes of a person's circles and letter: the phase 1 row and the phase 2
/// column draw the same shapes at different sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotSizes {
    pub core_diameter: u32,
    pub core_stroke: u32,
    /// Inner ring first.
    pub ring_diameters: [u32; 2],
    pub ring_stroke: u32,
    pub letter_size: u32,
}

/// Colour of a pixel on the 1-bit panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    Black,
    White,
}

/// A black-stroked circle, optionally filled with black.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Circle {
    pub diameter: u32,
    pub stroke_width: u32,
    pub filled: bool,
}

/// A capital letter centred on its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Letter {
    pub glyph: char,
    pub size: u32,
    pub ink: Ink,
}

/// Everything drawn for one person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub person: Person,
    pub center: Point,
    pub core: Circle,
    /// The activity mark: empty, or the two concentric rings.
    pub rings: Vec<Circle>,
    pub letter: Letter,
}

/// The four slots, in display order. A person missing from the state is
/// drawn as away.
pub fn layout(state: &HouseholdState) -> Vec<Slot> {
    let row_width = PEOPLE.len() as u32 * SLOT + (PEOPLE.len() as u32 - 1) * GAP;
    let first_center = (WIDTH - row_width) / 2 + SLOT / 2;
    PEOPLE
        .into_iter()
        .zip(0..)
        .map(|(person, index)| {
            let person_state = state.get(&person).copied().unwrap_or(PersonState::Away);
            let x = first_center + index * (SLOT + GAP);
            slot(
                person,
                person_state,
                Point::new(x as i32, HEIGHT as i32 / 2),
                &PHASE1_SIZES,
            )
        })
        .collect()
}

/// Everything drawn for one person, centred on `center`.
pub fn slot(person: Person, person_state: PersonState, center: Point, sizes: &SlotSizes) -> Slot {
    let home = person_state != PersonState::Away;
    let rings = if person_state == PersonState::HomeActive {
        sizes
            .ring_diameters
            .into_iter()
            .map(|diameter| Circle {
                diameter,
                stroke_width: sizes.ring_stroke,
                filled: false,
            })
            .collect()
    } else {
        Vec::new()
    };
    Slot {
        person,
        center,
        core: Circle {
            diameter: sizes.core_diameter,
            stroke_width: sizes.core_stroke,
            filled: home,
        },
        rings,
        letter: Letter {
            glyph: person.0,
            size: sizes.letter_size,
            ink: if home { Ink::White } else { Ink::Black },
        },
    }
}

/// A rectangle of the display. `x` and `width` are multiples of 8: the panel
/// receives 8 pixels per byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Window {
    /// The smallest window holding both.
    pub fn union(self, other: Window) -> Window {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Window {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
}

/// The window of a slot: its whole square (rings included), widened to
/// multiples of 8 horizontally.
pub fn slot_window(slot: &Slot) -> Window {
    let half = SLOT as i32 / 2;
    let left = (slot.center.x - half) as u32 / 8 * 8;
    let right = ((slot.center.x + half) as u32).div_ceil(8) * 8;
    Window {
        x: left,
        y: (slot.center.y - half) as u32,
        width: right - left,
        height: SLOT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PersonState::{Away, HomeActive, HomePassive};

    fn household(states: [PersonState; 4]) -> HouseholdState {
        PEOPLE.into_iter().zip(states).collect()
    }

    fn slot_of(slots: &[Slot], initial: char) -> &Slot {
        slots
            .iter()
            .find(|slot| slot.person == Person(initial))
            .unwrap()
    }

    #[test]
    fn four_slots_in_display_order_and_position() {
        let slots = layout(&household([Away; 4]));
        let placed: Vec<(char, Point)> = slots
            .iter()
            .map(|slot| (slot.person.0, slot.center))
            .collect();
        assert_eq!(
            placed,
            [
                ('R', Point::new(109, 240)),
                ('M', Point::new(303, 240)),
                ('H', Point::new(497, 240)),
                ('L', Point::new(691, 240)),
            ]
        );
    }

    #[test]
    fn away_is_an_outline_with_a_black_letter_and_no_mark() {
        let slots = layout(&household([Away; 4]));
        let slot = slot_of(&slots, 'R');
        assert_eq!(
            slot.core,
            Circle {
                diameter: 130,
                stroke_width: 12,
                filled: false
            }
        );
        assert_eq!(
            slot.letter,
            Letter {
                glyph: 'R',
                size: 88,
                ink: Ink::Black
            }
        );
        assert!(slot.rings.is_empty());
    }

    #[test]
    fn home_passive_is_filled_with_a_white_letter_and_no_mark() {
        let slots = layout(&household([HomePassive, Away, Away, Away]));
        let slot = slot_of(&slots, 'R');
        assert!(slot.core.filled);
        assert_eq!(slot.letter.ink, Ink::White);
        assert!(slot.rings.is_empty());
    }

    #[test]
    fn home_active_adds_the_double_ring() {
        let slots = layout(&household([Away, Away, HomeActive, Away]));
        let slot = slot_of(&slots, 'H');
        assert!(slot.core.filled);
        assert_eq!(slot.letter.ink, Ink::White);
        assert_eq!(
            slot.rings,
            [
                Circle {
                    diameter: 154,
                    stroke_width: 6,
                    filled: false
                },
                Circle {
                    diameter: 178,
                    stroke_width: 6,
                    filled: false
                },
            ]
        );
    }

    #[test]
    fn person_missing_from_the_state_is_drawn_away() {
        let state: HouseholdState = [(Person('R'), HomePassive)].into_iter().collect();
        let slots = layout(&state);
        assert_eq!(slots.len(), 4);
        let slot = slot_of(&slots, 'M');
        assert!(!slot.core.filled);
        assert!(slot.rings.is_empty());
    }

    #[test]
    fn every_shape_fits_on_the_display() {
        let slots = layout(&household([HomeActive; 4]));
        for slot in &slots {
            let outer = slot
                .rings
                .iter()
                .chain([&slot.core])
                .map(|circle| circle.diameter)
                .max()
                .unwrap() as i32;
            let (x, y) = (slot.center.x, slot.center.y);
            assert!(x - outer / 2 >= 0 && x + outer / 2 <= WIDTH as i32);
            assert!(y - outer / 2 >= 0 && y + outer / 2 <= HEIGHT as i32);
        }
        assert_eq!(slots[0].center.x - 178 / 2, 20);
        assert_eq!(slots[3].center.x + 178 / 2, 780);
    }

    #[test]
    fn slot_window_covers_the_whole_slot_aligned_on_8_pixels() {
        let slots = layout(&household([Away; 4]));
        // R: slot from x = 20 to 198, widened to 16..200; y from 151 to 329.
        assert_eq!(
            slot_window(&slots[0]),
            Window {
                x: 16,
                y: 151,
                width: 184,
                height: 178
            }
        );
        for slot in &slots {
            let window = slot_window(slot);
            assert_eq!((window.x % 8, window.width % 8), (0, 0));
            assert!(window.x as i32 <= slot.center.x - 89);
            assert!((window.x + window.width) as i32 >= slot.center.x + 89);
        }
    }

    #[test]
    fn union_of_windows_holds_both() {
        let a = Window {
            x: 16,
            y: 151,
            width: 184,
            height: 178,
        };
        let b = Window {
            x: 600,
            y: 100,
            width: 184,
            height: 178,
        };
        assert_eq!(
            a.union(b),
            Window {
                x: 16,
                y: 100,
                width: 768,
                height: 229
            }
        );
    }
}
