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
