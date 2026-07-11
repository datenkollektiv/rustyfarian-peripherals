//! MPU6050 IMU protocol constants, raw-buffer parsing, and accelerometer calibration.
//!
//! Provides platform-independent types and functions for working with the
//! InvenSense MPU6050 6-axis IMU (accelerometer + gyroscope) over I2C. The
//! I2C transport is left entirely to the caller — this module only
//! interprets the datasheet's register map and byte layout, and accumulates
//! calibration offsets. It imports no I2C, HAL, or chip crate.
//!
//! This module is device-specific by design (see the [feature
//! doc](https://github.com/datenkollektiv/rustyfarian-peripherals/blob/main/docs/features/mpu6050-imu-v1.md)):
//! the register addresses, init sequence, and 14-byte burst layout are fixed
//! MPU6050 datasheet facts, not a generic IMU abstraction.
//!
//! Two hardware-agnostic mechanisms that pair with this module live
//! elsewhere in `tamer`:
//! - [`crate::smoothing::EmaFilter`] — smooths noisy raw accelerometer or
//!   gyroscope samples before use.
//! - `tamer::tilt` (feature `tilt`) — computes a two-axis tilt angle from
//!   corrected accelerometer readings. Not linked here because the module
//!   only exists when the `tilt` feature is enabled.
//!
//! # Protocol
//!
//! The MPU6050 listens on [`I2C_ADDR`](crate::mpu6050::I2C_ADDR) (`0x68`) by
//! default, or [`I2C_ADDR_ALT`](crate::mpu6050::I2C_ADDR_ALT) (`0x69`) when
//! the AD0 pin is pulled high. Verify the sensor is present by reading
//! [`REG_WHO_AM_I`](crate::mpu6050::REG_WHO_AM_I) and comparing to
//! [`WHO_AM_I_VALUE`](crate::mpu6050::WHO_AM_I_VALUE).
//!
//! Initialise the sensor by writing each `(register, value)` pair in
//! [`INIT_SEQUENCE`](crate::mpu6050::INIT_SEQUENCE) in order. Then burst-read
//! 14 bytes starting from
//! [`REG_ACCEL_XOUT_H`](crate::mpu6050::REG_ACCEL_XOUT_H) and pass them to
//! [`parse_raw`](crate::mpu6050::parse_raw).
//!
//! # Example
//!
//! ```
//! use tamer::mpu6050::parse_raw;
//!
//! // Simulated 14-byte burst read: sensor lying flat on a table.
//! // Accel X=0, Y=0, Z=+1g (16384 LSB), temp skipped, gyro all zero.
//! let mut buf = [0u8; 14];
//! buf[4] = 0x40; // Z high byte: 0x4000 = 16384
//! buf[5] = 0x00; // Z low byte
//!
//! let reading = parse_raw(&buf);
//! assert_eq!(reading.accel_z(), 16384);
//! ```
//!
//! # Recommended pattern with calibration offsets
//!
//! Use [`AccelCalibration`](crate::mpu6050::AccelCalibration) to learn
//! per-axis offsets at rest, then apply them with
//! [`apply_offsets`](crate::mpu6050::apply_offsets) before further
//! processing (e.g. `tamer::tilt::tilt_degrees_i32`, under the `tilt`
//! feature):
//!
//! ```
//! use tamer::mpu6050::{apply_offsets, parse_raw, AccelCalibration};
//!
//! let mut cal = AccelCalibration::new();
//! cal.add_sample(50, 16400);
//! cal.add_sample(40, 16368);
//! let offsets = cal.offsets();
//! assert_eq!(offsets.y, 45); // average of 50 and 40
//!
//! let mut buf = [0u8; 14];
//! buf[4] = 0x40; // accel_z = 16384 (+1g)
//! let reading = parse_raw(&buf);
//!
//! let (ay, az) = apply_offsets(&reading, offsets.y, offsets.z);
//! assert_eq!(ay, -45); // reading.accel_y() is 0, so 0 - 45 = -45
//! assert_eq!(az, 16384);
//! ```

// ─── Protocol constants ─────────────────────────────────────────────────────

/// Primary I2C address (AD0 pin low).
pub const I2C_ADDR: u8 = 0x68;

/// Alternate I2C address (AD0 pin high).
pub const I2C_ADDR_ALT: u8 = 0x69;

/// Power management register 1. Write `0x00` to wake from sleep.
pub const REG_PWR_MGMT_1: u8 = 0x6B;

/// Accelerometer configuration register. Write `0x00` for +/-2g full-scale range.
pub const REG_ACCEL_CONFIG: u8 = 0x1C;

/// Gyroscope configuration register. Write `0x00` for +/-250 deg/s full-scale range.
pub const REG_GYRO_CONFIG: u8 = 0x1B;

/// Sample rate divider register. The sample rate is `8 kHz / (1 + SMPLRT_DIV)`.
pub const REG_SMPLRT_DIV: u8 = 0x19;

/// Configuration register. Controls DLPF (digital low-pass filter) bandwidth.
pub const REG_CONFIG: u8 = 0x1A;

/// Who-Am-I register. Read to verify device identity.
pub const REG_WHO_AM_I: u8 = 0x75;

/// Expected value of [`REG_WHO_AM_I`] for a genuine MPU6050.
pub const WHO_AM_I_VALUE: u8 = 0x68;

/// First register of the 14-byte accelerometer + temperature + gyro burst.
///
/// Read 14 consecutive bytes starting here to obtain all six axes plus the
/// temperature word. Pass the buffer directly to [`parse_raw`].
pub const REG_ACCEL_XOUT_H: u8 = 0x3B;

/// LSB/g for the +/-2g accelerometer full-scale range.
///
/// Divide a raw accel value by this constant to obtain acceleration in g.
pub const ACCEL_SENSITIVITY_2G: f32 = 16384.0;

/// Startup init sequence — write each `(register, value)` pair in order.
///
/// This configures the sensor for:
/// - Woken from sleep (`PWR_MGMT_1`)
/// - ~44 Hz DLPF bandwidth (`CONFIG`)
/// - ~100 Hz output data rate (`SMPLRT_DIV`)
/// - +/-2g accelerometer range (`ACCEL_CONFIG`)
/// - +/-250 deg/s gyroscope range (`GYRO_CONFIG`)
///
/// Exposed as a slice (not a fixed-size array) so a later minor version can
/// append an additional configuration write without a breaking type change.
pub const INIT_SEQUENCE: &[(u8, u8)] = &[
    (REG_PWR_MGMT_1, 0x00),   // Wake from sleep
    (REG_CONFIG, 0x03),       // DLPF ~44 Hz
    (REG_SMPLRT_DIV, 0x09),   // ~100 Hz sample rate
    (REG_ACCEL_CONFIG, 0x00), // +/-2g
    (REG_GYRO_CONFIG, 0x00),  // +/-250 deg/s
];

// ─── RawReading ──────────────────────────────────────────────────────────────

/// Raw sensor reading from a 14-byte MPU6050 burst.
///
/// All values are in sensor LSBs. Divide accelerometer values by
/// [`ACCEL_SENSITIVITY_2G`] to convert to g-force, or pass them (optionally
/// through [`apply_offsets`]) to `tamer::tilt::tilt_degrees` /
/// `tamer::tilt::tilt_degrees_i32` (feature `tilt`) for angle calculation.
///
/// Temperature bytes (bytes 6-7 in the 14-byte burst) are intentionally
/// skipped — they are not populated in this struct because on-chip
/// temperature measurement is rarely needed for motion sensing and its
/// calibration offset is device-specific.
///
/// `RawReading` has no public constructor and no public fields: the only way
/// to obtain one is [`parse_raw`]. This mirrors
/// [`crate::analog::AnalogSample`] — leaving room to add fields in a later
/// minor version without breaking existing callers.
///
/// The private fields cannot be set from outside the crate, so struct-literal
/// construction does not compile — this locks in the semver guarantee above:
///
/// ```compile_fail
/// use tamer::mpu6050::RawReading;
///
/// // Fields are private: this does not compile.
/// let _ = RawReading {
///     accel_x: 0,
///     accel_y: 0,
///     accel_z: 0,
///     gyro_x: 0,
///     gyro_y: 0,
///     gyro_z: 0,
/// };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawReading {
    accel_x: i16,
    accel_y: i16,
    accel_z: i16,
    gyro_x: i16,
    gyro_y: i16,
    gyro_z: i16,
}

impl RawReading {
    /// Returns the accelerometer X axis, raw LSBs.
    #[must_use]
    pub const fn accel_x(self) -> i16 {
        self.accel_x
    }

    /// Returns the accelerometer Y axis, raw LSBs.
    #[must_use]
    pub const fn accel_y(self) -> i16 {
        self.accel_y
    }

    /// Returns the accelerometer Z axis, raw LSBs.
    #[must_use]
    pub const fn accel_z(self) -> i16 {
        self.accel_z
    }

    /// Returns the gyroscope X axis, raw LSBs.
    #[must_use]
    pub const fn gyro_x(self) -> i16 {
        self.gyro_x
    }

    /// Returns the gyroscope Y axis, raw LSBs.
    #[must_use]
    pub const fn gyro_y(self) -> i16 {
        self.gyro_y
    }

    /// Returns the gyroscope Z axis, raw LSBs.
    #[must_use]
    pub const fn gyro_z(self) -> i16 {
        self.gyro_z
    }
}

// ─── Parsing ─────────────────────────────────────────────────────────────────

/// Parses a 14-byte MPU6050 burst read into a [`RawReading`].
///
/// Byte layout (big-endian, all values are signed 16-bit):
/// - `[0..=1]` — accelerometer X
/// - `[2..=3]` — accelerometer Y
/// - `[4..=5]` — accelerometer Z
/// - `[6..=7]` — temperature (skipped, not stored in [`RawReading`])
/// - `[8..=9]` — gyroscope X
/// - `[10..=11]` — gyroscope Y
/// - `[12..=13]` — gyroscope Z
///
/// Takes a fixed-size `[u8; 14]`, so parsing is total: there is no short-read
/// case to check and this function never panics.
#[must_use]
pub const fn parse_raw(buf: &[u8; 14]) -> RawReading {
    RawReading {
        accel_x: i16::from_be_bytes([buf[0], buf[1]]),
        accel_y: i16::from_be_bytes([buf[2], buf[3]]),
        accel_z: i16::from_be_bytes([buf[4], buf[5]]),
        // buf[6..=7] are temperature — intentionally skipped
        gyro_x: i16::from_be_bytes([buf[8], buf[9]]),
        gyro_y: i16::from_be_bytes([buf[10], buf[11]]),
        gyro_z: i16::from_be_bytes([buf[12], buf[13]]),
    }
}

// ─── Offset application ──────────────────────────────────────────────────────

/// Applies Y/Z calibration offsets to a raw reading, returning corrected
/// values as `i32` to prevent overflow.
///
/// Subtracting two `i16` values can overflow in debug builds when the
/// difference exceeds `i16::MAX` (e.g. `accel_z = 32405` minus a small
/// negative offset). This function widens the reading to `i32` before
/// subtracting, making the arithmetic unconditionally safe. Offsets are
/// already `i32` (see [`AccelOffsets`]), so they need no widening of their
/// own.
///
/// Returns `(corrected_y, corrected_z)` as `(i32, i32)` — a positional pair
/// of corrected axes, kept as a tuple because it is the documented input
/// pairing for `tamer::tilt::tilt_degrees_i32` (feature `tilt`).
///
/// # Arguments
///
/// * `reading` — raw burst reading from [`parse_raw`]
/// * `off_y` — Y calibration offset from [`AccelCalibration::offsets`]
/// * `off_z` — Z calibration offset from [`AccelCalibration::offsets`]
///
/// # Example
///
/// ```
/// use tamer::mpu6050::{apply_offsets, parse_raw};
///
/// let mut buf = [0u8; 14];
/// buf[4] = 0x40; // accel_z = 16384
/// let reading = parse_raw(&buf);
///
/// let (ay, az) = apply_offsets(&reading, 0, 0);
/// assert_eq!(ay, 0);
/// assert_eq!(az, 16384);
/// ```
#[must_use]
pub fn apply_offsets(reading: &RawReading, off_y: i32, off_z: i32) -> (i32, i32) {
    (
        i32::from(reading.accel_y()) - off_y,
        i32::from(reading.accel_z()) - off_z,
    )
}

// ─── AccelCalibration ────────────────────────────────────────────────────────

/// Accelerometer Y/Z axis offsets produced by [`AccelCalibration::offsets`].
///
/// A named return type rather than a bare `(i32, i32)` tuple: it can gain
/// fields (e.g. an X offset) in a later minor version without a signature
/// break, and callers read `offsets.y` / `offsets.z` instead of a
/// positional, error-prone pair.
///
/// Fields are `i32`, not `i16`: the Z offset is an average of `i16` samples
/// minus the `16384` 1g reference, which can fall outside `i16` range (e.g.
/// a sensor resting upside-down averages near `-16384`, and `-16384 - 16384`
/// underflows `i16`). Widening to `i32` here — matching [`apply_offsets`]'s
/// existing `i32` domain — makes the whole calibration-to-correction
/// pipeline overflow-free by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AccelOffsets {
    /// Y-axis calibration offset, in raw accelerometer LSBs.
    pub y: i32,
    /// Z-axis calibration offset, in raw accelerometer LSBs.
    pub z: i32,
}

/// Accelerometer Y/Z calibration accumulator.
///
/// Collects raw accelerometer samples at rest and computes average offsets
/// so that the sensor reads zero on Y and 1g on Z when held in its neutral
/// orientation. Feed samples with [`add_sample`](Self::add_sample), then
/// call [`offsets`](Self::offsets) to retrieve the corrected [`AccelOffsets`].
///
/// Apply the offsets to subsequent readings with [`apply_offsets`] before
/// further processing:
///
/// ```
/// use tamer::mpu6050::AccelCalibration;
///
/// let mut cal = AccelCalibration::new();
/// cal.add_sample(50, 16000);
/// cal.add_sample(60, 16100);
///
/// let offsets = cal.offsets();
/// // Subtract offsets.y / offsets.z from live readings via `apply_offsets`.
/// assert_eq!(offsets.y, 55);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct AccelCalibration {
    sum_y: i32,
    sum_z: i32,
    count: u32,
}

impl AccelCalibration {
    /// Creates a new empty accumulator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sum_y: 0,
            sum_z: 0,
            count: 0,
        }
    }

    /// Feeds one accelerometer sample.
    pub fn add_sample(&mut self, accel_y: i16, accel_z: i16) {
        self.sum_y += i32::from(accel_y);
        self.sum_z += i32::from(accel_z);
        self.count += 1;
    }

    /// Returns the number of samples collected.
    #[must_use]
    pub const fn count(&self) -> u32 {
        self.count
    }

    /// Computes the calibration offsets.
    ///
    /// - `y` is the average Y reading (expected `0` when flat).
    /// - `z` is the average Z reading minus 1g (`16384` LSB for the +/-2g
    ///   range), so the offset is `0` when the sensor correctly reports 1g.
    ///
    /// Both fields are `i32` (see [`AccelOffsets`]): the accumulator sums
    /// `i16` samples into `i32` already, and the whole computation here
    /// stays in `i32`, so this cannot overflow for any sequence of `i16`
    /// samples.
    ///
    /// Returns `AccelOffsets { y: 0, z: 0 }` if no samples have been
    /// collected.
    #[must_use]
    pub fn offsets(&self) -> AccelOffsets {
        if self.count == 0 {
            return AccelOffsets::default();
        }
        let count = self.count as i32;
        AccelOffsets {
            y: self.sum_y / count,
            z: self.sum_z / count - 16384,
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_raw ───────────────────────────────────────────────────────

    #[test]
    fn parse_known_positive_values() {
        let mut buf = [0u8; 14];
        // accel_x = 0x0064 = 100
        buf[0] = 0x00;
        buf[1] = 0x64;
        // accel_y = 0x01F4 = 500
        buf[2] = 0x01;
        buf[3] = 0xF4;
        // accel_z = 0x4000 = 16384
        buf[4] = 0x40;
        buf[5] = 0x00;
        // gyro_x = 0x0032 = 50
        buf[8] = 0x00;
        buf[9] = 0x32;
        // gyro_y = 0x00C8 = 200
        buf[10] = 0x00;
        buf[11] = 0xC8;
        // gyro_z = 0x0190 = 400
        buf[12] = 0x01;
        buf[13] = 0x90;

        let r = parse_raw(&buf);
        assert_eq!(r.accel_x(), 100);
        assert_eq!(r.accel_y(), 500);
        assert_eq!(r.accel_z(), 16384);
        assert_eq!(r.gyro_x(), 50);
        assert_eq!(r.gyro_y(), 200);
        assert_eq!(r.gyro_z(), 400);
    }

    #[test]
    fn parse_known_negative_values() {
        let mut buf = [0u8; 14];
        // accel_x = -100: two's complement 0xFF9C
        buf[0] = 0xFF;
        buf[1] = 0x9C;
        // gyro_z = -200: two's complement 0xFF38
        buf[12] = 0xFF;
        buf[13] = 0x38;

        let r = parse_raw(&buf);
        assert_eq!(r.accel_x(), -100);
        assert_eq!(r.gyro_z(), -200);
    }

    #[test]
    fn parse_zero_buffer_gives_all_zeros() {
        let buf = [0u8; 14];
        let r = parse_raw(&buf);
        assert_eq!(r, parse_raw(&[0u8; 14]));
        assert_eq!(r.accel_x(), 0);
        assert_eq!(r.accel_y(), 0);
        assert_eq!(r.accel_z(), 0);
        assert_eq!(r.gyro_x(), 0);
        assert_eq!(r.gyro_y(), 0);
        assert_eq!(r.gyro_z(), 0);
    }

    #[test]
    fn parse_max_positive_all_axes() {
        // 0x7FFF = 32767
        let mut buf = [0u8; 14];
        for i in [0, 2, 4, 8, 10, 12] {
            buf[i] = 0x7F;
            buf[i + 1] = 0xFF;
        }
        let r = parse_raw(&buf);
        assert_eq!(r.accel_x(), i16::MAX);
        assert_eq!(r.accel_y(), i16::MAX);
        assert_eq!(r.accel_z(), i16::MAX);
        assert_eq!(r.gyro_x(), i16::MAX);
        assert_eq!(r.gyro_y(), i16::MAX);
        assert_eq!(r.gyro_z(), i16::MAX);
    }

    #[test]
    fn parse_max_negative_all_axes() {
        // 0x8000 = -32768
        let mut buf = [0u8; 14];
        for i in [0, 2, 4, 8, 10, 12] {
            buf[i] = 0x80;
            buf[i + 1] = 0x00;
        }
        let r = parse_raw(&buf);
        assert_eq!(r.accel_x(), i16::MIN);
        assert_eq!(r.accel_y(), i16::MIN);
        assert_eq!(r.accel_z(), i16::MIN);
        assert_eq!(r.gyro_x(), i16::MIN);
        assert_eq!(r.gyro_y(), i16::MIN);
        assert_eq!(r.gyro_z(), i16::MIN);
    }

    #[test]
    fn parse_temperature_bytes_do_not_affect_result() {
        let mut buf_a = [0u8; 14];
        buf_a[0] = 0x40;
        buf_a[1] = 0x00;
        let mut buf_b = buf_a;
        // Bytes 6-7 are temperature — modifying them must not change the result.
        buf_b[6] = 0xFF;
        buf_b[7] = 0xFF;
        assert_eq!(parse_raw(&buf_a), parse_raw(&buf_b));
    }

    #[test]
    fn parse_byte_order_matters() {
        let mut buf_normal = [0u8; 14];
        buf_normal[0] = 0x01;
        buf_normal[1] = 0x00; // 0x0100 = 256

        let mut buf_swapped = [0u8; 14];
        buf_swapped[0] = 0x00;
        buf_swapped[1] = 0x01; // 0x0001 = 1

        assert_ne!(
            parse_raw(&buf_normal).accel_x(),
            parse_raw(&buf_swapped).accel_x()
        );
        assert_eq!(parse_raw(&buf_normal).accel_x(), 256);
        assert_eq!(parse_raw(&buf_swapped).accel_x(), 1);
    }

    // ── apply_offsets ────────────────────────────────────────────────────

    fn reading_with(accel_y: i16, accel_z: i16) -> RawReading {
        let mut buf = [0u8; 14];
        let [yh, yl] = accel_y.to_be_bytes();
        buf[2] = yh;
        buf[3] = yl;
        let [zh, zl] = accel_z.to_be_bytes();
        buf[4] = zh;
        buf[5] = zl;
        parse_raw(&buf)
    }

    #[test]
    fn apply_offsets_zero_reading_zero_offsets_returns_zero() {
        assert_eq!(apply_offsets(&reading_with(0, 0), 0, 0), (0, 0));
    }

    #[test]
    fn apply_offsets_subtracts_positive_offset() {
        let reading = reading_with(100, 0);
        assert_eq!(apply_offsets(&reading, 20, 0), (80, 0));
    }

    #[test]
    fn apply_offsets_negative_offset_increases_value() {
        let reading = reading_with(10, 0);
        assert_eq!(apply_offsets(&reading, -50, 0), (60, 0));
    }

    #[test]
    fn apply_offsets_overflow_scenario_does_not_panic() {
        let reading = reading_with(0, 32405);
        let (_, cz) = apply_offsets(&reading, 0, -400);
        assert_eq!(cz, 32805_i32);
    }

    // ── AccelCalibration ────────────────────────────────────────────────

    #[test]
    fn calibration_no_samples_returns_zero_offsets() {
        let cal = AccelCalibration::new();
        assert_eq!(cal.count(), 0);
        assert_eq!(cal.offsets(), AccelOffsets { y: 0, z: 0 });
    }

    #[test]
    fn calibration_single_sample() {
        let mut cal = AccelCalibration::new();
        // Perfect flat reading: Y=0, Z=16384 (exactly 1g).
        cal.add_sample(0, 16384);
        assert_eq!(cal.count(), 1);
        let offsets = cal.offsets();
        assert_eq!(offsets.y, 0);
        assert_eq!(offsets.z, 0); // avg_z(16384) - 16384 = 0
    }

    #[test]
    fn calibration_multiple_samples_average() {
        let mut cal = AccelCalibration::new();
        // Y readings: 100, 50 -> avg = 75
        // Z readings: 16484, 16284 -> avg = 16384 -> offset = 0
        cal.add_sample(100, 16484);
        cal.add_sample(50, 16284);
        let offsets = cal.offsets();
        assert_eq!(offsets.y, 75);
        assert_eq!(offsets.z, 0);
    }

    #[test]
    fn calibration_z_offset_subtracts_one_g_reference() {
        let mut cal = AccelCalibration::new();
        // Z reads 16000 instead of 16384 -> offset = 16000 - 16384 = -384
        cal.add_sample(0, 16000);
        let offsets = cal.offsets();
        assert_eq!(offsets.z, 16000_i32 - 16384_i32);
    }

    #[test]
    fn calibration_extreme_negative_z_does_not_panic() {
        // Regression test: a sensor resting upside-down reads accel_z close
        // to -16384 (or, at the extreme, i16::MIN). The old i16-narrowed
        // computation (`avg_z - 16384_i16`) underflowed i16::MIN and panicked
        // in debug builds. The i32 pipeline must not panic and must produce
        // the exact widened result.
        let mut cal = AccelCalibration::new();
        for _ in 0..5 {
            cal.add_sample(0, i16::MIN);
        }
        let offsets = cal.offsets();
        assert_eq!(offsets.z, i32::from(i16::MIN) - 16384);
        assert_eq!(offsets.z, -49152);
    }

    #[test]
    fn calibration_extreme_negative_z_offsets_feed_apply_offsets_cleanly() {
        let mut cal = AccelCalibration::new();
        cal.add_sample(0, i16::MIN);
        let offsets = cal.offsets();

        // A live reading identical to the calibration sample should correct
        // back to exactly the 1g reference (16384) once the offset is
        // applied: accel_z - offset_z == accel_z - (accel_z - 16384) == 16384.
        let reading = reading_with(0, i16::MIN);
        let (ay, az) = apply_offsets(&reading, offsets.y, offsets.z);
        assert_eq!(ay, 0);
        assert_eq!(az, i32::from(i16::MIN) - offsets.z);
        assert_eq!(az, 16384);
    }

    #[test]
    fn calibration_negative_y_offset() {
        let mut cal = AccelCalibration::new();
        cal.add_sample(-200, 16384);
        let offsets = cal.offsets();
        assert_eq!(offsets.y, -200);
    }

    #[test]
    fn calibration_default_trait() {
        let cal = AccelCalibration::default();
        assert_eq!(cal.count(), 0);
        assert_eq!(cal.offsets(), AccelOffsets { y: 0, z: 0 });
    }

    // ── Protocol constants ──────────────────────────────────────────────

    #[test]
    fn protocol_constant_i2c_addr() {
        assert_eq!(I2C_ADDR, 0x68);
    }

    #[test]
    fn protocol_constant_reg_accel_xout_h() {
        assert_eq!(REG_ACCEL_XOUT_H, 0x3B);
    }

    #[test]
    fn protocol_constant_who_am_i_value() {
        assert_eq!(WHO_AM_I_VALUE, 0x68);
    }

    #[test]
    fn protocol_init_sequence_length() {
        assert_eq!(INIT_SEQUENCE.len(), 5);
    }

    #[test]
    fn protocol_init_sequence_entries() {
        assert_eq!(INIT_SEQUENCE[0], (REG_PWR_MGMT_1, 0x00));
        assert_eq!(INIT_SEQUENCE[1], (REG_CONFIG, 0x03));
        assert_eq!(INIT_SEQUENCE[2], (REG_SMPLRT_DIV, 0x09));
        assert_eq!(INIT_SEQUENCE[3], (REG_ACCEL_CONFIG, 0x00));
        assert_eq!(INIT_SEQUENCE[4], (REG_GYRO_CONFIG, 0x00));
    }
}
