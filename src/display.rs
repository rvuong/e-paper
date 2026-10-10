//! Where a frame goes: the e-paper panel, or a PNG file on the dev PC.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal::spi::SpiDevice;

use crate::epd::Epd7in5V2;
use crate::frame::Frame;
use crate::layout::Window;

/// How to bring the display to a new frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update<'a> {
    /// Full refresh: the panel flashes.
    Full,
    /// Partial refresh of the window, from the frame currently shown.
    Partial { previous: &'a Frame, window: Window },
}

/// Something that shows a frame.
pub trait Display {
    fn show(&mut self, frame: &Frame, update: Update) -> Result<(), Box<dyn Error>>;
}

/// Writes each frame into a PNG file.
pub struct PngDisplay {
    pub path: PathBuf,
}

/// A PNG file has no partial refresh: the whole frame is always written.
impl Display for PngDisplay {
    fn show(&mut self, frame: &Frame, _update: Update) -> Result<(), Box<dyn Error>> {
        fs::write(&self.path, frame.to_png())
            .map_err(|error| format!("cannot write {}: {error}", self.path.display()))?;
        Ok(())
    }
}

/// Each frame: power and init, full or partial refresh, deep sleep and power
/// off.
impl<PWR, RST, DC, SPI, BUSY, DELAY> Display for Epd7in5V2<PWR, RST, DC, SPI, BUSY, DELAY>
where
    PWR: OutputPin,
    RST: OutputPin,
    DC: OutputPin,
    SPI: SpiDevice,
    BUSY: InputPin,
    DELAY: DelayNs,
{
    fn show(&mut self, frame: &Frame, update: Update) -> Result<(), Box<dyn Error>> {
        match update {
            Update::Full => {
                self.init()?;
                self.display(frame)?;
            }
            Update::Partial { previous, window } => {
                self.init_part()?;
                self.display_partial(previous, frame, window)?;
            }
        }
        self.sleep()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epd::fake::{Op, fake_epd, ops};
    use embedded_graphics::pixelcolor::BinaryColor;
    use embedded_graphics::prelude::*;

    #[test]
    fn png_display_writes_the_frame() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("display.png");
        let mut frame = Frame::new();
        frame
            .draw_iter([Pixel(Point::new(10, 20), BinaryColor::On)])
            .unwrap();

        PngDisplay { path: path.clone() }
            .show(&frame, Update::Full)
            .unwrap();

        assert_eq!(Frame::from_png(&fs::read(&path).unwrap()).unwrap(), frame);
    }

    #[test]
    fn epd_display_inits_refreshes_then_sleeps() {
        let (mut epd, log) = fake_epd([], true);
        epd.show(&Frame::new(), Update::Full).unwrap();
        let ops = ops(&log.events());
        let position = |op: Op| ops.iter().position(|candidate| *candidate == op).unwrap();
        let power_on = position(Op::Pin("PWR", true));
        let init = position(Op::Command(0x06));
        let refresh = position(Op::Command(0x12));
        let deep_sleep = position(Op::Command(0x07));
        assert!(power_on < init && init < refresh && refresh < deep_sleep);
        assert_eq!(ops.last(), Some(&Op::Pin("PWR", false)));
    }

    const WINDOW: Window = Window {
        x: 16,
        y: 151,
        width: 184,
        height: 178,
    };

    #[test]
    fn png_display_writes_the_whole_frame_for_a_partial_update() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("display.png");
        let mut frame = Frame::new();
        frame
            .draw_iter([Pixel(Point::new(700, 20), BinaryColor::On)])
            .unwrap();
        let update = Update::Partial {
            previous: &Frame::new(),
            window: WINDOW,
        };

        PngDisplay { path: path.clone() }
            .show(&frame, update)
            .unwrap();

        assert_eq!(Frame::from_png(&fs::read(&path).unwrap()).unwrap(), frame);
    }

    #[test]
    fn epd_partial_update_inits_part_refreshes_the_window_then_sleeps() {
        let (mut epd, log) = fake_epd([], true);
        let previous = Frame::new();
        let update = Update::Partial {
            previous: &previous,
            window: WINDOW,
        };
        epd.show(&Frame::new(), update).unwrap();
        let ops = ops(&log.events());
        let position = |op: Op| ops.iter().position(|candidate| *candidate == op).unwrap();
        let init_part = position(Op::Command(0x21)); // partial waveforms loaded
        let partial = position(Op::Command(0x91));
        let deep_sleep = position(Op::Command(0x07));
        assert!(init_part < partial && partial < deep_sleep);
        assert!(!ops.contains(&Op::Command(0x06)), "no full init");
        assert_eq!(ops.last(), Some(&Op::Pin("PWR", false)));
    }
}
