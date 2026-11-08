//! Stateless bus functions: the lowest rung of the ladder.
//!
//! Each function does one self-contained job over an
//! [`embedded_hal::i2c::I2c`] and keeps no state, so it can be used without
//! [`Oled`](crate::Oled).

use embedded_hal::i2c::I2c;

use crate::command::{set_page_column, CONTROL_COMMAND, CONTROL_DATA, DISPLAY_ON, NOP};
use crate::{Controller, Error, MAX_WRITE_LEN, PAGES, WIDTH};

/// Data bytes per write: `MAX_WRITE_LEN` minus the control byte.
const CHUNK: usize = MAX_WRITE_LEN - 1;
/// Column used by [`detect`]: past the 128 visible columns of an SH1106 (132 RAM columns).
const PROBE_COLUMN: u8 = 130;
/// The two values written and read back by [`detect`].
const PROBE_VALUES: [u8; 2] = [0xA5, 0x5A];

/// Sends a NOP command, a cheap liveness check that draws nothing.
///
/// # Errors
///
/// Returns [`Error::Bus`] when the device does not acknowledge or the bus fails.
pub fn probe<I: I2c>(i2c: &mut I, address: u8) -> Result<(), Error<I::Error>> {
    i2c.write(address, &[CONTROL_COMMAND, NOP])?;
    Ok(())
}

/// Tells an SSD1306 from an SH1106 by a display-RAM read-back.
///
/// For each of two different values the function selects page 0, column 130,
/// writes the value, selects the column again and reads two bytes after a data
/// control byte.
/// The SH1106 returns a dummy byte followed by the value; the SSD1306 cannot
/// read its RAM and returns its status byte.
/// Both values coming back means [`Controller::Sh1106`]; anything else means
/// [`Controller::Ssd1306`].
/// A read that the device rejects after its writes were acknowledged also
/// means [`Controller::Ssd1306`]: the datasheet documents its serial mode as
/// write-only, while the SH1106 always supports RAM read-back.
/// Column 130 is outside the SH1106's visible area, and the SSD1306 ignores
/// columns past 127, so the screen is unaffected.
/// Run it before [`init`]; a later full flush overwrites any stray RAM.
///
/// # Errors
///
/// Returns [`Error::Bus`] when an address-select or test-value write fails
/// (including no ACK at the address); the panel is then not classified.
/// A failing read phase is not an error, see above.
pub fn detect<I: I2c>(i2c: &mut I, address: u8) -> Result<Controller, Error<I::Error>> {
    let select = set_page_column(0, PROBE_COLUMN);
    let mut matches = 0;
    for value in PROBE_VALUES {
        i2c.write(address, &select)?;
        i2c.write(address, &[CONTROL_DATA, value])?;
        i2c.write(address, &select)?;
        let mut read = [0u8; 2];
        if i2c.write_read(address, &[CONTROL_DATA], &mut read).is_err() {
            // The writes were acknowledged, so the device is there and
            // refuses reads: only an SSD1306 does that.
            return Ok(Controller::Ssd1306);
        }
        if read[1] == value {
            matches += 1;
        }
    }
    Ok(if matches == PROBE_VALUES.len() {
        Controller::Sh1106
    } else {
        Controller::Ssd1306
    })
}

/// Sends the init sequence of `controller`, leaving the display off.
///
/// Call [`display_on`] after the first frame has been written.
///
/// # Errors
///
/// Returns [`Error::Bus`] on the first failing write.
pub fn init<I: I2c>(
    i2c: &mut I,
    address: u8,
    controller: Controller,
) -> Result<(), Error<I::Error>> {
    for command in controller.init_commands() {
        let mut frame = [0u8; 4];
        frame[0] = CONTROL_COMMAND;
        let len = command.len().min(frame.len() - 1);
        frame[1..=len].copy_from_slice(&command[..len]);
        i2c.write(address, &frame[..=len])?;
    }
    Ok(())
}

/// Turns the display on.
///
/// # Errors
///
/// Returns [`Error::Bus`] when the write fails.
pub fn display_on<I: I2c>(i2c: &mut I, address: u8) -> Result<(), Error<I::Error>> {
    i2c.write(address, &[CONTROL_COMMAND, DISPLAY_ON])?;
    Ok(())
}

/// Writes one 128-byte page.
///
/// Sends the page and column selection (column offset 0 or 2 per
/// `controller`), then the data in writes of at most [`MAX_WRITE_LEN`] bytes.
/// `page` is reduced modulo [`PAGES`].
///
/// # Errors
///
/// Returns [`Error::Bus`] on the first failing write; the page is then only
/// partly written and should be sent again in full.
pub fn write_page<I: I2c>(
    i2c: &mut I,
    address: u8,
    controller: Controller,
    page: u8,
    data: &[u8; WIDTH as usize],
) -> Result<(), Error<I::Error>> {
    i2c.write(
        address,
        &set_page_column(page % PAGES, controller.column_offset()),
    )?;
    for chunk in data.chunks(CHUNK) {
        let mut frame = [0u8; MAX_WRITE_LEN];
        frame[0] = CONTROL_DATA;
        frame[1..=chunk.len()].copy_from_slice(chunk);
        i2c.write(address, &frame[..=chunk.len()])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{set_page_column, CONTROL_DATA};
    use crate::testutil::{detect_script, init_script, page_script, probe_txn, write, ADDR};
    use tamer::mock::{MockI2c, MockI2cError, Transaction};

    #[test]
    fn detect_read_rejected_is_ssd1306() {
        let select = set_page_column(0, 130);
        let script = [
            write(&select),
            write(&[CONTROL_DATA, 0xA5]),
            write(&select),
            Transaction::Error(MockI2cError::nack()),
        ];
        let mut bus = MockI2c::new(&script);
        assert_eq!(detect(&mut bus, ADDR), Ok(Controller::Ssd1306));
        bus.done();
    }

    #[test]
    fn detect_write_nack_is_error() {
        let select = set_page_column(0, 130);
        let script = [write(&select), Transaction::Error(MockI2cError::nack())];
        let mut bus = MockI2c::new(&script);
        assert!(detect(&mut bus, ADDR).unwrap_err().is_nack());
        bus.done();
    }

    #[test]
    fn detect_sh1106_both_values_match() {
        let script = detect_script(Controller::Sh1106);
        let mut bus = MockI2c::new(&script);
        assert_eq!(detect(&mut bus, ADDR), Ok(Controller::Sh1106));
        bus.done();
    }

    #[test]
    fn detect_ssd1306_status_0x45() {
        let script = detect_script(Controller::Ssd1306);
        let mut bus = MockI2c::new(&script);
        assert_eq!(detect(&mut bus, ADDR), Ok(Controller::Ssd1306));
        bus.done();
    }

    #[test]
    fn detect_one_match_is_ssd1306() {
        let mut script = detect_script(Controller::Sh1106);
        // Second value reads back wrong.
        let last = script.len() - 1;
        script[last] = Transaction::WriteRead {
            address: ADDR,
            write: &[CONTROL_DATA],
            response: &[0x00, 0x00],
        };
        let mut bus = MockI2c::new(&script);
        assert_eq!(detect(&mut bus, ADDR), Ok(Controller::Ssd1306));
        bus.done();
    }

    #[test]
    fn detect_nack_is_error() {
        let script = [Transaction::Error(MockI2cError::nack())];
        let mut bus = MockI2c::new(&script);
        let err = detect(&mut bus, ADDR).unwrap_err();
        assert!(err.is_nack());
        bus.done();
    }

    #[test]
    fn probe_sends_nop() {
        let script = [probe_txn()];
        let mut bus = MockI2c::new(&script);
        assert_eq!(probe(&mut bus, ADDR), Ok(()));
        bus.done();
    }

    #[test]
    fn init_bytes_ssd1306() {
        let script = init_script(Controller::Ssd1306);
        let mut bus = MockI2c::new(&script);
        assert_eq!(init(&mut bus, ADDR, Controller::Ssd1306), Ok(()));
        bus.done();
        assert!(script.contains(&write(&[0x00, 0x20, 0x02])));
        assert!(script.contains(&write(&[0x00, 0x8D, 0x14])));
    }

    #[test]
    fn init_bytes_sh1106() {
        let script = init_script(Controller::Sh1106);
        let mut bus = MockI2c::new(&script);
        assert_eq!(init(&mut bus, ADDR, Controller::Sh1106), Ok(()));
        bus.done();
        assert!(script.contains(&write(&[0x00, 0xAD, 0x8B])));
    }

    #[test]
    fn init_has_no_display_on() {
        for c in [Controller::Ssd1306, Controller::Sh1106] {
            let script = init_script(c);
            assert_eq!(script.first(), Some(&write(&[0x00, 0xAE])));
            assert!(!script.contains(&write(&[0x00, 0xAF])));
        }
    }

    #[test]
    fn display_on_sends_af() {
        let script = [write(&[0x00, 0xAF])];
        let mut bus = MockI2c::new(&script);
        assert_eq!(display_on(&mut bus, ADDR), Ok(()));
        bus.done();
    }

    #[test]
    fn write_page_offset_0_and_2() {
        let data = [0u8; 128];
        for (c, col) in [(Controller::Ssd1306, 0x00), (Controller::Sh1106, 0x02)] {
            let script = page_script(c, 3, &data);
            assert_eq!(script[0], write(&[0x00, 0xB3, col, 0x10]));
            let mut bus = MockI2c::new(&script);
            assert_eq!(write_page(&mut bus, ADDR, c, 3, &data), Ok(()));
            bus.done();
        }
    }

    #[test]
    fn write_page_chunks_31() {
        let mut data = [0u8; 128];
        for (i, b) in data.iter_mut().enumerate() {
            *b = i as u8;
        }
        let script = page_script(Controller::Ssd1306, 0, &data);
        // Header + 30,30,30,30,8 data bytes.
        assert_eq!(script.len(), 6);
        let mut bus = MockI2c::new(&script);
        assert_eq!(
            write_page(&mut bus, ADDR, Controller::Ssd1306, 0, &data),
            Ok(())
        );
        bus.done();
    }
}
