#![cfg_attr(not(test), no_std)]
//! A `no_std`, alloc-free driver for 128x64 I2C OLEDs: SSD1306 or SH1106,
//! detected at runtime and hot-swappable while the firmware runs.
//!
//! Both controllers answer at [`ADDRESS_PRIMARY`] and have no ID register, so
//! [`detect`] writes two different values to an off-screen column and reads
//! them back: only the SH1106 can read its display RAM.
//! A read the panel rejects after its writes were acknowledged also means
//! SSD1306, whose serial mode is write-only; [`Oled::with_controller`] pins the
//! controller and skips detection altogether.
//! The application sees one [`DrawTarget`](embedded_graphics_core::draw_target::DrawTarget)
//! with [`BinaryColor`](embedded_graphics_core::pixelcolor::BinaryColor).
//!
//! # Layers
//!
//! 1. Stateless bus functions: [`probe`], [`detect`], [`init`], [`display_on`],
//!    [`write_page`].
//! 2. [`Framebuffer`]: an I/O-free page-major buffer with a dirty mask.
//! 3. [`Oled`]: the blocking runner that owns the bus, the framebuffer and a
//!    [`tamer::hotplug::Hotplug`] machine.
//!
//! `signwriter` never sleeps: the caller passes millisecond timestamps to
//! [`Oled::tick`].
//!
//! # Example
//!
//! Drawing works whether or not a panel is attached; nothing touches the bus
//! until [`Oled::tick`] sees the device.
//!
//! ```
//! use embedded_graphics_core::{pixelcolor::BinaryColor, prelude::*};
//! use signwriter::{Oled, ADDRESS_PRIMARY};
//! use tamer::mock::MockI2c;
//!
//! let mut oled = Oled::new(MockI2c::new(&[]), ADDRESS_PRIMARY, 200, 3, 250);
//! Pixel(Point::new(3, 9), BinaryColor::On).draw(&mut oled).unwrap();
//! assert_eq!(oled.framebuffer().dirty_pages(), 0b0000_0010);
//! assert!(!oled.is_attached());
//! oled.release().done();
//! ```

use core::fmt;

use embedded_hal::i2c::ErrorKind;

mod bus;
mod command;
mod framebuffer;
mod oled;
#[cfg(test)]
mod testutil;

pub use bus::{detect, display_on, init, probe, write_page};
pub use framebuffer::Framebuffer;
pub use oled::Oled;

/// The usual 7-bit I2C address of these panels (`SA0` low).
pub const ADDRESS_PRIMARY: u8 = 0x3C;
/// The alternate 7-bit I2C address (`SA0` high).
pub const ADDRESS_SECONDARY: u8 = 0x3D;
/// Panel width in pixels.
pub const WIDTH: u32 = 128;
/// Panel height in pixels.
pub const HEIGHT: u32 = 64;
/// Number of 8-pixel-high pages.
pub const PAGES: u8 = 8;
/// Longest single I2C write `signwriter` issues: a control byte plus 30 data bytes.
pub const MAX_WRITE_LEN: usize = 31;

/// The display controller found on the bus.
///
/// The two differ only in the init table and the column offset: the SH1106
/// has 132 RAM columns with the visible 128 starting at column 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Controller {
    /// Solomon Systech SSD1306 (128 RAM columns, no offset).
    Ssd1306,
    /// Sino Wealth / Sitronix SH1106 (132 RAM columns, offset 2).
    Sh1106,
}

impl Controller {
    /// First RAM column of the visible area: `0` for the SSD1306, `2` for the SH1106.
    #[must_use]
    pub const fn column_offset(self) -> u8 {
        match self {
            Self::Ssd1306 => 0,
            Self::Sh1106 => 2,
        }
    }

    /// The init sequence: one entry per command, each with its arguments.
    ///
    /// The runner sends every entry as one write behind a command control byte.
    /// The list never contains display-on (`0xAF`); see [`display_on`].
    #[must_use]
    pub const fn init_commands(self) -> &'static [&'static [u8]] {
        match self {
            Self::Ssd1306 => command::SSD1306_INIT,
            Self::Sh1106 => command::SH1106_INIT,
        }
    }
}

/// Errors from the bus functions and [`Oled::flush`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    /// The I2C bus reported an error.
    Bus(E),
    /// No panel is attached, so nothing was sent.
    Detached,
}

impl<E: embedded_hal::i2c::Error> Error<E> {
    /// Returns `true` when the bus error is an address or data NACK, which is
    /// what a missing or unplugged panel produces.
    #[must_use]
    pub fn is_nack(&self) -> bool {
        match self {
            Self::Bus(e) => matches!(e.kind(), ErrorKind::NoAcknowledge(_)),
            Self::Detached => false,
        }
    }
}

impl<E> From<E> for Error<E> {
    fn from(error: E) -> Self {
        Self::Bus(error)
    }
}

impl<E: fmt::Debug> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus(e) => write!(f, "I2C bus error: {e:?}"),
            Self::Detached => f.write_str("display detached"),
        }
    }
}

/// An attachment change reported by [`Oled::tick`] and [`Oled::redetect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// A panel was found, initialised and fully drawn.
    Attached(Controller),
    /// The panel stopped answering.
    Detached,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tamer::mock::MockI2cError;

    #[test]
    fn column_offsets() {
        assert_eq!(Controller::Ssd1306.column_offset(), 0);
        assert_eq!(Controller::Sh1106.column_offset(), 2);
    }

    #[test]
    fn nack_classification() {
        assert!(Error::Bus(MockI2cError::nack()).is_nack());
        assert!(!Error::Bus(MockI2cError::bus()).is_nack());
        assert!(!Error::<MockI2cError>::Detached.is_nack());
    }

    #[test]
    fn display_formats() {
        extern crate std;
        use std::string::ToString;
        assert_eq!(
            Error::<MockI2cError>::Detached.to_string(),
            "display detached"
        );
        assert!(Error::Bus(MockI2cError::bus())
            .to_string()
            .starts_with("I2C bus error"));
    }
}
