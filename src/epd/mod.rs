//! Driver for the Waveshare 7.5" V2 black and white e-paper panel (800×480).
//!
//! Ported from the Waveshare Python driver `epd7in5_V2.py`, which drove this
//! panel in the previous project. Generic over the `embedded-hal` traits: the
//! Raspberry Pi hardware is in `linux`, a recording fake for the tests in `fake`.

#[cfg(test)]
pub mod fake;
pub mod linux;

use std::fmt;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal::spi::SpiDevice;

use crate::frame::Frame;
use crate::layout::{HEIGHT, WIDTH};

/// Give up waiting for the panel after this long.
const BUSY_TIMEOUT_MS: u32 = 30_000;
/// Pause between two reads of BUSY.
const BUSY_POLL_MS: u32 = 10;
/// Largest SPI write: the default `spidev` buffer size.
const SPI_CHUNK: usize = 4096;

/// What can go wrong with the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpdError {
    /// SPI or GPIO failure.
    Hardware(String),
    /// The panel stayed busy longer than `BUSY_TIMEOUT_MS`.
    BusyTimeout,
}

impl fmt::Display for EpdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hardware(detail) => write!(f, "e-paper hardware error: {detail}"),
            Self::BusyTimeout => write!(f, "e-paper still busy after {BUSY_TIMEOUT_MS} ms"),
        }
    }
}

impl std::error::Error for EpdError {}

fn hardware<E: fmt::Debug>(error: E) -> EpdError {
    EpdError::Hardware(format!("{error:?}"))
}

/// The frame as the panel expects it: 100 bytes per row, most significant bit
/// on the left, 1 for black.
pub fn panel_buffer(frame: &Frame) -> Vec<u8> {
    let mut buffer = vec![0u8; (WIDTH / 8 * HEIGHT) as usize];
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            if frame.is_black(x, y) {
                let index = (y as u32 * WIDTH + x as u32) as usize;
                buffer[index / 8] |= 0x80 >> (index % 8);
            }
        }
    }
    buffer
}

/// The panel and the pins of its HAT.
pub struct Epd7in5V2<PWR, RST, DC, SPI, BUSY, DELAY> {
    /// Power of the HAT (GPIO 18).
    pwr: PWR,
    rst: RST,
    /// Low for a command, high for data.
    dc: DC,
    spi: SPI,
    /// Low while the panel is busy.
    busy: BUSY,
    delay: DELAY,
}

impl<PWR, RST, DC, SPI, BUSY, DELAY> Epd7in5V2<PWR, RST, DC, SPI, BUSY, DELAY>
where
    PWR: OutputPin,
    RST: OutputPin,
    DC: OutputPin,
    SPI: SpiDevice,
    BUSY: InputPin,
    DELAY: DelayNs,
{
    pub fn new(pwr: PWR, rst: RST, dc: DC, spi: SPI, busy: BUSY, delay: DELAY) -> Self {
        Self {
            pwr,
            rst,
            dc,
            spi,
            busy,
            delay,
        }
    }

    /// Powers the HAT, resets the panel and sends the init sequence.
    pub fn init(&mut self) -> Result<(), EpdError> {
        self.power_on_and_reset()?;
        self.command(0x06)?; // booster soft start
        self.data(&[0x17, 0x17, 0x28, 0x17])?;
        self.command(0x01)?; // power setting: VGH 20 V, VGL -20 V, VDH 15 V, VDL -15 V
        self.data(&[0x07, 0x07, 0x3f, 0x3f])?;
        self.command(0x04)?; // power on
        self.delay.delay_ms(100);
        self.wait_busy()?;
        self.command(0x00)?; // panel setting: black and white
        self.data(&[0x1f])?;
        self.command(0x61)?; // resolution: 800 (0x0320) × 480 (0x01e0)
        self.data(&[0x03, 0x20, 0x01, 0xe0])?;
        self.command(0x15)?;
        self.data(&[0x00])?;
        self.command(0x50)?; // VCOM and data interval
        self.data(&[0x10, 0x07])?;
        self.command(0x60)?; // TCON
        self.data(&[0x22])
    }

    /// Full refresh with the frame (several seconds, the panel flashes).
    pub fn display(&mut self, frame: &Frame) -> Result<(), EpdError> {
        let new = panel_buffer(frame);
        let old: Vec<u8> = new.iter().map(|byte| !byte).collect();
        self.command(0x10)?; // old data
        self.data(&old)?;
        self.command(0x13)?; // new data
        self.data(&new)?;
        self.command(0x12)?; // display refresh
        self.delay.delay_ms(100);
        self.wait_busy()
    }

    /// Full refresh with a white frame.
    pub fn clear(&mut self) -> Result<(), EpdError> {
        self.display(&Frame::new())
    }

    /// Deep sleep, then cuts the power of the HAT. The image stays.
    pub fn sleep(&mut self) -> Result<(), EpdError> {
        self.command(0x02)?; // power off
        self.wait_busy()?;
        self.command(0x07)?; // deep sleep
        self.data(&[0xa5])?;
        self.delay.delay_ms(2_000);
        self.rst.set_low().map_err(hardware)?;
        self.dc.set_low().map_err(hardware)?;
        self.pwr.set_low().map_err(hardware)
    }

    fn power_on_and_reset(&mut self) -> Result<(), EpdError> {
        self.pwr.set_high().map_err(hardware)?;
        self.rst.set_high().map_err(hardware)?;
        self.delay.delay_ms(20);
        self.rst.set_low().map_err(hardware)?;
        self.delay.delay_ms(2);
        self.rst.set_high().map_err(hardware)?;
        self.delay.delay_ms(20);
        Ok(())
    }

    fn command(&mut self, command: u8) -> Result<(), EpdError> {
        self.dc.set_low().map_err(hardware)?;
        self.spi.write(&[command]).map_err(hardware)
    }

    fn data(&mut self, bytes: &[u8]) -> Result<(), EpdError> {
        self.dc.set_high().map_err(hardware)?;
        for chunk in bytes.chunks(SPI_CHUNK) {
            self.spi.write(chunk).map_err(hardware)?;
        }
        Ok(())
    }

    /// Waits until BUSY goes high, asking the panel for its status (0x71).
    fn wait_busy(&mut self) -> Result<(), EpdError> {
        let mut waited = 0;
        loop {
            self.command(0x71)?; // get status
            if self.busy.is_high().map_err(hardware)? {
                break;
            }
            if waited >= BUSY_TIMEOUT_MS {
                return Err(EpdError::BusyTimeout);
            }
            self.delay.delay_ms(BUSY_POLL_MS);
            waited += BUSY_POLL_MS;
        }
        self.delay.delay_ms(20);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{Event, Op, fake_epd, ops};
    use super::*;
    use embedded_graphics::pixelcolor::BinaryColor;
    use embedded_graphics::prelude::*;

    const BUFFER_SIZE: usize = 800 / 8 * 480;

    fn frame_with_black(points: &[(i32, i32)]) -> Frame {
        let mut frame = Frame::new();
        frame
            .draw_iter(
                points
                    .iter()
                    .map(|&(x, y)| Pixel(Point::new(x, y), BinaryColor::On)),
            )
            .unwrap();
        frame
    }

    /// The data sent right after `command`.
    fn data_after(ops: &[Op], command: u8) -> Vec<u8> {
        let index = ops
            .iter()
            .position(|op| *op == Op::Command(command))
            .unwrap();
        match &ops[index + 1] {
            Op::Data(data) => data.clone(),
            other => panic!("expected data after {command:#04x}, got {other:?}"),
        }
    }

    #[test]
    fn white_frame_is_all_zero() {
        let buffer = panel_buffer(&Frame::new());
        assert_eq!(buffer.len(), BUFFER_SIZE);
        assert!(buffer.iter().all(|&byte| byte == 0x00));
    }

    #[test]
    fn black_pixels_set_bits_left_to_right_row_by_row() {
        let buffer = panel_buffer(&frame_with_black(&[(0, 0)]));
        assert_eq!(buffer[0], 0x80);
        let buffer = panel_buffer(&frame_with_black(&[(7, 0)]));
        assert_eq!(buffer[0], 0x01);
        let buffer = panel_buffer(&frame_with_black(&[(8, 0)]));
        assert_eq!((buffer[0], buffer[1]), (0x00, 0x80));
        let buffer = panel_buffer(&frame_with_black(&[(0, 1)]));
        assert_eq!((buffer[0], buffer[100]), (0x00, 0x80));
    }

    #[test]
    fn old_data_is_the_inverse_of_new_data() {
        let (mut epd, log) = fake_epd([], true);
        epd.display(&frame_with_black(&[(0, 0), (799, 479)]))
            .unwrap();
        let ops = ops(&log.events());
        let new = data_after(&ops, 0x13);
        let old = data_after(&ops, 0x10);
        assert_eq!(new.len(), BUFFER_SIZE);
        assert_eq!(old, new.iter().map(|byte| !byte).collect::<Vec<u8>>());
    }

    #[test]
    fn init_powers_the_hat_and_resets_the_panel() {
        let (mut epd, log) = fake_epd([], true);
        epd.init().unwrap();
        assert_eq!(
            ops(&log.events())[..7],
            [
                Op::Pin("PWR", true),
                Op::Pin("RST", true),
                Op::DelayMs(20),
                Op::Pin("RST", false),
                Op::DelayMs(2),
                Op::Pin("RST", true),
                Op::DelayMs(20),
            ]
        );
    }

    #[test]
    fn init_sends_the_waveshare_sequence() {
        let (mut epd, log) = fake_epd([], true);
        epd.init().unwrap();
        let data = |bytes: &[u8]| Op::Data(bytes.to_vec());
        assert_eq!(
            ops(&log.events())[7..],
            [
                Op::Command(0x06), // booster soft start
                data(&[0x17, 0x17, 0x28, 0x17]),
                Op::Command(0x01), // power setting
                data(&[0x07, 0x07, 0x3f, 0x3f]),
                Op::Command(0x04), // power on
                Op::DelayMs(100),
                Op::Command(0x71), // busy wait: ready at once
                Op::DelayMs(20),
                Op::Command(0x00), // panel setting
                data(&[0x1f]),
                Op::Command(0x61), // resolution 800×480
                data(&[0x03, 0x20, 0x01, 0xe0]),
                Op::Command(0x15),
                data(&[0x00]),
                Op::Command(0x50), // VCOM and data interval
                data(&[0x10, 0x07]),
                Op::Command(0x60), // TCON
                data(&[0x22]),
            ]
        );
    }

    #[test]
    fn busy_wait_polls_until_busy_goes_high() {
        let (mut epd, log) = fake_epd([false, false], true);
        epd.wait_busy().unwrap();
        assert_eq!(
            ops(&log.events()),
            [
                Op::Command(0x71),
                Op::DelayMs(10),
                Op::Command(0x71),
                Op::DelayMs(10),
                Op::Command(0x71),
                Op::DelayMs(20),
            ]
        );
    }

    #[test]
    fn busy_stuck_low_times_out() {
        let (mut epd, log) = fake_epd([], false);
        assert_eq!(epd.wait_busy(), Err(EpdError::BusyTimeout));
        let waited: u32 = ops(&log.events())
            .iter()
            .map(|op| match op {
                Op::DelayMs(ms) => *ms,
                _ => 0,
            })
            .sum();
        assert_eq!(waited, 30_000);
    }

    #[test]
    fn display_sends_old_and_new_data_then_refreshes() {
        let (mut epd, log) = fake_epd([], true);
        epd.display(&Frame::new()).unwrap();
        assert_eq!(
            ops(&log.events()),
            [
                Op::Command(0x10),
                Op::Data(vec![0xff; BUFFER_SIZE]),
                Op::Command(0x13),
                Op::Data(vec![0x00; BUFFER_SIZE]),
                Op::Command(0x12), // display refresh
                Op::DelayMs(100),
                Op::Command(0x71),
                Op::DelayMs(20),
            ]
        );
    }

    #[test]
    fn sleep_powers_off_the_panel_then_the_hat() {
        let (mut epd, log) = fake_epd([], true);
        epd.sleep().unwrap();
        assert_eq!(
            ops(&log.events()),
            [
                Op::Command(0x02), // power off
                Op::Command(0x71),
                Op::DelayMs(20),
                Op::Command(0x07), // deep sleep
                Op::Data(vec![0xa5]),
                Op::DelayMs(2_000),
                Op::Pin("RST", false),
                Op::Pin("PWR", false),
            ]
        );
        // DC is low too (hidden by `ops`, which uses it to tell commands from data).
        assert!(log.events().ends_with(&[
            Event::Pin("RST", false),
            Event::Pin("DC", false),
            Event::Pin("PWR", false),
        ]));
    }

    #[test]
    fn clear_displays_a_white_frame() {
        let (mut epd, log) = fake_epd([], true);
        epd.clear().unwrap();
        let ops = ops(&log.events());
        assert_eq!(data_after(&ops, 0x13), vec![0x00; BUFFER_SIZE]);
        assert!(ops.contains(&Op::Command(0x12)));
    }

    #[test]
    fn large_data_is_written_in_chunks_of_at_most_4096_bytes() {
        let (mut epd, log) = fake_epd([], true);
        let bytes: Vec<u8> = (0..10_000u32).map(|i| i as u8).collect();
        epd.data(&bytes).unwrap();
        let writes: Vec<Vec<u8>> = log
            .events()
            .into_iter()
            .filter_map(|event| match event {
                Event::Spi(bytes) => Some(bytes),
                _ => None,
            })
            .collect();
        assert!(writes.iter().all(|write| write.len() <= 4096));
        assert_eq!(writes.len(), 3);
        assert_eq!(writes.concat(), bytes);
    }

    #[test]
    fn dc_is_low_for_commands_and_high_for_data() {
        let (mut epd, log) = fake_epd([], true);
        epd.command(0x12).unwrap();
        epd.data(&[0x01, 0x02]).unwrap();
        assert_eq!(
            log.events(),
            [
                Event::Pin("DC", false),
                Event::Spi(vec![0x12]),
                Event::Pin("DC", true),
                Event::Spi(vec![0x01, 0x02]),
            ]
        );
    }
}
