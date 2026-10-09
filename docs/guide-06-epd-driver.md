# Guide 06: e-paper driver

Our own driver for the Waveshare 7.5" V2 panel, ported from the Waveshare
Python driver that drove it in the previous project (`epd7in5_V2.py` and
`epdconfig.py`), then the `Display` trait with two outputs: PNG file and panel.

Approved decisions: own driver (port of the Python sequence);
`linux-embedded-hal` (SPI `/dev/spidev0.0`, GPIO `/dev/gpiochip0`); tests with
a home-made recording fake; BUSY timeout of 30 s (10 ms between reads); a
`Display` trait with PNG and e-paper outputs, `run --epd`, new `test-pattern`
and `clear` commands; full refresh only; SPI writes in chunks of at most
4096 bytes.

Approved tests: 14 (12 for the driver, 2 for the outputs). 89 unit tests in
total at the end, plus the golden image test.

Facts from the Python driver: SPI bus 0, chip select CE0 (handled by the
kernel), 4 MHz, mode 0. Pins (BCM numbers): RST 17, PWR 18 (power of the HAT:
high before use, low after), BUSY 24 (low while busy), DC 25 (low for a
command, high for data).

## 0. Stop the old program first

Two programs must never drive the panel at the same time. Disable the old
cron jobs now (step 3 of the decommissioning, see `CLAUDE.md`). Back up the
root crontab on the dev PC first (outside the repository):

```bash
mkdir -p ~/e-paper-backup
ssh epaper 'sudo crontab -l' > ~/e-paper-backup/root-crontab.txt
cat ~/e-paper-backup/root-crontab.txt
```

Then edit it on the Pi and put a `#` in front of the two `main.py` lines (the
06:00 one and the 22:00 one):

```bash
ssh -t epaper 'sudo crontab -e'
```

Check: both lines now start with `#`.

```bash
ssh epaper 'sudo crontab -l' | grep main.py
```

## 1. Dependencies

```bash
cargo add embedded-hal@1
cargo add linux-embedded-hal --no-default-features --features gpio_cdev,spi
```

- `embedded-hal`: the standard traits (`SpiDevice`, `OutputPin`, `InputPin`,
  `DelayNs`) the driver is written against.
- `linux-embedded-hal`: their implementation on Linux. Only the GPIO
  (character device) and SPI features: the default features include the serial
  port, which needs the C library `libudev` and would break the static build
  for the Pi.

## 2. The `epd` module: the driver (tests 1–12)

The module is a folder: `src/epd/mod.rs` (the driver), `src/epd/fake.rs`
(fake hardware, compiled for the tests only), `src/epd/linux.rs` (the Pi
hardware).

### 2.1 The fake hardware

Each fake part writes into a shared log: pin changes, SPI writes, delays.
`ops` then reads the log as the panel does: a byte sent with DC low is a
command, bytes sent with DC high are data. `Rc<RefCell<…>>` lets several
parts share one log in a test.

Create the file `src/epd/fake.rs` with this content:

```rust
//! Fake hardware for the driver tests: it records what the driver does.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::rc::Rc;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{self, InputPin, OutputPin};
use embedded_hal::spi::{self, Operation, SpiDevice};

use super::Epd7in5V2;

/// One low-level action of the driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Pin(&'static str, bool),
    Spi(Vec<u8>),
    DelayMs(u32),
    DelayNs(u32),
}

/// The events, shared by all the fake parts.
#[derive(Debug, Clone, Default)]
pub struct Log(Rc<RefCell<Vec<Event>>>);

impl Log {
    fn push(&self, event: Event) {
        self.0.borrow_mut().push(event);
    }

    pub fn events(&self) -> Vec<Event> {
        self.0.borrow().clone()
    }
}

pub struct FakeSpi(Log);

impl spi::ErrorType for FakeSpi {
    type Error = Infallible;
}

impl SpiDevice for FakeSpi {
    fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Infallible> {
        for operation in operations {
            if let Operation::Write(bytes) = operation {
                self.0.push(Event::Spi(bytes.to_vec()));
            }
        }
        Ok(())
    }
}

pub struct FakePin {
    name: &'static str,
    log: Log,
}

impl digital::ErrorType for FakePin {
    type Error = Infallible;
}

impl OutputPin for FakePin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        self.log.push(Event::Pin(self.name, false));
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        self.log.push(Event::Pin(self.name, true));
        Ok(())
    }
}

/// BUSY input: returns the queued levels, then `idle` forever.
pub struct FakeBusy {
    levels: VecDeque<bool>,
    idle: bool,
}

impl digital::ErrorType for FakeBusy {
    type Error = Infallible;
}

impl InputPin for FakeBusy {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        Ok(self.levels.pop_front().unwrap_or(self.idle))
    }

    fn is_low(&mut self) -> Result<bool, Infallible> {
        self.is_high().map(|high| !high)
    }
}

pub struct FakeDelay(Log);

impl DelayNs for FakeDelay {
    fn delay_ns(&mut self, ns: u32) {
        self.0.push(Event::DelayNs(ns));
    }

    fn delay_ms(&mut self, ms: u32) {
        self.0.push(Event::DelayMs(ms));
    }
}

pub type FakeEpd = Epd7in5V2<FakePin, FakePin, FakePin, FakeSpi, FakeBusy, FakeDelay>;

/// A driver on fake hardware. BUSY returns `busy_levels` first, then `idle`
/// (high: the panel is ready).
pub fn fake_epd(busy_levels: impl IntoIterator<Item = bool>, idle: bool) -> (FakeEpd, Log) {
    let log = Log::default();
    let pin = |name| FakePin {
        name,
        log: log.clone(),
    };
    let epd = Epd7in5V2::new(
        pin("PWR"),
        pin("RST"),
        pin("DC"),
        FakeSpi(log.clone()),
        FakeBusy {
            levels: busy_levels.into_iter().collect(),
            idle,
        },
        FakeDelay(log.clone()),
    );
    (epd, log)
}

/// What the panel receives, at the level of the datasheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Command(u8),
    Data(Vec<u8>),
    DelayMs(u32),
    /// A pin other than DC (DC only tells commands from data).
    Pin(&'static str, bool),
}

/// Turns the low-level events into commands and data: a byte sent with DC
/// low is a command, bytes sent with DC high are data (merged until the next
/// command).
pub fn ops(events: &[Event]) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut dc_high = false;
    for event in events {
        match event {
            Event::Pin("DC", level) => dc_high = *level,
            Event::Pin(name, level) => ops.push(Op::Pin(name, *level)),
            Event::Spi(bytes) if !dc_high => {
                ops.extend(bytes.iter().map(|&byte| Op::Command(byte)))
            }
            Event::Spi(bytes) => match ops.last_mut() {
                Some(Op::Data(data)) => data.extend(bytes),
                _ => ops.push(Op::Data(bytes.clone())),
            },
            Event::DelayMs(ms) => ops.push(Op::DelayMs(*ms)),
            Event::DelayNs(_) => {}
        }
    }
    ops
}
```

### 2.2 The Pi hardware

Not unit-tested (approved): it only opens the devices and the GPIO lines.
Checked on the real panel in section 6.

Create the file `src/epd/linux.rs` with this content:

```rust
//! The panel on the Raspberry Pi: SPI0 (chip select CE0) and the pins of the
//! Waveshare HAT, as in the Waveshare Python `epdconfig.py`. Not unit-tested:
//! checked on the real panel.

use std::error::Error;

use linux_embedded_hal::gpio_cdev::{Chip, LineRequestFlags};
use linux_embedded_hal::spidev::{SpiModeFlags, SpidevOptions};
use linux_embedded_hal::{CdevPin, Delay, SpidevDevice};

use super::Epd7in5V2;

const SPI_DEVICE: &str = "/dev/spidev0.0";
const SPI_SPEED_HZ: u32 = 4_000_000;
const GPIO_CHIP: &str = "/dev/gpiochip0";
const RST_PIN: u32 = 17;
const PWR_PIN: u32 = 18;
const BUSY_PIN: u32 = 24;
const DC_PIN: u32 = 25;

/// The panel driver on the Raspberry Pi hardware.
pub type PiEpd = Epd7in5V2<CdevPin, CdevPin, CdevPin, SpidevDevice, CdevPin, Delay>;

/// Opens the SPI device and requests the GPIO lines. The user must be in the
/// `spi` and `gpio` groups.
pub fn open() -> Result<PiEpd, Box<dyn Error>> {
    let mut spi = SpidevDevice::open(SPI_DEVICE)
        .map_err(|error| format!("cannot open {SPI_DEVICE}: {error}"))?;
    spi.configure(
        &SpidevOptions::new()
            .bits_per_word(8)
            .max_speed_hz(SPI_SPEED_HZ)
            .mode(SpiModeFlags::SPI_MODE_0)
            .build(),
    )?;
    let mut chip =
        Chip::new(GPIO_CHIP).map_err(|error| format!("cannot open {GPIO_CHIP}: {error}"))?;
    let mut line = |offset: u32, flags: LineRequestFlags| -> Result<CdevPin, Box<dyn Error>> {
        let handle = chip.get_line(offset)?.request(flags, 0, "e-paper")?;
        Ok(CdevPin::new(handle)?)
    };
    Ok(Epd7in5V2::new(
        line(PWR_PIN, LineRequestFlags::OUTPUT)?,
        line(RST_PIN, LineRequestFlags::OUTPUT)?,
        line(DC_PIN, LineRequestFlags::OUTPUT)?,
        spi,
        line(BUSY_PIN, LineRequestFlags::INPUT)?,
        Delay,
    ))
}
```

### 2.3 Red

Create the file `src/epd/mod.rs` with this content:

```rust
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
    todo!()
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
        todo!()
    }

    /// Full refresh with the frame (several seconds, the panel flashes).
    pub fn display(&mut self, frame: &Frame) -> Result<(), EpdError> {
        todo!()
    }

    /// Full refresh with a white frame.
    pub fn clear(&mut self) -> Result<(), EpdError> {
        todo!()
    }

    /// Deep sleep, then cuts the power of the HAT. The image stays.
    pub fn sleep(&mut self) -> Result<(), EpdError> {
        todo!()
    }

    fn power_on_and_reset(&mut self) -> Result<(), EpdError> {
        todo!()
    }

    fn command(&mut self, command: u8) -> Result<(), EpdError> {
        todo!()
    }

    fn data(&mut self, bytes: &[u8]) -> Result<(), EpdError> {
        todo!()
    }

    /// Waits until BUSY goes high, asking the panel for its status (0x71).
    fn wait_busy(&mut self) -> Result<(), EpdError> {
        todo!()
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
```

Add `pub mod epd;` to `src/lib.rs`.

```bash
cargo test epd
```

Expected: `12 failed` (and "unused variable" warnings).

### 2.4 Green

In `src/epd/mod.rs`, replace each `todo!()`.

`panel_buffer` (same packing as the PNG export, but 1 is black):

```rust
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
```

`init` (the sequence of `epd7in5_V2.py`, comments from its source):

```rust
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
```

`display` (the panel wants the previous image under 0x10, the new one under
0x13; like the Python driver, the "previous" one is the inverse of the new):

```rust
        let new = panel_buffer(frame);
        let old: Vec<u8> = new.iter().map(|byte| !byte).collect();
        self.command(0x10)?; // old data
        self.data(&old)?;
        self.command(0x13)?; // new data
        self.data(&new)?;
        self.command(0x12)?; // display refresh
        self.delay.delay_ms(100);
        self.wait_busy()
```

`clear`:

```rust
        self.display(&Frame::new())
```

`sleep`:

```rust
        self.command(0x02)?; // power off
        self.wait_busy()?;
        self.command(0x07)?; // deep sleep
        self.data(&[0xa5])?;
        self.delay.delay_ms(2_000);
        self.rst.set_low().map_err(hardware)?;
        self.dc.set_low().map_err(hardware)?;
        self.pwr.set_low().map_err(hardware)
```

`power_on_and_reset`:

```rust
        self.pwr.set_high().map_err(hardware)?;
        self.rst.set_high().map_err(hardware)?;
        self.delay.delay_ms(20);
        self.rst.set_low().map_err(hardware)?;
        self.delay.delay_ms(2);
        self.rst.set_high().map_err(hardware)?;
        self.delay.delay_ms(20);
        Ok(())
```

`command`:

```rust
        self.dc.set_low().map_err(hardware)?;
        self.spi.write(&[command]).map_err(hardware)
```

`data`:

```rust
        self.dc.set_high().map_err(hardware)?;
        for chunk in bytes.chunks(SPI_CHUNK) {
            self.spi.write(chunk).map_err(hardware)?;
        }
        Ok(())
```

`wait_busy` (unlike the Python driver, it gives up after 30 s):

```rust
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
```

```bash
cargo test epd
```

Expected: `12 passed`.

## 3. The `display` module: the outputs (tests 13–14)

**Red.** Create the file `src/display.rs` with this content:

```rust
//! Where a frame goes: the e-paper panel, or a PNG file on the dev PC.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal::spi::SpiDevice;

use crate::epd::Epd7in5V2;
use crate::frame::Frame;

/// Something that shows a frame.
pub trait Display {
    fn show(&mut self, frame: &Frame) -> Result<(), Box<dyn Error>>;
}

/// Writes each frame into a PNG file.
pub struct PngDisplay {
    pub path: PathBuf,
}

impl Display for PngDisplay {
    fn show(&mut self, frame: &Frame) -> Result<(), Box<dyn Error>> {
        todo!()
    }
}

/// Each frame: power and init, full refresh, deep sleep and power off.
impl<PWR, RST, DC, SPI, BUSY, DELAY> Display for Epd7in5V2<PWR, RST, DC, SPI, BUSY, DELAY>
where
    PWR: OutputPin,
    RST: OutputPin,
    DC: OutputPin,
    SPI: SpiDevice,
    BUSY: InputPin,
    DELAY: DelayNs,
{
    fn show(&mut self, frame: &Frame) -> Result<(), Box<dyn Error>> {
        todo!()
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

        PngDisplay { path: path.clone() }.show(&frame).unwrap();

        assert_eq!(Frame::from_png(&fs::read(&path).unwrap()).unwrap(), frame);
    }

    #[test]
    fn epd_display_inits_refreshes_then_sleeps() {
        let (mut epd, log) = fake_epd([], true);
        epd.show(&Frame::new()).unwrap();
        let ops = ops(&log.events());
        let position = |op: Op| ops.iter().position(|candidate| *candidate == op).unwrap();
        let power_on = position(Op::Pin("PWR", true));
        let init = position(Op::Command(0x06));
        let refresh = position(Op::Command(0x12));
        let deep_sleep = position(Op::Command(0x07));
        assert!(power_on < init && init < refresh && refresh < deep_sleep);
        assert_eq!(ops.last(), Some(&Op::Pin("PWR", false)));
    }
}
```

Add `pub mod display;` to `src/lib.rs`.

```bash
cargo test display
```

Expected: 2 failed.

**Green.** The `show` of `PngDisplay`:

```rust
        fs::write(&self.path, frame.to_png())
            .map_err(|error| format!("cannot write {}: {error}", self.path.display()))?;
        Ok(())
```

The `show` of the panel:

```rust
        self.init()?;
        self.display(frame)?;
        self.sleep()?;
        Ok(())
```

```bash
cargo test display
```

Expected: all `display` tests pass.

## 4. The command line

`run` gets `--epd`; new `test-pattern` (same `--epd` / `--png` options) and
`clear` (panel only). The `Output` struct holds the two options shared by
`run` and `test-pattern` (`#[command(flatten)]`).

Replace the whole content of `src/main.rs` with:

```rust
//! `e-paper`: household e-paper dashboard.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use e_paper::cycle::{CycleInput, cycle};
use e_paper::devices;
use e_paper::display::{Display, PngDisplay};
use e_paper::epd;
use e_paper::frame::Frame;
use e_paper::freebox_client::Client;
use e_paper::layout::{PEOPLE, layout};
use e_paper::mockups::phase1_states;
use e_paper::render::{atkinson_bold, render};
use e_paper::saved;
use e_paper::state::{ACTIVITY_THRESHOLD_BPS, PersonState};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// One-time authorization on the Freebox: confirm on its screen.
    Authorize {
        /// Where to store the app token (a secret).
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
    },
    /// Writes the devices known to the Freebox into a draft mapping file.
    Devices {
        /// The app token written by `authorize`.
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
        /// The draft mapping file to create (never overwritten).
        #[arg(long, default_value = "devices.toml")]
        output: PathBuf,
    },
    /// Checks the mapping file and prints a summary per person.
    CheckDevices {
        /// The mapping file.
        #[arg(long, default_value = "devices.toml")]
        devices: PathBuf,
    },
    /// Runs one cycle: polls the Freebox, computes the state, redraws if needed.
    Run {
        /// The app token written by `authorize`.
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
        /// The mapping file.
        #[arg(long, default_value = "devices.toml")]
        devices: PathBuf,
        /// The state kept between runs.
        #[arg(long, default_value = "state.json")]
        state: PathBuf,
        #[command(flatten)]
        output: Output,
        /// Presence window in seconds: the cycle period.
        #[arg(long, default_value_t = 900)]
        interval: i64,
        /// Redraw even if nothing changed.
        #[arg(long)]
        force: bool,
    },
    /// Shows the phase 1 mockup with an activity ring (legibility test).
    TestPattern {
        #[command(flatten)]
        output: Output,
    },
    /// Clears the e-paper panel to white.
    Clear,
}

/// Where to show the display.
#[derive(clap::Args)]
struct Output {
    /// Draw on the e-paper panel (on the Raspberry Pi) instead of a PNG file.
    #[arg(long)]
    epd: bool,
    /// The PNG file, when not drawing on the panel.
    #[arg(long, default_value = "target/e-paper.png")]
    png: PathBuf,
}

impl Output {
    fn open(&self) -> Result<Box<dyn Display>, Box<dyn Error>> {
        if self.epd {
            Ok(Box::new(epd::linux::open()?))
        } else {
            Ok(Box::new(PngDisplay {
                path: self.png.clone(),
            }))
        }
    }

    fn describe(&self) -> String {
        if self.epd {
            "e-paper panel".into()
        } else {
            self.png.display().to_string()
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Command::Authorize { token } => authorize(&token),
        Command::Devices { token, output } => list_devices(&token, &output),
        Command::CheckDevices { devices } => check_devices(&devices),
        Command::Run {
            token,
            devices,
            state,
            output,
            interval,
            force,
        } => run_cycle(&token, &devices, &state, &output, interval, force),
        Command::TestPattern { output } => test_pattern(&output),
        Command::Clear => {
            epd::linux::open()?.clear()?;
            println!("Panel cleared");
            Ok(())
        }
    }
}

fn authorize(token_path: &Path) -> Result<(), Box<dyn Error>> {
    if token_path.exists() {
        return Err(format!(
            "{} already exists: delete it to authorize again",
            token_path.display()
        )
        .into());
    }
    let client = Client::connect()?;
    let app_token = client.authorize(&device_name())?;
    save_secret(token_path, &app_token)?;
    println!("Authorized. App token saved in {}", token_path.display());
    Ok(())
}

fn list_devices(token_path: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let app_token = read_token(token_path)?;
    let mut client = Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
    devices::write_new(output, &devices::draft(&hosts)).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            format!("{} already exists: not overwritten", output.display())
        } else {
            format!("cannot write {}: {error}", output.display())
        }
    })?;
    println!("{} devices written to {}", hosts.len(), output.display());
    Ok(())
}

fn check_devices(path: &Path) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(path)?;
    for line in devices::summary(&mapping) {
        println!("{line}");
    }
    Ok(())
}

fn run_cycle(
    token_path: &Path,
    devices_path: &Path,
    state_path: &Path,
    output_target: &Output,
    interval_secs: i64,
    force: bool,
) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(devices_path)?;
    let app_token = read_token(token_path)?;
    let (previous, warning) = saved::load(state_path);
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }

    let mut client = Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
    let counters = client.wifi_counters()?;
    let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;

    let output = cycle(&CycleInput {
        now,
        interval_secs,
        threshold_bps: ACTIVITY_THRESHOLD_BPS,
        mapping: &mapping,
        hosts: &hosts,
        counters: &counters,
        previous: previous.as_ref(),
        force,
    });
    let states: Vec<String> = PEOPLE
        .iter()
        .map(|person| format!("{} {}", person.0, describe(output.household[person])))
        .collect();
    println!("{}", states.join(", "));

    if output.refresh {
        let mut frame = Frame::new();
        render(&mut frame, &layout(&output.household), &atkinson_bold())?;
        output_target.open()?.show(&frame)?;
        println!("Display redrawn: {}", output_target.describe());
    } else {
        println!("No change: display not touched");
    }
    saved::save(state_path, &output.saved)
        .map_err(|error| format!("cannot write {}: {error}", state_path.display()))?;
    Ok(())
}

fn test_pattern(output: &Output) -> Result<(), Box<dyn Error>> {
    let (_, state) = phase1_states()
        .into_iter()
        .find(|(name, _)| *name == "phase1-activity-ring")
        .expect("mockup exists");
    let mut frame = Frame::new();
    render(&mut frame, &layout(&state), &atkinson_bold())?;
    output.open()?.show(&frame)?;
    println!("Test pattern shown: {}", output.describe());
    Ok(())
}

fn describe(state: PersonState) -> &'static str {
    match state {
        PersonState::Away => "away",
        PersonState::HomePassive => "home",
        PersonState::HomeActive => "home (active)",
    }
}

fn read_mapping(path: &Path) -> Result<devices::Mapping, Box<dyn Error>> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run `e-paper devices` first)",
            path.display()
        )
    })?;
    Ok(devices::parse_mapping(&text)?)
}

fn read_token(path: &Path) -> Result<String, Box<dyn Error>> {
    let token = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run `e-paper authorize` first)",
            path.display()
        )
    })?;
    Ok(token.trim().to_string())
}

/// Name shown on the Freebox screen during the authorization.
fn device_name() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_string())
        .unwrap_or_else(|_| "e-paper".into())
}

/// Writes a secret readable by its owner only; never overwrites.
fn save_secret(path: &Path, secret: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(secret.as_bytes())
}
```

## 5. Checks on the dev PC

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
cargo run -- test-pattern
```

Expected: 89 passed plus the golden image test, no clippy warning,
`Test pattern shown: target/e-paper.png`. `cargo run -- clear` fails on the PC
(`cannot open /dev/spidev0.0`): normal, there is no panel.

## 6. On the Raspberry Pi

Check that the user can reach the SPI device and the GPIO chip without `sudo`:

```bash
ssh epaper 'groups; ls -l /dev/spidev0.0 /dev/gpiochip0'
```

Expected: `spi` and `gpio` in the groups, both devices present. If a group is
missing: `ssh epaper 'sudo usermod -aG spi,gpio $USER'`, then log in again.

Build for the Pi and copy the binary:

```bash
cargo zigbuild --release --target aarch64-unknown-linux-musl
scp target/aarch64-unknown-linux-musl/release/e-paper epaper:~/e-paper
```

Show the test pattern on the panel (it flashes for several seconds):

```bash
ssh epaper '~/e-paper test-pattern --epd'
```

Expected: the four circles, R and H filled, the double ring around H. This is
also the moment for the legibility test at 3–4 m with the people concerned.

Then clear it:

```bash
ssh epaper '~/e-paper clear'
```

If a command hangs, it stops after 30 s with `e-paper still busy`: check the
ribbon cables.

### 6.1 A real cycle on the Pi (optional)

The Pi needs its own app token (one authorization per device) and the mapping:

```bash
ssh -t epaper '~/e-paper authorize'
scp devices.toml epaper:~/devices.toml
ssh epaper '~/e-paper check-devices'
ssh epaper '~/e-paper run --epd'
```

Accept the authorization on the Freebox screen, as on the dev PC. The files
(`secrets/`, `devices.toml`, `state.json`) live in the home folder of the Pi
for now; their final places come with the systemd units.

## 7. Commit

```bash
git add -A
git status
git commit -m "feat: e-paper driver for the Waveshare 7.5 V2 and Display trait"
```

## Report

Share the output of section 5, of `groups` on the Pi, and what the panel shows
with `test-pattern --epd` (a photo is welcome).
