//! Test-support mocks for digital input pins, I2C buses, and delays.
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
//!
//! [`MockI2c`] replays a scripted slice of [`Transaction`](crate::mock::Transaction)s and [`NoopDelay`]
//! is a `DelayNs` that returns immediately; together they let the
//! [`Mpu6050`](crate::mpu6050::Mpu6050) driver run on the host.
//!
//! ```
//! use embedded_hal::i2c::I2c;
//! use tamer::mock::{MockI2c, Transaction};
//!
//! let script = [Transaction::WriteRead {
//!     address: 0x68,
//!     write: &[0x75],
//!     response: &[0x68],
//! }];
//! let mut bus = MockI2c::new(&script);
//! let mut id = [0u8; 1];
//! bus.write_read(0x68, &[0x75], &mut id).unwrap();
//! assert_eq!(id, [0x68]);
//! bus.done();
//! ```

use core::convert::Infallible;
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{ErrorType, InputPin};
use embedded_hal::i2c::{
    self, ErrorKind, ErrorType as I2cErrorType, I2c, NoAcknowledgeSource, Operation,
    SevenBitAddress,
};

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

/// Error returned by [`MockI2c`], with a configurable
/// [`ErrorKind`].
///
/// Use [`nack`](Self::nack) to simulate an absent or miswired device and
/// [`bus`](Self::bus) to simulate a bus fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MockI2cError {
    kind: ErrorKind,
}

impl MockI2cError {
    /// Creates an error that reports the given `kind`.
    #[must_use]
    pub const fn new(kind: ErrorKind) -> Self {
        Self { kind }
    }

    /// Creates a no-acknowledge error (source unknown), as a missing device
    /// produces.
    #[must_use]
    pub const fn nack() -> Self {
        Self::new(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown))
    }

    /// Creates a generic bus-fault error.
    #[must_use]
    pub const fn bus() -> Self {
        Self::new(ErrorKind::Bus)
    }
}

impl i2c::Error for MockI2cError {
    fn kind(&self) -> ErrorKind {
        self.kind
    }
}

/// One scripted I2C transaction expected by [`MockI2c`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transaction<'a> {
    /// Expects a write of exactly `bytes` to `address`.
    Write {
        /// Expected 7-bit device address.
        address: u8,
        /// Expected payload.
        bytes: &'a [u8],
    },
    /// Expects a read from `address`, filled from `response`.
    Read {
        /// Expected 7-bit device address.
        address: u8,
        /// Bytes handed back to the caller; its length must match the read buffer.
        response: &'a [u8],
    },
    /// Expects a write of `write` then a repeated-start read, filled from `response`.
    WriteRead {
        /// Expected 7-bit device address.
        address: u8,
        /// Expected payload of the write phase.
        write: &'a [u8],
        /// Bytes handed back to the caller; its length must match the read buffer.
        response: &'a [u8],
    },
    /// Fails the next transaction, whatever it is, with the given error.
    Error(MockI2cError),
}

/// Scripted mock that implements [`embedded_hal::i2c::I2c`].
///
/// The script is a borrowed slice of [`Transaction`]s consumed in order, so the
/// mock allocates nothing. Call [`done`](Self::done) at the end of a test to
/// assert the whole script was consumed.
///
/// # Panics
///
/// As a test double, the mock panics when the driver under test issues a
/// transaction that differs from the script (wrong kind, address, payload, or
/// read length), when the script is exhausted, or when the operation sequence
/// is not a plain write, read, or write-then-read.
#[derive(Debug)]
pub struct MockI2c<'a> {
    script: &'a [Transaction<'a>],
    next: usize,
}

impl<'a> MockI2c<'a> {
    /// Creates a mock that expects exactly the given `script`, in order.
    #[must_use]
    pub const fn new(script: &'a [Transaction<'a>]) -> Self {
        Self { script, next: 0 }
    }

    /// Returns the number of scripted transactions not yet consumed.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.script.len() - self.next
    }

    /// Asserts that every scripted transaction was consumed.
    ///
    /// # Panics
    ///
    /// Panics if any scripted transaction is left over.
    pub fn done(&self) {
        assert_eq!(
            self.remaining(),
            0,
            "MockI2c: {} scripted transaction(s) not consumed",
            self.remaining()
        );
    }

    fn take(&mut self) -> Transaction<'a> {
        let Some(t) = self.script.get(self.next) else {
            panic!("MockI2c: unexpected transaction, script exhausted");
        };
        self.next += 1;
        *t
    }
}

fn fill(read: &mut [u8], response: &[u8]) {
    assert_eq!(
        read.len(),
        response.len(),
        "MockI2c: read length does not match scripted response"
    );
    read.copy_from_slice(response);
}

impl I2cErrorType for MockI2c<'_> {
    type Error = MockI2cError;
}

impl I2c<SevenBitAddress> for MockI2c<'_> {
    fn transaction(
        &mut self,
        address: SevenBitAddress,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        let expected = self.take();
        if let Transaction::Error(e) = expected {
            return Err(e);
        }
        match (expected, operations) {
            (Transaction::Write { address: a, bytes }, [Operation::Write(w)]) => {
                assert_eq!((a, bytes), (address, *w), "MockI2c: write mismatch");
            }
            (
                Transaction::Read {
                    address: a,
                    response,
                },
                [Operation::Read(r)],
            ) => {
                assert_eq!(a, address, "MockI2c: read address mismatch");
                fill(r, response);
            }
            (
                Transaction::WriteRead {
                    address: a,
                    write,
                    response,
                },
                [Operation::Write(w), Operation::Read(r)],
            ) => {
                assert_eq!((a, write), (address, *w), "MockI2c: write_read mismatch");
                fill(r, response);
            }
            (expected, _) => panic!("MockI2c: expected {expected:?}, got a different operation"),
        }
        Ok(())
    }
}

/// `DelayNs` implementation that returns immediately.
///
/// Use it where a driver needs a delay but the test must not sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoopDelay;

impl DelayNs for NoopDelay {
    fn delay_ns(&mut self, _ns: u32) {}
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

    #[test]
    fn i2c_write_then_write_read_consumes_script() {
        let script = [
            Transaction::Write {
                address: 0x68,
                bytes: &[0x6B, 0x00],
            },
            Transaction::WriteRead {
                address: 0x68,
                write: &[0x75],
                response: &[0x68],
            },
        ];
        let mut bus = MockI2c::new(&script);
        assert_eq!(bus.remaining(), 2);
        bus.write(0x68, &[0x6B, 0x00]).unwrap();
        let mut id = [0u8; 1];
        bus.write_read(0x68, &[0x75], &mut id).unwrap();
        assert_eq!(id, [0x68]);
        bus.done();
    }

    #[test]
    fn i2c_injected_error_reports_kind() {
        use embedded_hal::i2c::Error as _;
        let script = [Transaction::Error(MockI2cError::nack())];
        let mut bus = MockI2c::new(&script);
        let err = bus.write(0x68, &[0x00]).unwrap_err();
        assert!(matches!(err.kind(), ErrorKind::NoAcknowledge(_)));
        assert_eq!(MockI2cError::bus().kind(), ErrorKind::Bus);
    }

    #[test]
    #[should_panic(expected = "not consumed")]
    fn i2c_done_panics_with_leftover_script() {
        let script = [Transaction::Read {
            address: 0x68,
            response: &[1],
        }];
        MockI2c::new(&script).done();
    }

    #[test]
    #[should_panic(expected = "write mismatch")]
    fn i2c_unexpected_payload_panics() {
        let script = [Transaction::Write {
            address: 0x68,
            bytes: &[1],
        }];
        let _ = MockI2c::new(&script).write(0x68, &[2]);
    }

    #[test]
    fn noop_delay_returns() {
        let mut d = NoopDelay;
        d.delay_ms(1000);
    }
}
