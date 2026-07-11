//! Scale-free two-axis tilt-angle trigonometry (feature `tilt`).
//!
//! [`tilt_degrees`] and [`tilt_degrees_i32`] compute an inclination angle
//! from two accelerometer axes via `atan2`. The functions take caller-supplied
//! values in shared units — raw LSBs, physical g, or any other consistent
//! scale — and have no MPU6050 (or any other device) register knowledge; pair
//! them with [`crate::mpu6050`] or any other accelerometer source.
//!
//! This module is gated behind the `tilt` Cargo feature because `atan2`
//! requires [`micromath`], a `no_std` CORDIC approximation library — the only
//! floating-point-trigonometry dependency in `tamer`. The default build stays
//! dependency-free; enable `tilt` only where an FPU (or acceptable software-float
//! cost) is available.
//!
//! `micromath`'s CORDIC `atan2` is a bounded-error approximation (accurate to
//! roughly +/-0.1%), not bit-exact versus `libm`/`std`. Host tests in this
//! module assert results within a tolerance, never by exact `f32` equality.
//!
//! # Example
//!
//! ```
//! use tamer::tilt::tilt_degrees;
//!
//! // Sensor flat: Y = 0, Z = +1g (16384 LSB at +/-2g) -> ~0 degrees.
//! let tilt = tilt_degrees(0, 16384);
//! assert!(tilt.abs() < 1.0);
//! ```

/// Computes the tilt angle in degrees from the Y and Z accelerometer axes.
///
/// Uses `atan2(y, z)` to determine the angle of the sensor about the X axis,
/// expressed in degrees. A value of `0.0` degrees means the sensor is flat
/// (Z points up), `+90.0` degrees means the Y axis points up, and `-90.0`
/// degrees means the Y axis points down.
///
/// Tilt is computed via [`micromath::F32Ext::atan2`], which uses a CORDIC
/// approximation accurate to approximately +/-0.1%. `atan2(0, 0)` is
/// implementation-defined but returns a finite value — it never panics.
///
/// See also [`tilt_degrees_i32`] for an `i32` variant that avoids overflow
/// when applying calibration offsets to wide raw values.
///
/// # Arguments
///
/// * `accel_y` — raw Y accelerometer value (LSBs)
/// * `accel_z` — raw Z accelerometer value (LSBs)
#[must_use]
pub fn tilt_degrees(accel_y: i16, accel_z: i16) -> f32 {
    tilt_degrees_i32(i32::from(accel_y), i32::from(accel_z))
}

/// Computes the tilt angle in degrees from the Y and Z accelerometer axes,
/// accepting `i32` inputs to avoid overflow when applying calibration offsets.
///
/// Identical in behaviour to [`tilt_degrees`] — uses `atan2(y, z)` to
/// determine the angle of the sensor about the X axis, expressed in degrees.
/// A value of `0.0` degrees means the sensor is flat (Z points up), `+90.0`
/// degrees means the Y axis points up, and `-90.0` degrees means the Y axis
/// points down.
///
/// Use this function after subtracting `i16` calibration offsets from `i16`
/// raw readings: casting both operands to `i32` before subtraction prevents
/// the `attempt to subtract with overflow` panic that occurs in debug builds
/// when the difference crosses `i16::MAX` or `i16::MIN`. The companion
/// helper [`crate::mpu6050::apply_offsets`] does this widening for you.
///
/// Tilt is computed via [`micromath::F32Ext::atan2`], accurate to
/// approximately +/-0.1%.
///
/// # Arguments
///
/// * `accel_y` — calibration-corrected Y accelerometer value (LSBs, `i32`)
/// * `accel_z` — calibration-corrected Z accelerometer value (LSBs, `i32`)
///
/// # Example
///
/// ```
/// use tamer::tilt::tilt_degrees_i32;
///
/// // Hardware reading after calibration: corrected accel_z exceeds i16::MAX.
/// // accel_z raw = 32405, offset = -400 -> corrected = 32805 (> 32767)
/// let tilt = tilt_degrees_i32(0, 32_805);
/// assert!(tilt.abs() < 1.0); // sensor is still mostly flat
/// ```
#[must_use]
pub fn tilt_degrees_i32(accel_y: i32, accel_z: i32) -> f32 {
    use micromath::F32;
    let y = F32(accel_y as f32);
    let z = F32(accel_z as f32);
    // F32::atan2 is the inherent method on the micromath newtype.
    y.atan2(z).0 * (180.0 / core::f32::consts::PI)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpu6050::{apply_offsets, parse_raw};

    fn assert_approx(actual: f32, expected: f32, tolerance: f32) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= tolerance,
            "expected {expected} +/- {tolerance}, got {actual} (diff {diff})"
        );
    }

    fn reading_with(accel_y: i16, accel_z: i16) -> crate::mpu6050::RawReading {
        let mut buf = [0u8; 14];
        let [yh, yl] = accel_y.to_be_bytes();
        buf[2] = yh;
        buf[3] = yl;
        let [zh, zl] = accel_z.to_be_bytes();
        buf[4] = zh;
        buf[5] = zl;
        parse_raw(&buf)
    }

    // ── tilt_degrees ────────────────────────────────────────────────────

    #[test]
    fn tilt_flat_is_zero_degrees() {
        // Sensor flat: Z = +1g, Y = 0 -> 0 degrees
        let t = tilt_degrees(0, 16384);
        assert_approx(t, 0.0, 1.0);
    }

    #[test]
    fn tilt_vertical_up_is_ninety_degrees() {
        // Y = +1g, Z = 0 -> 90 degrees
        let t = tilt_degrees(16384, 0);
        assert_approx(t, 90.0, 1.0);
    }

    #[test]
    fn tilt_vertical_down_is_minus_ninety_degrees() {
        // Y = -1g, Z = 0 -> -90 degrees
        let t = tilt_degrees(-16384, 0);
        assert_approx(t, -90.0, 1.0);
    }

    #[test]
    fn tilt_forty_five_degrees() {
        // Y ~= Z ~= 1g/sqrt(2) in LSBs: 16384/sqrt(2) ~= 11585
        let t = tilt_degrees(11585, 11585);
        assert_approx(t, 45.0, 1.0);
    }

    #[test]
    fn tilt_zero_vector_does_not_panic() {
        // atan2(0, 0) is implementation-defined but must not panic.
        let _ = tilt_degrees(0, 0);
    }

    // ── tilt_degrees_i32 ──────────────────────────────────────────────────

    #[test]
    fn tilt_degrees_i32_matches_i16_version_flat() {
        assert_approx(tilt_degrees_i32(0, 16384), tilt_degrees(0, 16384), 0.01);
    }

    #[test]
    fn tilt_degrees_i32_matches_i16_version_positive_ninety() {
        assert_approx(tilt_degrees_i32(16384, 0), tilt_degrees(16384, 0), 0.01);
    }

    #[test]
    fn tilt_degrees_i32_matches_i16_version_negative_ninety() {
        assert_approx(tilt_degrees_i32(-16384, 0), tilt_degrees(-16384, 0), 0.01);
    }

    #[test]
    fn tilt_degrees_i32_matches_i16_version_forty_five() {
        assert_approx(
            tilt_degrees_i32(11585, 11585),
            tilt_degrees(11585, 11585),
            0.01,
        );
    }

    #[test]
    fn tilt_degrees_i32_large_positive_y_does_not_panic() {
        let t = tilt_degrees_i32(37767, 16384);
        assert!(t.is_finite());
        assert!(t > 0.0 && t < 90.0);
    }

    #[test]
    fn tilt_degrees_i32_large_negative_y_does_not_panic() {
        let t = tilt_degrees_i32(-37768, 16384);
        assert!(t.is_finite());
        assert!(t < 0.0 && t > -90.0);
    }

    #[test]
    fn tilt_degrees_i32_large_positive_z_does_not_panic() {
        let t = tilt_degrees_i32(0, 33767);
        assert!(t.is_finite());
        assert_approx(t, 0.0, 1.0);
    }

    #[test]
    fn tilt_degrees_i32_corrected_z_beyond_i16_max_regression() {
        // Hardware reports accel_z=32405. Calibration offset off_z=-400.
        // Corrected: 32405 - (-400) = 32805, which overflows i16::MAX=32767.
        // tilt_degrees_i32 accepts i32 directly; if a caller wraps to i16
        // first they would get a near-180-degree wrong angle. This test
        // locks in the contrast.
        let t = tilt_degrees_i32(0, 32805);
        assert!(t.is_finite());
        assert_approx(t, 0.0, 1.0);

        let wrapped_z = 32805_i32 as i16; // -32731 in two's complement
        let wrong_tilt = tilt_degrees(0, wrapped_z);
        assert!(
            wrong_tilt.abs() > 170.0,
            "i16 wrap produces ~180 degree error"
        );
    }

    // ── apply_offsets + tilt_degrees_i32 regression ──────────────────────

    #[test]
    fn apply_offsets_result_feeds_tilt_degrees_i32() {
        let reading = reading_with(0, 32405);
        let (cy, cz) = apply_offsets(&reading, 0, -400);
        let t = tilt_degrees_i32(cy, cz);
        assert!(t.is_finite());
        assert_approx(t, 0.0, 1.0);
    }
}
