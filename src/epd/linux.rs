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
