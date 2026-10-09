//! In-memory 1-bit image of the display, with PNG export and import.

use std::convert::Infallible;
use std::error::Error;

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

use crate::layout::{HEIGHT, WIDTH};

/// An 800×480 black and white image. `BinaryColor::On` is black.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Row by row, `true` for black.
    pixels: Vec<bool>,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    /// A white frame.
    pub fn new() -> Self {
        Self {
            pixels: vec![false; (WIDTH * HEIGHT) as usize],
        }
    }

    /// Whether the pixel is black. Pixels outside the frame are white.
    pub fn is_black(&self, x: i32, y: i32) -> bool {
        Self::index(x, y).is_some_and(|index| self.pixels[index])
    }

    fn index(x: i32, y: i32) -> Option<usize> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        (x < WIDTH && y < HEIGHT).then(|| (y * WIDTH + x) as usize)
    }

    /// Encodes the frame as a 1-bit grayscale PNG.
    pub fn to_png(&self) -> Vec<u8> {
        // 1-bit grayscale: 8 pixels per byte, most significant bit first, 1 is white.
        let mut data = vec![0u8; (WIDTH / 8 * HEIGHT) as usize];
        for (index, &black) in self.pixels.iter().enumerate() {
            if !black {
                data[index / 8] |= 0x80 >> (index % 8);
            }
        }
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::One);
        let mut writer = encoder.write_header().expect("valid PNG header");
        writer.write_image_data(&data).expect("valid PNG data");
        writer.finish().expect("PNG written to memory");
        png
    }

    /// Decodes an 800×480 PNG; dark pixels (below mid-grey) are black.
    pub fn from_png(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        // Expands every format to 8 bits per channel; the first channel is enough.
        decoder.set_transformations(png::Transformations::EXPAND);
        let mut reader = decoder.read_info()?;
        let mut buffer = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
        let info = reader.next_frame(&mut buffer)?;
        if (info.width, info.height) != (WIDTH, HEIGHT) {
            return Err(format!(
                "expected {WIDTH}×{HEIGHT}, got {}×{}",
                info.width, info.height
            )
            .into());
        }
        let channels = info.line_size / WIDTH as usize;
        let pixels = buffer[..info.buffer_size()]
            .chunks(info.line_size)
            .flat_map(|row| row.chunks(channels).map(|pixel| pixel[0] < 128))
            .collect();
        Ok(Self { pixels })
    }
}

impl OriginDimensions for Frame {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl DrawTarget for Frame {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if let Some(index) = Self::index(point.x, point.y) {
                self.pixels[index] = color == BinaryColor::On;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip_keeps_size_and_pixels() {
        let mut frame = Frame::new();
        let black = [Point::new(0, 0), Point::new(400, 240), Point::new(799, 479)];
        frame
            .draw_iter(black.map(|point| Pixel(point, BinaryColor::On)))
            .unwrap();

        let decoded = Frame::from_png(&frame.to_png()).unwrap();

        assert_eq!(decoded, frame);
        assert_eq!(decoded.size(), Size::new(800, 480));
        assert!(black.iter().all(|point| decoded.is_black(point.x, point.y)));
        assert!(!decoded.is_black(1, 0));
    }
}
