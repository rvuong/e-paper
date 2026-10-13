//! Draws the layouts (phase 1 row, phase 2 board) as black and white pixels.

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{
    self, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle, StrokeAlignment,
};
use fontdue::{Font, FontSettings, Metrics};

use crate::board::{BoardLayout, Element, Rect, Text};
use crate::layout::{Circle, Ink, Slot};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/AtkinsonHyperlegible-Bold.ttf");

/// Minimum glyph coverage (0 to 255) for a pixel to take the letter's ink.
const COVERAGE_THRESHOLD: u8 = 128;

/// Atkinson Hyperlegible Bold, embedded in the binary.
pub fn atkinson_bold() -> Font {
    Font::from_bytes(FONT_BYTES, FontSettings::default()).expect("embedded font is valid")
}

/// Where to draw a letter centred on `center`, with the mockups' rule:
/// horizontally on its advance width, vertically on a line box as high as the
/// font size. Returns the left end of the advance and the baseline.
pub fn letter_origin(font: &Font, glyph: char, size: f32, center: Point) -> (f32, f32) {
    let advance = font.metrics(glyph, size).advance_width;
    let line = font
        .horizontal_line_metrics(size)
        .expect("embedded font has horizontal metrics");
    // `descent` is negative: the baseline sits below the centre by half of
    // (ascent − |descent|).
    (
        center.x as f32 - advance / 2.0,
        center.y as f32 + (line.ascent + line.descent) / 2.0,
    )
}

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

fn color(ink: Ink) -> BinaryColor {
    match ink {
        Ink::Black => BinaryColor::On,
        Ink::White => BinaryColor::Off,
    }
}

fn draw_circle<D>(target: &mut D, center: Point, circle: &Circle) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut style = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(circle.stroke_width)
        .stroke_alignment(StrokeAlignment::Inside);
    if circle.filled {
        style = style.fill_color(BinaryColor::On);
    }
    // As in the mockups, `center` is a point between pixels: a circle of even
    // diameter d spans d whole pixels from center − d/2. (`Circle::with_center`
    // would put the centre on a pixel and shift the circle by one.)
    let radius = circle.diameter as i32 / 2;
    primitives::Circle::new(center - Point::new(radius, radius), circle.diameter)
        .into_styled(style.build())
        .draw(target)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Frame;
    use crate::layout::{PEOPLE, layout};
    use crate::refresh::HouseholdState;
    use crate::state::PersonState::{self, Away, HomeActive, HomePassive};

    /// Centre of R's slot.
    const CX: i32 = 109;
    const CY: i32 = 240;

    fn rendered(states: [PersonState; 4]) -> Frame {
        let state: HouseholdState = PEOPLE.into_iter().zip(states).collect();
        let mut frame = Frame::new();
        render(&mut frame, &layout(&state), &atkinson_bold()).unwrap();
        frame
    }

    /// Whether the pixel `distance` px left of R's centre is black.
    fn black_left_of_center(frame: &Frame, distance: i32) -> bool {
        frame.is_black(CX - distance, CY)
    }

    #[test]
    fn letter_origin_follows_the_mockup_rule() {
        // From the font tables: advance of R = 654/1000 em, hhea ascent 950,
        // descent -290. x = 109 - 0.654 × 88 / 2, baseline = 240 + (0.95 - 0.29) × 88 / 2.
        let (x, baseline) = letter_origin(&atkinson_bold(), 'R', 88.0, Point::new(CX, CY));
        assert!((x - 80.224).abs() < 0.01, "x = {x}");
        assert!((baseline - 269.04).abs() < 0.01, "baseline = {baseline}");
    }

    #[test]
    fn away_draws_a_black_outline_around_a_white_inside() {
        let frame = rendered([Away; 4]);
        assert!(black_left_of_center(&frame, 59));
        assert!(frame.is_black(CX, CY - 59));
        assert!(!black_left_of_center(&frame, 45));
    }

    #[test]
    fn home_fills_the_circle_with_black() {
        let frame = rendered([HomePassive, Away, Away, Away]);
        assert!(black_left_of_center(&frame, 45));
        assert!(black_left_of_center(&frame, 59));
    }

    #[test]
    fn activity_draws_two_black_rings_separated_by_white_gaps() {
        let frame = rendered([HomeActive, Away, Away, Away]);
        assert!(!black_left_of_center(&frame, 68));
        assert!(black_left_of_center(&frame, 74));
        assert!(!black_left_of_center(&frame, 80));
        assert!(black_left_of_center(&frame, 86));
    }

    #[test]
    fn no_rings_without_activity() {
        let frame = rendered([HomePassive, Away, Away, Away]);
        assert!(!black_left_of_center(&frame, 74));
        assert!(!black_left_of_center(&frame, 86));
    }

    #[test]
    fn letter_is_drawn_around_the_center() {
        let frame = rendered([Away; 4]);
        let ink: Vec<(i32, i32)> = (CY - 45..=CY + 45)
            .flat_map(|y| (CX - 45..=CX + 45).map(move |x| (x, y)))
            .filter(|&(x, y)| (x - CX).pow(2) + (y - CY).pow(2) < 45 * 45)
            .filter(|&(x, y)| frame.is_black(x, y))
            .collect();
        assert!(ink.len() > 500, "{} black pixels", ink.len());
        let (min_x, max_x) = (ink.iter().map(|p| p.0).min(), ink.iter().map(|p| p.0).max());
        let (min_y, max_y) = (ink.iter().map(|p| p.1).min(), ink.iter().map(|p| p.1).max());
        let center_x = (min_x.unwrap() + max_x.unwrap()) / 2;
        let center_y = (min_y.unwrap() + max_y.unwrap()) / 2;
        assert!((center_x - CX).abs() <= 10, "ink centred at x = {center_x}");
        assert!((center_y - CY).abs() <= 10, "ink centred at y = {center_y}");
    }

    #[test]
    fn background_is_white() {
        let frame = rendered([HomeActive; 4]);
        for (x, y) in [
            (0, 0),
            (799, 0),
            (0, 479),
            (799, 479),
            (400, 20),
            (400, 460),
        ] {
            assert!(!frame.is_black(x, y), "({x}, {y}) is black");
        }
    }

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
}
