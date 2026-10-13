# Guide 11: phase 2 render

Draws the phase 2 screen of the approved mockup
(`docs/mockups/phase2-buses-presence.svg`, CLAUDE.md "Phase 2", "Layout"):
clock, two direction blocks with two departure times each, presence circles
in a column on the right. Nothing calls it yet: the phase 2 cycle (PRIM
answers, choice of the departures, refresh) comes in a later guide. Phase 1,
running on the Pi, is not changed: its golden images must stay identical.

Same split as phase 1:

| Module | Content | Tests |
|---|---|---|
| `layout` | phase 1 row; the circle sizes become a parameter | existing |
| `board` (new) | time format, phase 2 layout: what to draw and where | A, B, C (1–12) |
| `render` | draws the phase 2 layout as pixels | D (13–17) |
| `tests/golden.rs` | phase 2 golden image | E (18) |

Input of the layout: the clock time, the next departures of each direction
(already chosen and sorted, local time; only the first two are shown) and
the household state. Choosing the departures is the job of the parsing
guide.

Approved tests: 18 (2026-10-13). Approved choices: a cell without departure
shows a long dash "—" at 96 px, right-aligned like a time; times have no
leading zero on the hour ("8:04").

Checked while writing this guide, on a copy of the project: the 18 tests
pass, the phase 1 golden images are unchanged, clippy is clean, and the
render matches the mockup.

Same TDD loop as before: red (`todo!()`), green, refactor. Run every command
from the project root; create files in VS Code.

## 1. Refactor: circle sizes as a parameter (`layout`)

Phase 2 draws the same circles as phase 1, smaller. The sizes, constants
today, become a `SlotSizes` value, and `slot` becomes public. No new test:
the existing ones (and the phase 1 golden images) must stay green.

In `src/layout.rs`, replace the five constants from `CORE_DIAMETER` to
`LETTER_SIZE` with:

```rust
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
```

In `layout`, the call to `slot` takes the sizes as a fourth argument:

```rust
            slot(
                person,
                person_state,
                Point::new(x as i32, HEIGHT as i32 / 2),
                &PHASE1_SIZES,
            )
```

Replace the start of `fn slot` (its signature and the `rings` block) with:

```rust
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
```

and, further down in `slot`, use the sizes for the core and the letter:

```rust
        core: Circle {
            diameter: sizes.core_diameter,
            stroke_width: sizes.core_stroke,
            filled: home,
        },
        rings,
        letter: Letter {
            glyph: person.0,
            size: sizes.letter_size,
```

```bash
cargo test
```

Expected: `140 passed`, and the golden image test passes (phase 1 unchanged).

## 2. `board`: the phase 2 layout (tests 1–12)

### What the tests check

**A. Time format**

1. `times_have_no_leading_zero_on_the_hour`: 8:04 → "8:04", 0:05 → "0:05",
   23:59 → "23:59"; minutes always on two digits.

**B. Layout, positions from the mockup**

2. `circles_are_a_column_on_the_right`: R, M, H, L top to bottom, centred at
   (721, 63), (721, 181), (721, 299), (721, 417).
3. `circles_use_the_phase_2_sizes`: H with activity: core 74 px (8 px
   stroke), rings 92 and 110 px (5 px stroke), letter 50 px. The home / away
   / activity rules are phase 1's, already tested.
4. `separators_form_a_t`: vertical line at x 630 from y 48 (384 px high),
   horizontal line at y 92 from x 24 to x 634: it ends on the vertical one.
5. `clock_is_top_left`: "8:04", 64 px, at x 24, line box from y 14.
6. `rungis_block_has_one_tag_then_its_label`: tag "319" at (24, 126), 36 px
   high, radius 6, white text; the label 12 px after the tag.
7. `antony_block_has_two_tags_then_its_label`: "297" at (24, 302), "401"
   8 px after it, then "Antony RER" 12 px further.
8. `two_departures_fill_both_cells_right_aligned`: each 96 px time ends on
   the right edge of its cell, x 299 and x 606.
9. `one_departure_fills_the_first_cell`: one departure goes to the left cell,
   the right one shows "—"; no departure at all gives "—" twice.

**C. Everything fits, measured with the real font**

10. `every_time_of_day_fits_its_cell`: the 1,440 times from 0:00 to 23:59
    fit a 275 px cell at 96 px ("00:00", the widest, is about 274 px).
11. `every_time_of_day_fits_the_clock_area`: same check for the 64 px clock,
    24 px before the vertical line.
12. `direction_labels_fit_on_one_line`: each label is one line of text and
    ends at least 24 px before the vertical line. The labels are fixed in
    the code (not the destinations from PRIM): if one is changed and gets
    too long, this test fails before any deployment.

### Red

Create the file `src/board.rs` with this content:

```rust
//! Phase 2 layout: clock, next departures per direction and presence column,
//! without drawing them.
//!
//! Geometry from the mockup (`docs/mockups/phase2-buses-presence.svg`,
//! CLAUDE.md "Phase 2", "Layout"). Text widths come from the font: the layout
//! needs them to right-align the times and to put the tags side by side.

use std::fmt;

use embedded_graphics::prelude::Point;
use fontdue::Font;

use crate::layout::{Ink, PEOPLE, Slot, SlotSizes, slot};
use crate::refresh::HouseholdState;
use crate::state::PersonState;

/// Shown in a cell without departure.
pub const NO_DEPARTURE: &str = "—";

/// A time of day, local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
}

impl fmt::Display for Time {
    /// "8:04": no leading zero on the hour, two digits for the minutes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

/// What the bus part shows: the clock and the next departures of each
/// direction, already chosen and sorted (only the first two are shown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub clock: Time,
    pub rungis: Vec<Time>,
    pub antony: Vec<Time>,
}

/// A black rectangle, with rounded corners when `radius` is not 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub radius: u32,
}

/// One line of text. It starts at `left`; its baseline sits in a line box
/// `line_height` high from `top`, as in the mockup's CSS.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub text: String,
    pub size: u32,
    pub left: f32,
    pub top: f32,
    pub line_height: f32,
    /// Sum of the advance widths.
    pub width: f32,
    pub ink: Ink,
}

impl Text {
    pub fn right(&self) -> f32 {
        self.left + self.width
    }
}

/// Something to draw, in drawing order.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Rect(Rect),
    Text(Text),
}

/// The whole phase 2 screen.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardLayout {
    pub elements: Vec<Element>,
    pub slots: Vec<Slot>,
}

/// Width of a line of text: the sum of its advance widths (no kerning, as in
/// the mockup export).
pub fn text_width(font: &Font, text: &str, size: u32) -> f32 {
    todo!()
}

/// Where everything of the phase 2 screen goes. A person missing from the
/// state is drawn as away.
pub fn board_layout(board: &Board, household: &HouseholdState, font: &Font) -> BoardLayout {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Circle, Letter};
    use crate::render::atkinson_bold;
    use crate::state::Person;
    use crate::state::PersonState::{HomeActive, HomePassive};

    fn at(hour: u8, minute: u8) -> Time {
        Time { hour, minute }
    }

    /// The mockup's board: R home, H home with the activity mark.
    fn mockup_layout(rungis: Vec<Time>, antony: Vec<Time>) -> BoardLayout {
        let board = Board {
            clock: at(8, 4),
            rungis,
            antony,
        };
        let household: HouseholdState = [(Person('R'), HomePassive), (Person('H'), HomeActive)]
            .into_iter()
            .collect();
        board_layout(&board, &household, &atkinson_bold())
    }

    fn texts(layout: &BoardLayout) -> Vec<&Text> {
        layout
            .elements
            .iter()
            .filter_map(|element| match element {
                Element::Text(text) => Some(text),
                Element::Rect(_) => None,
            })
            .collect()
    }

    fn rects(layout: &BoardLayout) -> Vec<Rect> {
        layout
            .elements
            .iter()
            .filter_map(|element| match element {
                Element::Rect(rect) => Some(*rect),
                Element::Text(_) => None,
            })
            .collect()
    }

    fn text_of<'a>(layout: &'a BoardLayout, content: &str) -> &'a Text {
        texts(layout)
            .into_iter()
            .find(|text| text.text == content)
            .unwrap_or_else(|| panic!("no text {content:?}"))
    }

    /// The texts of size 96 whose line box starts at `top`, left to right.
    fn times_at(layout: &BoardLayout, top: f32) -> Vec<&Text> {
        texts(layout)
            .into_iter()
            .filter(|text| text.size == 96 && text.top == top)
            .collect()
    }

    /// The tag rectangle around a tag text.
    fn tag_of(layout: &BoardLayout, line: &str) -> Rect {
        let text = text_of(layout, line);
        rects(layout)
            .into_iter()
            .find(|rect| {
                rect.radius > 0
                    && (rect.x as f32) < text.left
                    && text.right() < (rect.x + rect.width as i32) as f32
                    && (rect.y as f32) < text.top
                    && text.top + text.line_height < (rect.y + rect.height as i32) as f32
            })
            .unwrap()
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.01
    }

    fn every_time_of_day() -> impl Iterator<Item = Time> {
        (0..24).flat_map(|hour| (0..60).map(move |minute| at(hour, minute)))
    }

    // A. Time format

    #[test]
    fn times_have_no_leading_zero_on_the_hour() {
        assert_eq!(at(8, 4).to_string(), "8:04");
        assert_eq!(at(0, 5).to_string(), "0:05");
        assert_eq!(at(23, 59).to_string(), "23:59");
    }

    // B. Layout, positions from the mockup

    #[test]
    fn circles_are_a_column_on_the_right() {
        let layout = mockup_layout(vec![], vec![]);
        let placed: Vec<(char, Point)> = layout
            .slots
            .iter()
            .map(|slot| (slot.person.0, slot.center))
            .collect();
        assert_eq!(
            placed,
            [
                ('R', Point::new(721, 63)),
                ('M', Point::new(721, 181)),
                ('H', Point::new(721, 299)),
                ('L', Point::new(721, 417)),
            ]
        );
    }

    #[test]
    fn circles_use_the_phase_2_sizes() {
        let layout = mockup_layout(vec![], vec![]);
        let h = &layout.slots[2];
        assert_eq!(
            h.core,
            Circle {
                diameter: 74,
                stroke_width: 8,
                filled: true
            }
        );
        assert_eq!(
            h.rings,
            [
                Circle {
                    diameter: 92,
                    stroke_width: 5,
                    filled: false
                },
                Circle {
                    diameter: 110,
                    stroke_width: 5,
                    filled: false
                },
            ]
        );
        assert_eq!(
            h.letter,
            Letter {
                glyph: 'H',
                size: 50,
                ink: Ink::White
            }
        );
    }

    #[test]
    fn separators_form_a_t() {
        let lines: Vec<Rect> = rects(&mockup_layout(vec![], vec![]))
            .into_iter()
            .filter(|rect| rect.radius == 0)
            .collect();
        let vertical = Rect {
            x: 630,
            y: 48,
            width: 4,
            height: 384,
            radius: 0,
        };
        let horizontal = Rect {
            x: 24,
            y: 92,
            width: 610,
            height: 4,
            radius: 0,
        };
        assert_eq!(lines, [horizontal, vertical]);
        // The horizontal line ends on the vertical one, within its height.
        assert_eq!(
            horizontal.x + horizontal.width as i32,
            vertical.x + vertical.width as i32
        );
        assert!(vertical.y < horizontal.y && horizontal.y < vertical.y + vertical.height as i32);
    }

    #[test]
    fn clock_is_top_left() {
        let layout = mockup_layout(vec![], vec![]);
        let clock = text_of(&layout, "8:04");
        assert_eq!((clock.size, clock.ink), (64, Ink::Black));
        assert!(close(clock.left, 24.0) && close(clock.top, 14.0));
        assert!(close(clock.line_height, 64.0));
    }

    #[test]
    fn rungis_block_has_one_tag_then_its_label() {
        let layout = mockup_layout(vec![], vec![]);
        let tag = tag_of(&layout, "319");
        assert_eq!((tag.x, tag.y, tag.height, tag.radius), (24, 126, 36, 6));
        assert_eq!(text_of(&layout, "319").ink, Ink::White);
        let label = text_of(&layout, "Marché International de Rungis");
        assert_eq!((label.size, label.ink), (22, Ink::Black));
        assert!(close(label.left, (tag.x + tag.width as i32 + 12) as f32));
    }

    #[test]
    fn antony_block_has_two_tags_then_its_label() {
        let layout = mockup_layout(vec![], vec![]);
        let (first, second) = (tag_of(&layout, "297"), tag_of(&layout, "401"));
        assert_eq!((first.x, first.y), (24, 302));
        assert_eq!(second.x, first.x + first.width as i32 + 8);
        assert_eq!(second.y, first.y);
        let label = text_of(&layout, "Antony RER");
        assert!(close(
            label.left,
            (second.x + second.width as i32 + 12) as f32
        ));
    }

    #[test]
    fn two_departures_fill_both_cells_right_aligned() {
        let layout = mockup_layout(vec![at(8, 8), at(8, 31)], vec![at(8, 12), at(8, 21)]);
        for (top, expected) in [(172.0, ["8:08", "8:31"]), (348.0, ["8:12", "8:21"])] {
            let times = times_at(&layout, top);
            let shown: Vec<&str> = times.iter().map(|text| text.text.as_str()).collect();
            assert_eq!(shown, expected);
            assert!(close(times[0].right(), 299.0) && close(times[1].right(), 606.0));
        }
    }

    #[test]
    fn one_departure_fills_the_first_cell() {
        let layout = mockup_layout(vec![at(8, 8)], vec![]);
        let rungis: Vec<&str> = times_at(&layout, 172.0)
            .iter()
            .map(|text| text.text.as_str())
            .collect();
        assert_eq!(rungis, ["8:08", NO_DEPARTURE]);
        let antony: Vec<&str> = times_at(&layout, 348.0)
            .iter()
            .map(|text| text.text.as_str())
            .collect();
        assert_eq!(antony, [NO_DEPARTURE, NO_DEPARTURE]);
        assert!(close(times_at(&layout, 172.0)[1].right(), 606.0));
    }

    // C. Everything fits, measured with the real font

    #[test]
    fn every_time_of_day_fits_its_cell() {
        let font = atkinson_bold();
        for time in every_time_of_day() {
            let width = text_width(&font, &time.to_string(), 96);
            assert!(width <= 275.0, "{time} is {width} px wide");
        }
    }

    #[test]
    fn every_time_of_day_fits_the_clock_area() {
        let font = atkinson_bold();
        for time in every_time_of_day() {
            let right = 24.0 + text_width(&font, &time.to_string(), 64);
            assert!(right <= 630.0 - 24.0, "{time} ends at {right}");
        }
    }

    #[test]
    fn direction_labels_fit_on_one_line() {
        let layout = mockup_layout(vec![], vec![]);
        for label in ["Marché International de Rungis", "Antony RER"] {
            let text = text_of(&layout, label);
            assert!(
                text.right() <= 630.0 - 24.0,
                "{label} ends at {}",
                text.right()
            );
        }
    }
}
```

In `src/lib.rs`, add before `pub mod cycle;`:

```rust
pub mod board;
```

```bash
cargo test --lib board
```

Expected: the 12 tests fail on `todo!()` (warnings about unused imports and
arguments are normal at this stage).

### Green

In `fmt`, replace `todo!()` with:

```rust
write!(f, "{}:{:02}", self.hour, self.minute)
```

After the `use` lines, add the constants:

```rust
/// Left margin of the bus part.
const MARGIN: i32 = 24;
/// The vertical separator; the horizontal one runs from the margin to it.
const SEPARATOR_X: i32 = 630;
const SEPARATOR_TOP: i32 = 48;
const SEPARATOR_HEIGHT: u32 = 384;
const LINE_Y: i32 = 92;
const LINE_THICKNESS: u32 = 4;

const CLOCK_SIZE: u32 = 64;
const CLOCK_TOP: i32 = 14;

const RUNGIS_TOP: i32 = 124;
const ANTONY_TOP: i32 = 300;
/// Header row of a direction: tags then label, vertically centred.
const HEADER_HEIGHT: i32 = 40;
const TAG_SIZE: u32 = 28;
const TAG_PADDING_X: f32 = 12.0;
const TAG_PADDING_Y: u32 = 4;
const TAG_RADIUS: u32 = 6;
const TAG_GAP: i32 = 8;
const LABEL_GAP: i32 = 12;
const LABEL_SIZE: u32 = 22;
const LABEL_LINE_HEIGHT: f32 = 22.0 * 1.15;
/// Departure times, below the header after an 8 px gap.
const TIME_SIZE: u32 = 96;
const TIMES_OFFSET: i32 = HEADER_HEIGHT + 8;
const CELL_WIDTH: i32 = 275;
const CELL_GAP: i32 = 32;

const RUNGIS_LINES: [&str; 1] = ["319"];
const RUNGIS_LABEL: &str = "Marché International de Rungis";
const ANTONY_LINES: [&str; 2] = ["297", "401"];
const ANTONY_LABEL: &str = "Antony RER";

/// Presence column: 110 px slots, 8 px apart, from y = 8.
const COLUMN_X: i32 = 721;
const COLUMN_FIRST_Y: i32 = 63;
const COLUMN_STEP: i32 = 118;
const PHASE2_SIZES: SlotSizes = SlotSizes {
    core_diameter: 74,
    core_stroke: 8,
    ring_diameters: [92, 110],
    ring_stroke: 5,
    letter_size: 50,
};
```

Replace the two `todo!()` functions `text_width` and `board_layout` with:

```rust
/// Width of a line of text: the sum of its advance widths (no kerning, as in
/// the mockup export).
pub fn text_width(font: &Font, text: &str, size: u32) -> f32 {
    text.chars()
        .map(|glyph| font.metrics(glyph, size as f32).advance_width)
        .sum()
}

/// Where everything of the phase 2 screen goes. A person missing from the
/// state is drawn as away.
pub fn board_layout(board: &Board, household: &HouseholdState, font: &Font) -> BoardLayout {
    let mut elements = vec![
        Element::Text(text(
            font,
            &board.clock.to_string(),
            CLOCK_SIZE,
            MARGIN as f32,
            CLOCK_TOP as f32,
            CLOCK_SIZE as f32,
            Ink::Black,
        )),
        Element::Rect(Rect {
            x: MARGIN,
            y: LINE_Y,
            width: (SEPARATOR_X + LINE_THICKNESS as i32 - MARGIN) as u32,
            height: LINE_THICKNESS,
            radius: 0,
        }),
        Element::Rect(Rect {
            x: SEPARATOR_X,
            y: SEPARATOR_TOP,
            width: LINE_THICKNESS,
            height: SEPARATOR_HEIGHT,
            radius: 0,
        }),
    ];
    direction(
        &mut elements,
        font,
        RUNGIS_TOP,
        &RUNGIS_LINES,
        RUNGIS_LABEL,
        &board.rungis,
    );
    direction(
        &mut elements,
        font,
        ANTONY_TOP,
        &ANTONY_LINES,
        ANTONY_LABEL,
        &board.antony,
    );

    let slots = PEOPLE
        .into_iter()
        .zip(0..)
        .map(|(person, index)| {
            let person_state = household.get(&person).copied().unwrap_or(PersonState::Away);
            let center = Point::new(COLUMN_X, COLUMN_FIRST_Y + index * COLUMN_STEP);
            slot(person, person_state, center, &PHASE2_SIZES)
        })
        .collect();
    BoardLayout { elements, slots }
}

fn text(
    font: &Font,
    content: &str,
    size: u32,
    left: f32,
    top: f32,
    line_height: f32,
    ink: Ink,
) -> Text {
    Text {
        text: content.to_string(),
        size,
        left,
        top,
        line_height,
        width: text_width(font, content, size),
        ink,
    }
}

/// Header (tags, then label) and the two departure cells of one direction.
fn direction(
    elements: &mut Vec<Element>,
    font: &Font,
    top: i32,
    lines: &[&str],
    label: &str,
    departures: &[Time],
) {
    let row_center = top + HEADER_HEIGHT / 2;
    let mut x = MARGIN;
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            x += TAG_GAP;
        }
        let text_w = text_width(font, line, TAG_SIZE);
        let width = (text_w + 2.0 * TAG_PADDING_X).round() as u32;
        let height = TAG_SIZE + 2 * TAG_PADDING_Y;
        elements.push(Element::Rect(Rect {
            x,
            y: row_center - height as i32 / 2,
            width,
            height,
            radius: TAG_RADIUS,
        }));
        let left = x as f32 + (width as f32 - text_w) / 2.0;
        let text_top = (row_center - TAG_SIZE as i32 / 2) as f32;
        elements.push(Element::Text(text(
            font,
            line,
            TAG_SIZE,
            left,
            text_top,
            TAG_SIZE as f32,
            Ink::White,
        )));
        x += width as i32;
    }
    let label_left = (x + LABEL_GAP) as f32;
    let label_top = row_center as f32 - LABEL_LINE_HEIGHT / 2.0;
    elements.push(Element::Text(text(
        font,
        label,
        LABEL_SIZE,
        label_left,
        label_top,
        LABEL_LINE_HEIGHT,
        Ink::Black,
    )));

    for cell in 0..2 {
        let right = MARGIN + cell * (CELL_WIDTH + CELL_GAP) + CELL_WIDTH;
        let content = departures
            .get(cell as usize)
            .map_or(NO_DEPARTURE.to_string(), Time::to_string);
        let width = text_width(font, &content, TIME_SIZE);
        let time_top = (top + TIMES_OFFSET) as f32;
        elements.push(Element::Text(text(
            font,
            &content,
            TIME_SIZE,
            right as f32 - width,
            time_top,
            TIME_SIZE as f32,
            Ink::Black,
        )));
    }
}
```

```bash
cargo test --lib board
```

Expected: `12 passed`.

Notes:

- The layout needs the font: right-aligned times and tags side by side
  depend on text widths. Widths are the sum of the advance widths, without
  kerning, exactly as the mockup export measures them.
- Text positions follow the mockup's CSS: a line box (`top`,
  `line_height`) in which the glyph box (ascent + descent) is centred. The
  render computes the baseline from it.
- The tag width is rounded to a whole pixel, so the next tag and the label
  start on whole pixels.

## 3. `render`: drawing the board (tests 13–17)

### What the tests check

**D. Drawing, pixels**

13. `tags_are_white_text_on_black`: the left padding of the "319" tag is
    black, and its text area holds white pixels (the text).
14. `separators_are_drawn`: a black pixel on each line and at their
    junction; white above (y 40) and below (y 440) the vertical line, which
    is partial.
15. `times_are_right_aligned_in_pixels`: the rightmost black pixel of
    "8:08" is less than 8 px from the right edge of its cell (x 299).
16. `no_departure_mark_is_drawn`: the cells without departure hold the dash
    (more than 200 black pixels), not white.
17. `phase_2_background_is_white`: corners of the screen, between the two
    blocks, between the vertical line and the circles.

### Red

In `src/render.rs`, add before `use crate::layout::{Circle, Ink, Slot};`:

```rust
use crate::board::BoardLayout;
```

after `pub fn render`, add:

```rust
/// Clears the target to white, then draws the phase 2 board.
pub fn render_board<D>(target: &mut D, board: &BoardLayout, font: &Font) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    todo!()
}
```

and, at the end of `mod tests` (before its last closing brace), add:

```rust
    // D. Phase 2 board

    mod board {
        use super::*;
        use crate::board::{Board, Time, board_layout};
        use crate::state::Person;

        fn at(hour: u8, minute: u8) -> Time {
            Time { hour, minute }
        }

        fn rendered_board(rungis: Vec<Time>, antony: Vec<Time>) -> Frame {
            let font = atkinson_bold();
            let board = Board {
                clock: at(8, 4),
                rungis,
                antony,
            };
            let household: HouseholdState = [(Person('R'), HomePassive), (Person('H'), HomeActive)]
                .into_iter()
                .collect();
            let mut frame = Frame::new();
            render_board(&mut frame, &board_layout(&board, &household, &font), &font).unwrap();
            frame
        }

        fn black_pixels(
            frame: &Frame,
            xs: std::ops::Range<i32>,
            ys: std::ops::Range<i32>,
        ) -> Vec<(i32, i32)> {
            ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
                .filter(|&(x, y)| frame.is_black(x, y))
                .collect()
        }

        #[test]
        fn tags_are_white_text_on_black() {
            let frame = rendered_board(vec![], vec![]);
            // The 319 tag: x from 24, y 126 to 162; its left padding is black.
            assert!(frame.is_black(28, 144));
            // Inside the tag, the text is white.
            let white = 34 * 28 - black_pixels(&frame, 36..70, 130..158).len();
            assert!(white > 100, "{white} white pixels in the tag");
        }

        #[test]
        fn separators_are_drawn() {
            let frame = rendered_board(vec![], vec![]);
            assert!(frame.is_black(300, 93));
            assert!(frame.is_black(632, 200));
            assert!(frame.is_black(632, 93));
            assert!(!frame.is_black(632, 40));
            assert!(!frame.is_black(632, 440));
        }

        #[test]
        fn times_are_right_aligned_in_pixels() {
            let frame = rendered_board(vec![at(8, 8), at(8, 31)], vec![]);
            let rightmost = black_pixels(&frame, 24..300, 172..268)
                .iter()
                .map(|&(x, _)| x)
                .max()
                .unwrap();
            assert!((291..=299).contains(&rightmost), "ends at x = {rightmost}");
        }

        #[test]
        fn no_departure_mark_is_drawn() {
            let frame = rendered_board(vec![at(8, 8)], vec![]);
            // Second Rungis cell, and both Antony cells.
            for (xs, ys) in [
                (331..607, 172..268),
                (24..300, 348..444),
                (331..607, 348..444),
            ] {
                let ink = black_pixels(&frame, xs.clone(), ys).len();
                assert!(ink > 200, "{ink} black pixels in cell {xs:?}");
            }
        }

        #[test]
        fn phase_2_background_is_white() {
            let frame = rendered_board(vec![at(8, 8), at(8, 31)], vec![at(8, 12), at(8, 21)]);
            for (x, y) in [
                (0, 0),
                (799, 0),
                (0, 479),
                (799, 479),
                (300, 284),
                (650, 240),
            ] {
                assert!(!frame.is_black(x, y), "({x}, {y}) is black");
            }
        }
    }
```

```bash
cargo test --lib render
```

Expected: the 5 new tests fail on `todo!()`; the 7 phase 1 tests pass.

### Green

Change the module comment (first line) to:

```rust
//! Draws the layouts (phase 1 row, phase 2 board) as black and white pixels.
```

Replace the imports at the top:

```rust
use embedded_graphics::primitives::{self, PrimitiveStyleBuilder, StrokeAlignment};
use fontdue::{Font, FontSettings};

use crate::board::BoardLayout;
use crate::layout::{Circle, Ink, Slot};
```

with:

```rust
use embedded_graphics::primitives::{
    self, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle, StrokeAlignment,
};
use fontdue::{Font, FontSettings, Metrics};

use crate::board::{BoardLayout, Element, Rect, Text};
use crate::layout::{Circle, Ink, Slot};
```

Replace the whole `pub fn render` and the `render_board` you just added
with (the drawing of a slot moves to `draw_slot`, shared by both phases):

```rust
/// Clears the target to white, then draws every slot.
pub fn render<D>(target: &mut D, slots: &[Slot], font: &Font) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    target.clear(BinaryColor::Off)?;
    for slot in slots {
        draw_slot(target, slot, font)?;
    }
    Ok(())
}

/// Clears the target to white, then draws the phase 2 board.
pub fn render_board<D>(target: &mut D, board: &BoardLayout, font: &Font) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    target.clear(BinaryColor::Off)?;
    for element in &board.elements {
        match element {
            Element::Rect(rect) => draw_rect(target, rect)?,
            Element::Text(text) => draw_text(target, text, font)?,
        }
    }
    for slot in &board.slots {
        draw_slot(target, slot, font)?;
    }
    Ok(())
}

fn draw_slot<D>(target: &mut D, slot: &Slot, font: &Font) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    for ring in &slot.rings {
        draw_circle(target, slot.center, ring)?;
    }
    draw_circle(target, slot.center, &slot.core)?;
    draw_letter(target, slot, font)
}
```

Replace `fn draw_letter` (the glyph drawing moves to `draw_glyph`, shared
with the text):

```rust
fn draw_letter<D>(target: &mut D, slot: &Slot, font: &Font) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let letter = &slot.letter;
    let size = letter.size as f32;
    let (x, baseline) = letter_origin(font, letter.glyph, size, slot.center);
    let (metrics, coverage) = font.rasterize(letter.glyph, size);
    draw_glyph(target, &metrics, &coverage, x, baseline, letter.ink)
}
```

and add after it:

```rust
fn draw_rect<D>(target: &mut D, rect: &Rect) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let area = Rectangle::new(
        Point::new(rect.x, rect.y),
        Size::new(rect.width, rect.height),
    );
    RoundedRectangle::with_equal_corners(area, Size::new(rect.radius, rect.radius))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target)
}

/// Draws a line of text glyph by glyph, each one advancing the pen by its
/// advance width (no kerning, as in the mockup export).
fn draw_text<D>(target: &mut D, text: &Text, font: &Font) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let size = text.size as f32;
    let line = font
        .horizontal_line_metrics(size)
        .expect("embedded font has horizontal metrics");
    // CSS line box: the glyph box (ascent + |descent|) centred in `line_height`.
    let baseline = text.top + (text.line_height - (line.ascent - line.descent)) / 2.0 + line.ascent;
    let mut pen = text.left;
    for glyph in text.text.chars() {
        let (metrics, coverage) = font.rasterize(glyph, size);
        draw_glyph(target, &metrics, &coverage, pen, baseline, text.ink)?;
        pen += metrics.advance_width;
    }
    Ok(())
}

/// Draws the pixels a glyph covers enough, its advance starting at `x`.
fn draw_glyph<D>(
    target: &mut D,
    metrics: &Metrics,
    coverage: &[u8],
    x: f32,
    baseline: f32,
    ink: Ink,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    // fontdue's `ymin` is the bitmap's bottom edge, upwards from the baseline.
    let left = (x + metrics.xmin as f32).round() as i32;
    let top = (baseline - (metrics.ymin + metrics.height as i32) as f32).round() as i32;
    let ink = color(ink);
    let pixels = coverage
        .iter()
        .enumerate()
        .filter(|&(_, &value)| value >= COVERAGE_THRESHOLD)
        .map(|(index, _)| {
            let (column, row) = (index % metrics.width, index / metrics.width);
            Pixel(Point::new(left + column as i32, top + row as i32), ink)
        });
    target.draw_iter(pixels)
}
```

```bash
cargo test --lib
```

Expected: `157 passed`.

## 4. Golden image (test 18)

**E. Golden image**

18. `phase_2_render_matches_the_golden_image`: the mockup state (8:04;
    Rungis 8:08 and 8:31; Antony 8:12 and 8:21; R home, H with activity)
    renders exactly as `tests/golden/phase2-buses-presence.png`. As for
    phase 1, the image is first approved by eye against the SVG.

In `src/mockups.rs`, change the module comment and the imports:

```rust
//! The states shown by the mockups (`docs/mockups/*.svg`), shared by the
//! `mockups` example and the golden image tests.

use crate::board::{Board, Time};
use crate::refresh::HouseholdState;
```

and add at the end:

```rust
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
```

In `tests/golden.rs`, replace the `use e_paper::…` lines with:

```rust
use e_paper::board::board_layout;
use e_paper::frame::Frame;
use e_paper::layout::layout;
use e_paper::mockups::{phase1_states, phase2_state};
use e_paper::render::{atkinson_bold, render, render_board};
```

and add at the end:

```rust
#[test]
fn phase_2_render_matches_the_golden_image() {
    let font = atkinson_bold();
    let (name, board, household) = phase2_state();
    let path = Path::new("tests/golden").join(format!("{name}.png"));
    let golden = Frame::from_png(&fs::read(&path).unwrap()).unwrap();

    let mut frame = Frame::new();
    render_board(&mut frame, &board_layout(&board, &household, &font), &font).unwrap();

    assert!(
        frame == golden,
        "{name}: the render differs from {}",
        path.display()
    );
}
```

```bash
cargo test --test golden
```

Expected: the new test fails: `tests/golden/phase2-buses-presence.png` does
not exist yet.

In `examples/mockups.rs`, render phase 2 as well. Replace the module comment
and the imports:

```rust
//! Renders the mockup states (`docs/mockups/*.svg`).
//!
//! ```text
//! cargo run --example mockups -- png [DIR]   write the PNGs (default: target/mockups)
//! cargo run --example mockups -- show NAME   open one state in the simulator
//! ```
//!
//! NAME is one of `phase1-r-home`, `phase1-all-away`, `phase1-activity-ring`,
//! `phase2-buses-presence`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::{env, fs};

use e_paper::board::board_layout;
use e_paper::frame::Frame;
use e_paper::layout::{HEIGHT, WIDTH, layout};
use e_paper::mockups::{phase1_states, phase2_state};
use e_paper::render::{atkinson_bold, render, render_board};
```

At the end of `write_pngs`, before `Ok(())`, add:

```rust
    let (name, board, household) = phase2_state();
    let mut frame = Frame::new();
    render_board(&mut frame, &board_layout(&board, &household, &font), &font)?;
    let path = dir.join(format!("{name}.png"));
    fs::write(&path, frame.to_png())?;
    println!("{}", path.display());
```

In `show`, replace the lines from `let (_, state) = phase1_states()` to
`render(&mut display, …)?;` with:

```rust
    let font = atkinson_bold();
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(WIDTH, HEIGHT));
    let (phase2_name, board, household) = phase2_state();
    if name == phase2_name {
        render_board(
            &mut display,
            &board_layout(&board, &household, &font),
            &font,
        )?;
    } else {
        let (_, state) = phase1_states()
            .into_iter()
            .find(|(candidate, _)| *candidate == name)
            .ok_or(format!("unknown mockup: {name}"))?;
        render(&mut display, &layout(&state), &font)?;
    }
```

Render the PNGs, then look at the phase 2 one:

```bash
cargo run --example mockups -- png
```

```bash
cargo run --example mockups -- show phase2-buses-presence
```

Compare it with `docs/mockups/phase2-buses-presence.svg` (open both side by
side): same layout, same sizes; the PNG is 1-bit, so the edges are sharper.
If it is right, make it the golden image:

```bash
cp target/mockups/phase2-buses-presence.png tests/golden/
```

```bash
cargo test --test golden
```

Expected: `2 passed`.

## 5. Checks

```bash
cargo fmt
```

```bash
cargo test
```

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: `157 passed` for the library, `2 passed` for the golden images, no
clippy warning.

## 6. Commit

```bash
git add -A
```

```bash
git status
```

```bash
git commit -m "feat: phase 2 render (clock, departures per direction, presence column)"
```

Expected files: `docs/guide-11-phase2-render.md`, `src/board.rs`,
`tests/golden/phase2-buses-presence.png`, plus `examples/mockups.rs`,
`src/layout.rs`, `src/lib.rs`, `src/mockups.rs`, `src/render.rs`,
`tests/golden.rs`.

## Report

The output of section 5, and your opinion of the PNG against the SVG.
