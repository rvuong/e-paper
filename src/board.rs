//! Phase 2 layout: clock, next departures per direction and presence column,
//! without drawing them.
//!
//! Geometry from the mockup (`docs/mockups/phase2-buses-presence.svg`,
//! guide 11). Text widths come from the font: the layout
//! needs them to right-align the times and to put the tags side by side.

use std::fmt;

use embedded_graphics::prelude::Point;
use fontdue::Font;

use crate::icons::{ICON_SIZE, Icon};
use crate::layout::{Ink, PEOPLE, Slot, SlotSizes, slot};
use crate::refresh::HouseholdState;
use crate::state::PersonState;

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
/// Shown in a cell without departure.
pub const NO_DEPARTURE: &str = "—";

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

/// A time of day, local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
}

impl fmt::Display for Time {
    /// "8:04": no leading zero on the hour, two digits for the minutes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{:02}", self.hour, self.minute)
    }
}

/// What a direction block shows below its header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// The next departures, already chosen and sorted; only the first two
    /// are shown, a missing one as a dash.
    Times(Vec<Time>),
    /// The service is over, or not started yet.
    Moon,
    /// The bus data is unavailable.
    Wrench,
}

/// What the bus part shows: the clock and each direction's block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub clock: Time,
    pub rungis: Block,
    pub antony: Block,
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

/// A pictogram, `ICON_SIZE` px square, from its top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pictogram {
    pub icon: Icon,
    pub x: i32,
    pub y: i32,
}

/// Something to draw, in drawing order.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Rect(Rect),
    Text(Text),
    Pictogram(Pictogram),
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
    block: &Block,
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

    let time_top = top + TIMES_OFFSET;
    let icon = match block {
        Block::Times(departures) => return times(elements, font, departures, time_top),
        Block::Moon => Icon::Moon,
        Block::Wrench => Icon::Wrench,
    };
    // Right-aligned in the first cell, like a time; the second cell stays
    // empty.
    elements.push(Element::Pictogram(Pictogram {
        icon,
        x: MARGIN + CELL_WIDTH - ICON_SIZE as i32,
        y: time_top,
    }));
}

/// The two departure cells, a dash for a missing departure.
fn times(elements: &mut Vec<Element>, font: &Font, departures: &[Time], time_top: i32) {
    for cell in 0..2 {
        let right = MARGIN + cell * (CELL_WIDTH + CELL_GAP) + CELL_WIDTH;
        let content = departures
            .get(cell as usize)
            .map_or(NO_DEPARTURE.to_string(), Time::to_string);
        let width = text_width(font, &content, TIME_SIZE);
        elements.push(Element::Text(text(
            font,
            &content,
            TIME_SIZE,
            right as f32 - width,
            time_top as f32,
            TIME_SIZE as f32,
            Ink::Black,
        )));
    }
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
        blocks_layout(Block::Times(rungis), Block::Times(antony))
    }

    fn blocks_layout(rungis: Block, antony: Block) -> BoardLayout {
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
                _ => None,
            })
            .collect()
    }

    fn rects(layout: &BoardLayout) -> Vec<Rect> {
        layout
            .elements
            .iter()
            .filter_map(|element| match element {
                Element::Rect(rect) => Some(*rect),
                _ => None,
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

    fn pictograms(layout: &BoardLayout) -> Vec<Pictogram> {
        layout
            .elements
            .iter()
            .filter_map(|element| match element {
                Element::Pictogram(pictogram) => Some(*pictogram),
                _ => None,
            })
            .collect()
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

    #[test]
    fn moon_is_right_aligned_in_the_first_cell() {
        let layout = blocks_layout(Block::Moon, Block::Times(vec![at(21, 22)]));
        assert_eq!(
            pictograms(&layout),
            [Pictogram {
                icon: Icon::Moon,
                x: 203,
                y: 172
            }]
        );
        assert!(times_at(&layout, 172.0).is_empty());
        assert_eq!(times_at(&layout, 348.0).len(), 2);
    }

    #[test]
    fn wrench_is_in_both_blocks() {
        let layout = blocks_layout(Block::Wrench, Block::Wrench);
        assert_eq!(
            pictograms(&layout),
            [
                Pictogram {
                    icon: Icon::Wrench,
                    x: 203,
                    y: 172
                },
                Pictogram {
                    icon: Icon::Wrench,
                    x: 203,
                    y: 348
                },
            ]
        );
        assert!(times_at(&layout, 172.0).is_empty() && times_at(&layout, 348.0).is_empty());
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
