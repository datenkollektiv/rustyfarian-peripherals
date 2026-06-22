//! Test-support mock for digital input pins.
//!
//! [`MockInputPin`] implements `embedded_hal::digital::InputPin` with an
//! `Infallible` error type.
//! Drive its level with [`set_high`](MockInputPin::set_high) /
//! [`set_low`](MockInputPin::set_low) and read it through the HAL trait.
//!
//! This mock is intended for:
//!
//! - Testing [`DebouncedInput`](crate::debounce::DebouncedInput) and
//!   [`QuadratureInput`](crate::rotary::QuadratureInput) without real hardware.
//! - Downstream crates that need a settable `InputPin` in unit tests.
//!
//! # Example
//!
//! ```
//! use tamer::mock::MockInputPin;
//! use embedded_hal::digital::InputPin;
//!
//! let mut pin = MockInputPin::new(false);
//! assert!(!pin.is_high().unwrap());
//!
//! pin.set_high();
//! assert!(pin.is_high().unwrap());
//!
//! pin.set_low();
//! assert!(pin.is_low().unwrap());
//! ```

use core::convert::Infallible;
use embedded_hal::digital::{ErrorType, InputPin};

/// Settable mock that implements [`embedded_hal::digital::InputPin`].
///
/// The `Error` associated type is [`Infallible`], so `.unwrap()` is safe.
///
/// Create with [`MockInputPin::new`] and drive the level with
/// [`set_high`](MockInputPin::set_high) / [`set_low`](MockInputPin::set_low).
///
/// [`Default`] yields a low pin (`high = false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MockInputPin {
    high: bool,
}

impl MockInputPin {
    /// Creates a new mock pin with the given initial level.
    ///
    /// Pass `true` for logic-high, `false` for logic-low.
    #[must_use]
    pub fn new(high: bool) -> Self {
        Self { high }
    }

    /// Drives the pin high (logic `1`).
    pub fn set_high(&mut self) {
        self.high = true;
    }

    /// Drives the pin low (logic `0`).
    pub fn set_low(&mut self) {
        self.high = false;
    }

    /// Sets the pin level: `true` for high, `false` for low.
    pub fn set(&mut self, high: bool) {
        self.high = high;
    }
}

impl ErrorType for MockInputPin {
    type Error = Infallible;
}

impl InputPin for MockInputPin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.high)
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.high)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::digital::InputPin;

    #[test]
    fn new_high_reports_high() {
        let mut pin = MockInputPin::new(true);
        assert!(pin.is_high().unwrap());
        assert!(!pin.is_low().unwrap());
    }

    #[test]
    fn new_low_reports_low() {
        let mut pin = MockInputPin::new(false);
        assert!(!pin.is_high().unwrap());
        assert!(pin.is_low().unwrap());
    }

    #[test]
    fn set_high_then_low() {
        let mut pin = MockInputPin::new(false);
        pin.set_high();
        assert!(pin.is_high().unwrap());
        pin.set_low();
        assert!(pin.is_low().unwrap());
    }

    #[test]
    fn set_bool() {
        let mut pin = MockInputPin::new(false);
        pin.set(true);
        assert!(pin.is_high().unwrap());
        pin.set(false);
        assert!(!pin.is_high().unwrap());
    }
}
