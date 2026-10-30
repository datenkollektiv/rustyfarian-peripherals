//! BME680 environmental-sensor protocol constants, raw-block parsing, and
//! compensation (feature `bme680`).
//!
//! Provides platform-independent types and functions for the Bosch BME680
//! (temperature, pressure, humidity, and gas resistance) over I2C. Like
//! [`crate::mpu6050`], the transport is left to the caller: this module only
//! interprets the register map and byte layout, builds control-register
//! bytes, and turns raw ADC counts into physical units. It imports no I2C,
//! HAL, or chip crate, and uses only `core` `f32` arithmetic — no dependency.
//!
//! Every formula is a direct transcription of the floating-point path of the
//! Bosch [BME68x SensorAPI v4.4.8](https://github.com/boschsensortec/BME68x_SensorAPI/tree/v4.4.8)
//! (`bme68x.c`, the `BME68X_USE_FPU` branch: `calc_temperature`,
//! `calc_pressure`, `calc_humidity`, `calc_gas_resistance_low`,
//! `calc_res_heat`, `calc_gas_wait`, `bme68x_get_meas_dur`, and
//! `get_calib_data`), restricted to the BME680 ("low" gas variant).
//!
//! Gas resistance is reported in raw ohms. Interpreting it as air quality
//! (humidity correction, baselines, IAQ) is left to the consumer.
//!
//! # Protocol
//!
//! The BME680 listens on [`I2C_ADDR`](crate::bme680::I2C_ADDR) (`0x76`, SDO low) or [`I2C_ADDR_ALT`](crate::bme680::I2C_ADDR_ALT)
//! (`0x77`, SDO high). After a soft reset ([`REG_SOFT_RESET`](crate::bme680::REG_SOFT_RESET) /
//! [`SOFT_RESET_CMD`](crate::bme680::SOFT_RESET_CMD), then [`RESET_DELAY_MS`](crate::bme680::RESET_DELAY_MS)) read [`REG_CHIP_ID`](crate::bme680::REG_CHIP_ID) and
//! compare to [`CHIP_ID_VALUE`](crate::bme680::CHIP_ID_VALUE). Read the calibration blocks at
//! [`REG_COEFF1`](crate::bme680::REG_COEFF1) (25 bytes) and [`REG_COEFF2`](crate::bme680::REG_COEFF2) (16 bytes) plus the three
//! single-byte heater/range registers, and pass them to
//! [`CalibrationData::from_raw`](crate::bme680::CalibrationData::from_raw). A forced-mode measurement is configured
//! with [`config_byte`](crate::bme680::config_byte), [`ctrl_hum_byte`](crate::bme680::ctrl_hum_byte), the heater registers
//! ([`res_heat`](crate::bme680::res_heat), [`gas_wait`](crate::bme680::gas_wait), [`ctrl_gas_0_byte`](crate::bme680::ctrl_gas_0_byte), [`ctrl_gas_1_byte`](crate::bme680::ctrl_gas_1_byte)) and
//! triggered with [`ctrl_meas_byte`](crate::bme680::ctrl_meas_byte). When [`is_measurement_ready`](crate::bme680::is_measurement_ready) holds,
//! read 15 bytes from [`REG_MEAS_STATUS_0`](crate::bme680::REG_MEAS_STATUS_0) and pass them to [`parse_raw`](crate::bme680::parse_raw)
//! and [`compensate`](crate::bme680::compensate).
//!
//! # Example
//!
//! ```
//! use tamer::bme680::{compensate, parse_raw, CalibrationData};
//!
//! // Calibration blocks as read from 0x89 (25 bytes) and 0xE1 (16 bytes).
//! let coeff1 = [
//!     0x00, 0x66, 0x66, 0x03, 0x00, 0x72, 0x8D, 0x2E, 0xD7, 0x58, 0x00, 0xD4, 0x1C,
//!     0x79, 0xFF, 0x28, 0x1E, 0x00, 0x00, 0xF6, 0xF5, 0xEA, 0xF7, 0x1E, 0x00,
//! ];
//! let coeff2 = [
//!     0x3F, 0x2D, 0x32, 0x00, 0x2D, 0x14, 0x78, 0x9C, 0x9D, 0x65, 0x20, 0xEA, 0xE2,
//!     0x12, 0x00, 0x00,
//! ];
//! let calib = CalibrationData::from_raw(&coeff1, &coeff2, 1, -12, -2).unwrap();
//!
//! // Data block as read from 0x1D (15 bytes).
//! let block = [
//!     0x80, 0x00, 0x55, 0xD6, 0xB0, 0x79, 0x24, 0x60, 0x58, 0x76, 0x00, 0x00, 0x00,
//!     0xD5, 0xF7,
//! ];
//! let reading = compensate(&calib, &parse_raw(&block));
//! assert!((reading.temperature_c() - 25.0).abs() < 0.01);
//! assert!((reading.pressure_hpa() - 1000.0).abs() < 0.01);
//! assert!((reading.humidity_rh() - 50.0).abs() < 0.01);
//! assert!(reading.gas_resistance_ohm().is_some());
//! ```

// ─── Protocol constants ─────────────────────────────────────────────────────

/// Primary I2C address (SDO pin low).
pub const I2C_ADDR: u8 = 0x76;

/// Alternate I2C address (SDO pin high).
pub const I2C_ADDR_ALT: u8 = 0x77;

/// Chip-ID register. Read to verify device identity.
pub const REG_CHIP_ID: u8 = 0xD0;

/// Expected value of [`REG_CHIP_ID`] (Bosch `BME68X_CHIP_ID`); shared by the
/// BME680 and the BME688, see [`REG_VARIANT_ID`].
pub const CHIP_ID_VALUE: u8 = 0x61;

/// Soft-reset register. Write [`SOFT_RESET_CMD`] to reset the device.
pub const REG_SOFT_RESET: u8 = 0xE0;

/// Soft-reset command written to [`REG_SOFT_RESET`].
pub const SOFT_RESET_CMD: u8 = 0xB6;

/// Wait after a soft reset before the next access, in milliseconds
/// (Bosch `BME68X_PERIOD_RESET`, 10 000 µs).
pub const RESET_DELAY_MS: u32 = 10;

/// First register of the first calibration block; read [`COEFF1_LEN`] bytes.
pub const REG_COEFF1: u8 = 0x89;

/// Length of the calibration block at [`REG_COEFF1`], in bytes.
pub const COEFF1_LEN: usize = 25;

/// Variant-ID register (Bosch `BME68X_REG_VARIANT_ID`), the last byte of the
/// [`REG_COEFF2`] block; `0x00` is the BME680, `0x01` the BME688.
pub const REG_VARIANT_ID: u8 = 0xF0;

/// Value of [`REG_VARIANT_ID`] for a BME680 (Bosch `BME68X_VARIANT_GAS_LOW`).
pub const VARIANT_ID_BME680: u8 = 0x00;

/// First register of the second calibration block; read [`COEFF2_LEN`] bytes.
pub const REG_COEFF2: u8 = 0xE1;

/// Length of the block at [`REG_COEFF2`], in bytes.
///
/// Bosch reads 14 calibration bytes (`0xE1..=0xEE`); `0xEF` is not
/// calibration and `0xF0` is [`REG_VARIANT_ID`].
pub const COEFF2_LEN: usize = 16;

/// Heater-resistance trim register (`res_heat_val`, signed).
pub const REG_RES_HEAT_VAL: u8 = 0x00;

/// Heater-range register; decode with [`res_heat_range_from_reg`].
pub const REG_RES_HEAT_RANGE: u8 = 0x02;

/// Gas range switching-error register; decode with [`range_sw_err_from_reg`].
pub const REG_RANGE_SW_ERR: u8 = 0x04;

/// `meas_status_0`: first register of the data block; read [`DATA_LEN`]
/// bytes for [`parse_raw`], or one byte for the status helpers.
pub const REG_MEAS_STATUS_0: u8 = 0x1D;

/// Length of the BME680 data block at [`REG_MEAS_STATUS_0`], in bytes.
pub const DATA_LEN: usize = 15;

/// Heater set-point 0 target resistance register (`res_heat_0`); write a
/// [`res_heat`] value.
pub const REG_RES_HEAT_0: u8 = 0x5A;

/// Heater set-point 0 duration register (`gas_wait_0`); write a
/// [`gas_wait`] value.
pub const REG_GAS_WAIT_0: u8 = 0x64;

/// `ctrl_gas_0` register (heater off bit); write a [`ctrl_gas_0_byte`].
pub const REG_CTRL_GAS_0: u8 = 0x70;

/// `ctrl_gas_1` register (`run_gas`, `nb_conv`); write a [`ctrl_gas_1_byte`].
pub const REG_CTRL_GAS_1: u8 = 0x71;

/// `ctrl_hum` register (humidity oversampling); write a [`ctrl_hum_byte`].
pub const REG_CTRL_HUM: u8 = 0x72;

/// `ctrl_meas` register (temperature / pressure oversampling and mode);
/// write a [`ctrl_meas_byte`] to trigger a forced-mode measurement.
pub const REG_CTRL_MEAS: u8 = 0x74;

/// `config` register (IIR filter); write a [`config_byte`].
pub const REG_CONFIG: u8 = 0x75;

/// Fixed v1 heater target temperature, in °C.
pub const HEATER_TARGET_C: f32 = 320.0;

/// Fixed v1 heater duration, in milliseconds.
pub const HEATER_DURATION_MS: u16 = 150;

/// `meas_status_0` bit 7: a new data block is available.
const STATUS_NEW_DATA: u8 = 0x80;
/// `meas_status_0` bit 6: a gas conversion is running (BME680 datasheet).
const STATUS_GAS_MEASURING: u8 = 0x40;
/// `meas_status_0` bit 5: a conversion is running (BME680 datasheet).
const STATUS_MEASURING: u8 = 0x20;
/// `gas_r_lsb` bit 5: `gas_valid_r` (Bosch `BME68X_GASM_VALID_MSK`).
const GAS_VALID: u8 = 0x20;
/// `gas_r_lsb` bit 4: `heat_stab_r` (Bosch `BME68X_HEAT_STAB_MSK`).
const HEAT_STAB: u8 = 0x10;
/// `gas_r_lsb` bits 3:0: gas range (Bosch `BME68X_GAS_RANGE_MSK`).
const GAS_RANGE_MASK: u8 = 0x0F;
/// `ctrl_meas` bits 1:0 = `0b01`: forced mode.
const MODE_FORCED: u8 = 0b01;
/// `ctrl_meas` bits 1:0: operating mode (`0b00` = sleep).
const MODE_MASK: u8 = 0b11;
/// `ctrl_gas_0` bit 3: `heat_off` (Bosch `BME68X_HCTRL_MSK`).
const HEAT_OFF: u8 = 0x08;
/// `ctrl_gas_1` bit 4: `run_gas` (BME680 low variant).
const RUN_GAS: u8 = 0x10;

// ─── Oversampling / IIR filter ───────────────────────────────────────────────

/// Oversampling setting for one of the temperature, pressure, or humidity
/// channels.
///
/// The discriminant is the 3-bit register field (Bosch `BME68X_OS_*`), so
/// every value is valid by construction.
///
/// There is deliberately no "skip" setting (encoding 0).
/// A skipped channel makes the sensor emit the marker `0x80000` (`0x8000` for
/// humidity), which [`compensate`] would turn into a plausible number, and
/// skipping temperature breaks `t_fine` for every other channel.
/// The driver therefore always samples all three channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Oversampling {
    /// 1x oversampling.
    X1 = 1,
    /// 2x oversampling.
    X2 = 2,
    /// 4x oversampling.
    X4 = 3,
    /// 8x oversampling.
    X8 = 4,
    /// 16x oversampling.
    X16 = 5,
}

impl Oversampling {
    /// Returns the 3-bit register encoding (`1..=5`).
    #[must_use]
    pub const fn bits(self) -> u8 {
        self as u8
    }

    /// Returns the number of ADC conversion cycles (Bosch
    /// `os_to_meas_cycles`).
    const fn cycles(self) -> u32 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::X8 => 8,
            Self::X16 => 16,
        }
    }
}

/// IIR filter coefficient applied to temperature and pressure.
///
/// The discriminant is the 3-bit `filter` field of [`REG_CONFIG`] (Bosch
/// `BME68X_FILTER_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum IirFilter {
    /// Filter off.
    Off = 0,
    /// Filter coefficient 1.
    Coeff1 = 1,
    /// Filter coefficient 3.
    Coeff3 = 2,
    /// Filter coefficient 7.
    Coeff7 = 3,
    /// Filter coefficient 15.
    Coeff15 = 4,
    /// Filter coefficient 31.
    Coeff31 = 5,
    /// Filter coefficient 63.
    Coeff63 = 6,
    /// Filter coefficient 127.
    Coeff127 = 7,
}

impl IirFilter {
    /// Returns the 3-bit register encoding (`0..=7`).
    #[must_use]
    pub const fn bits(self) -> u8 {
        self as u8
    }
}

// ─── Register-byte builders ──────────────────────────────────────────────────

/// Builds the [`REG_CTRL_HUM`] byte: `osrs_h` in bits 2:0.
#[must_use]
pub const fn ctrl_hum_byte(osrs_h: Oversampling) -> u8 {
    osrs_h.bits()
}

/// Builds the [`REG_CTRL_MEAS`] byte that triggers one forced-mode
/// measurement: `osrs_t` in bits 7:5, `osrs_p` in bits 4:2, mode `0b01`.
///
/// Write [`REG_CTRL_HUM`] first: the datasheet applies a humidity change only
/// after a `ctrl_meas` write.
#[must_use]
pub const fn ctrl_meas_byte(osrs_t: Oversampling, osrs_p: Oversampling) -> u8 {
    (osrs_t.bits() << 5) | (osrs_p.bits() << 2) | MODE_FORCED
}

/// Returns the operating-mode bits (1:0) of a [`REG_CTRL_MEAS`] value;
/// `0b00` is sleep mode, `0b01` forced mode.
#[must_use]
pub const fn op_mode(ctrl_meas: u8) -> u8 {
    ctrl_meas & MODE_MASK
}

/// Returns a [`REG_CTRL_MEAS`] value with the mode bits cleared (sleep mode),
/// keeping the oversampling bits (Bosch `bme68x_set_op_mode` to sleep).
#[must_use]
pub const fn with_sleep_mode(ctrl_meas: u8) -> u8 {
    ctrl_meas & !MODE_MASK
}

/// Builds the [`REG_CONFIG`] byte: `filter` in bits 4:2, SPI 3-wire off.
#[must_use]
pub const fn config_byte(filter: IirFilter) -> u8 {
    filter.bits() << 2
}

/// Builds the [`REG_CTRL_GAS_0`] byte: `0x00` enables the heater, `0x08`
/// sets `heat_off`.
#[must_use]
pub const fn ctrl_gas_0_byte(heater_enabled: bool) -> u8 {
    if heater_enabled {
        0x00
    } else {
        HEAT_OFF
    }
}

/// Builds the [`REG_CTRL_GAS_1`] byte: `run_gas` in bit 4 and heater
/// set-point 0 (`nb_conv` = 0) in bits 3:0.
///
/// v1 has exactly one heater profile ([`HEATER_TARGET_C`] for
/// [`HEATER_DURATION_MS`]), so the set-point index is not a parameter.
/// Further set-points would be an additive later change.
#[must_use]
pub const fn ctrl_gas_1_byte(run_gas: bool) -> u8 {
    if run_gas {
        RUN_GAS
    } else {
        0
    }
}

/// Encodes a heater duration in milliseconds as a [`REG_GAS_WAIT_0`] byte
/// (Bosch v4.4.8 `calc_gas_wait`).
///
/// The byte is a 6-bit mantissa and a 2-bit multiplier (x1, x4, x16, x64).
/// Durations of `0xFC0` ms (4032 ms) and above saturate to `0xFF`.
///
/// ```
/// assert_eq!(tamer::bme680::gas_wait(150), 0x65);
/// ```
#[must_use]
pub const fn gas_wait(duration_ms: u16) -> u8 {
    if duration_ms >= 0xFC0 {
        return 0xFF;
    }
    let mut dur = duration_ms;
    let mut factor: u8 = 0;
    while dur > 0x3F {
        dur /= 4;
        factor += 1;
    }
    // dur <= 0x3F and factor <= 3 here, so the sum fits in a u8.
    dur as u8 + factor * 64
}

// ─── Status bits ─────────────────────────────────────────────────────────────

/// Returns `true` if `new_data_0` (bit 7) is set in a `meas_status_0` byte.
#[must_use]
pub const fn is_new_data(status: u8) -> bool {
    status & STATUS_NEW_DATA != 0
}

/// Returns `true` if `measuring` (bit 5) is set in a `meas_status_0` byte.
#[must_use]
pub const fn is_measuring(status: u8) -> bool {
    status & STATUS_MEASURING != 0
}

/// Returns `true` if `gas_measuring` (bit 6) is set in a `meas_status_0` byte.
#[must_use]
pub const fn is_gas_measuring(status: u8) -> bool {
    status & STATUS_GAS_MEASURING != 0
}

/// Returns `true` once a forced-mode measurement is complete: `new_data_0`
/// is set and both `measuring` and `gas_measuring` are clear.
#[must_use]
pub const fn is_measurement_ready(status: u8) -> bool {
    is_new_data(status) && !is_measuring(status) && !is_gas_measuring(status)
}

// ─── Calibration ─────────────────────────────────────────────────────────────

/// Decodes the [`REG_RES_HEAT_RANGE`] register into the heater range
/// (`(reg & 0x30) >> 4`, `0..=3`).
#[must_use]
pub const fn res_heat_range_from_reg(reg: u8) -> u8 {
    (reg & 0x30) >> 4
}

/// Decodes the [`REG_RANGE_SW_ERR`] register into the signed switching error
/// (`((reg & 0xF0) as i8) >> 4`, `-8..=7`).
///
/// The field is the signed upper nibble; shifting it as `i8` keeps the sign
/// (an unsigned shift would turn `-2` into `14`).
#[must_use]
pub const fn range_sw_err_from_reg(reg: u8) -> i8 {
    ((reg & 0xF0) as i8) >> 4
}

/// Error returned by [`CalibrationData::from_raw`] for an implausible
/// calibration read.
///
/// A calibration block of uniform `0x00` or `0xFF` bytes is what a missing,
/// unpowered, or stuck device returns; it would compensate to nonsense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalibrationError {
    /// A calibration block read as all `0x00`.
    AllZeros,
    /// A calibration block read as all `0xFF`.
    AllOnes,
}

impl core::fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AllZeros => f.write_str("BME680 calibration block reads as all 0x00"),
            Self::AllOnes => f.write_str("BME680 calibration block reads as all 0xFF"),
        }
    }
}

/// Per-unit factory calibration of one BME680.
///
/// Opaque: built only by [`from_raw`](Self::from_raw) and consumed by
/// [`compensate`] and [`res_heat`]. The fields are the Bosch `par_*`
/// coefficients in their register types; they are converted to `f32` at use,
/// as the Bosch SensorAPI does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalibrationData {
    t1: u16,
    t2: i16,
    t3: i8,
    p1: u16,
    p2: i16,
    p3: i8,
    p4: i16,
    p5: i16,
    p6: i8,
    p7: i8,
    p8: i16,
    p9: i16,
    p10: u8,
    h1: u16,
    h2: u16,
    h3: i8,
    h4: i8,
    h5: i8,
    h6: u8,
    h7: i8,
    gh1: i8,
    gh2: i16,
    gh3: i8,
    res_heat_range: u8,
    res_heat_val: i8,
    range_sw_err: i8,
}

const fn le_u16(lsb: u8, msb: u8) -> u16 {
    u16::from_le_bytes([lsb, msb])
}

const fn le_i16(lsb: u8, msb: u8) -> i16 {
    i16::from_le_bytes([lsb, msb])
}

const fn all_equal(bytes: &[u8], value: u8) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != value {
            return false;
        }
        i += 1;
    }
    true
}

impl CalibrationData {
    /// Parses the factory calibration (Bosch v4.4.8 `get_calib_data`).
    ///
    /// # Arguments
    ///
    /// * `coeff1` — 25 bytes read from [`REG_COEFF1`] (`0x89..=0xA1`)
    /// * `coeff2` — 16 bytes read from [`REG_COEFF2`] (`0xE1..=0xF0`)
    /// * `res_heat_range` — [`REG_RES_HEAT_RANGE`] decoded with
    ///   [`res_heat_range_from_reg`]
    /// * `res_heat_val` — [`REG_RES_HEAT_VAL`] reinterpreted as `i8`
    /// * `range_sw_err` — [`REG_RANGE_SW_ERR`] decoded with
    ///   [`range_sw_err_from_reg`]
    ///
    /// The humidity coefficients share register `0xE2`: its low nibble is the
    /// low part of `par_h1`, its high nibble the low part of `par_h2`, so
    /// `par_h1 = (0xE3 << 4) | (0xE2 & 0x0F)` and
    /// `par_h2 = (0xE1 << 4) | (0xE2 >> 4)`.
    ///
    /// # Errors
    ///
    /// Returns [`CalibrationError::AllZeros`] or [`CalibrationError::AllOnes`]
    /// if either block is uniformly `0x00` or `0xFF` (`coeff1` is checked
    /// first).
    pub const fn from_raw(
        coeff1: &[u8; 25],
        coeff2: &[u8; 16],
        res_heat_range: u8,
        res_heat_val: i8,
        range_sw_err: i8,
    ) -> Result<Self, CalibrationError> {
        if all_equal(coeff1, 0x00) || all_equal(coeff2, 0x00) {
            return Err(CalibrationError::AllZeros);
        }
        if all_equal(coeff1, 0xFF) || all_equal(coeff2, 0xFF) {
            return Err(CalibrationError::AllOnes);
        }
        let b1 = coeff1;
        let b2 = coeff2;
        Ok(Self {
            // Block 1: offset = register - 0x89.
            t2: le_i16(b1[1], b1[2]),
            t3: b1[3] as i8,
            p1: le_u16(b1[5], b1[6]),
            p2: le_i16(b1[7], b1[8]),
            p3: b1[9] as i8,
            p4: le_i16(b1[11], b1[12]),
            p5: le_i16(b1[13], b1[14]),
            p7: b1[15] as i8,
            p6: b1[16] as i8,
            p8: le_i16(b1[19], b1[20]),
            p9: le_i16(b1[21], b1[22]),
            p10: b1[23],
            // Block 2: offset = register - 0xE1.
            h1: ((b2[2] as u16) << 4) | (b2[1] & 0x0F) as u16,
            h2: ((b2[0] as u16) << 4) | (b2[1] >> 4) as u16,
            h3: b2[3] as i8,
            h4: b2[4] as i8,
            h5: b2[5] as i8,
            h6: b2[6],
            h7: b2[7] as i8,
            t1: le_u16(b2[8], b2[9]),
            gh2: le_i16(b2[10], b2[11]),
            gh1: b2[12] as i8,
            gh3: b2[13] as i8,
            res_heat_range,
            res_heat_val,
            range_sw_err,
        })
    }
}

// ─── RawReading ──────────────────────────────────────────────────────────────

/// Raw ADC values and gas flags from one 15-byte BME680 data block.
///
/// Obtained only through [`parse_raw`]; fields are private, mirroring
/// [`crate::mpu6050::RawReading`], so fields can be added in a later minor
/// version without breaking callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawReading {
    adc_pressure: u32,
    adc_temperature: u32,
    adc_humidity: u16,
    adc_gas: u16,
    gas_range: u8,
    gas_valid: bool,
    heat_stab: bool,
}

impl RawReading {
    /// Returns the 20-bit pressure ADC value.
    #[must_use]
    pub const fn adc_pressure(self) -> u32 {
        self.adc_pressure
    }

    /// Returns the 20-bit temperature ADC value.
    #[must_use]
    pub const fn adc_temperature(self) -> u32 {
        self.adc_temperature
    }

    /// Returns the 16-bit humidity ADC value.
    #[must_use]
    pub const fn adc_humidity(self) -> u16 {
        self.adc_humidity
    }

    /// Returns the 10-bit gas-resistance ADC value.
    #[must_use]
    pub const fn adc_gas(self) -> u16 {
        self.adc_gas
    }

    /// Returns the 4-bit gas range (`0..=15`).
    #[must_use]
    pub const fn gas_range(self) -> u8 {
        self.gas_range
    }

    /// Returns `gas_valid_r`: the gas conversion produced a valid result.
    #[must_use]
    pub const fn gas_valid(self) -> bool {
        self.gas_valid
    }

    /// Returns `heat_stab_r`: the heater reached its target temperature.
    #[must_use]
    pub const fn heat_stab(self) -> bool {
        self.heat_stab
    }
}

/// Parses the 15-byte data block read from [`REG_MEAS_STATUS_0`] into a
/// [`RawReading`] (Bosch v4.4.8 `read_field_data`, low gas variant).
///
/// Byte layout:
/// - `[0]` — `meas_status_0` (see [`is_measurement_ready`]; not stored)
/// - `[2..=4]` — pressure, `b2 << 12 | b3 << 4 | b4 >> 4`
/// - `[5..=7]` — temperature, `b5 << 12 | b6 << 4 | b7 >> 4`
/// - `[8..=9]` — humidity, `b8 << 8 | b9`
/// - `[13..=14]` — gas, `b13 << 2 | b14 >> 6`; `b14` bit 5 `gas_valid_r`,
///   bit 4 `heat_stab_r`, bits 3:0 gas range
///
/// Takes a fixed-size `[u8; 15]`, so parsing is total and never panics.
/// The `0x80000` / `0x8000` markers of a skipped channel are not detected
/// (relevant only to callers with their own transport; the driver never
/// skips a channel).
#[must_use]
pub const fn parse_raw(buf: &[u8; 15]) -> RawReading {
    RawReading {
        adc_pressure: ((buf[2] as u32) << 12) | ((buf[3] as u32) << 4) | ((buf[4] as u32) >> 4),
        adc_temperature: ((buf[5] as u32) << 12) | ((buf[6] as u32) << 4) | ((buf[7] as u32) >> 4),
        adc_humidity: ((buf[8] as u16) << 8) | buf[9] as u16,
        adc_gas: ((buf[13] as u16) << 2) | (buf[14] >> 6) as u16,
        gas_range: buf[14] & GAS_RANGE_MASK,
        gas_valid: buf[14] & GAS_VALID != 0,
        heat_stab: buf[14] & HEAT_STAB != 0,
    }
}

// ─── Compensation ────────────────────────────────────────────────────────────

/// A compensated BME680 measurement.
///
/// Obtained only through [`compensate`]; values are read through accessors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    temperature_c: f32,
    pressure_hpa: f32,
    humidity_rh: f32,
    gas_resistance_ohm: Option<f32>,
}

impl Reading {
    /// Returns the temperature in °C.
    #[must_use]
    pub const fn temperature_c(&self) -> f32 {
        self.temperature_c
    }

    /// Returns the pressure in hPa.
    ///
    /// `0.0` if the calibration makes the Bosch pressure divisor truncate to
    /// zero (Bosch returns `0` there too, avoiding a division by zero).
    #[must_use]
    pub const fn pressure_hpa(&self) -> f32 {
        self.pressure_hpa
    }

    /// Returns the relative humidity in %RH, clamped to `0.0..=100.0`.
    #[must_use]
    pub const fn humidity_rh(&self) -> f32 {
        self.humidity_rh
    }

    /// Returns the gas resistance in ohms.
    ///
    /// `None` when `gas_valid_r` or `heat_stab_r` is clear in the data block
    /// (including when no gas conversion was requested).
    #[must_use]
    pub const fn gas_resistance_ohm(&self) -> Option<f32> {
        self.gas_resistance_ohm
    }
}

/// Bosch `lookup_k1_range`, in percent.
const GAS_K1: [f32; 16] = [
    0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, -0.8, 0.0, 0.0, -0.2, -0.5, 0.0, -1.0, 0.0, 0.0,
];

/// Bosch `lookup_k2_range`, in percent.
const GAS_K2: [f32; 16] = [
    0.0, 0.0, 0.0, 0.0, 0.1, 0.7, 0.0, -0.8, -0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
];

/// Bosch `calc_temperature`: returns `t_fine` (untruncated).
fn calc_t_fine(c: &CalibrationData, adc: u32) -> f32 {
    let adc = adc as f32;
    let t1 = f32::from(c.t1);
    let var1 = ((adc / 16384.0) - (t1 / 1024.0)) * f32::from(c.t2);
    let var2 = ((adc / 131072.0) - (t1 / 8192.0))
        * ((adc / 131072.0) - (t1 / 8192.0))
        * (f32::from(c.t3) * 16.0);
    var1 + var2
}

/// Bosch `calc_pressure`: returns pascals, or `0.0` when the divisor
/// truncates to zero.
fn calc_pressure_pa(c: &CalibrationData, adc: u32, t_fine: f32) -> f32 {
    let mut var1 = (t_fine / 2.0) - 64000.0;
    let mut var2 = var1 * var1 * (f32::from(c.p6) / 131072.0);
    var2 += var1 * f32::from(c.p5) * 2.0;
    var2 = (var2 / 4.0) + (f32::from(c.p4) * 65536.0);
    var1 = ((f32::from(c.p3) * var1 * var1) / 16384.0 + (f32::from(c.p2) * var1)) / 524288.0;
    var1 = (1.0 + (var1 / 32768.0)) * f32::from(c.p1);
    let mut calc_pres = 1048576.0 - adc as f32;

    // Bosch: `if ((int)var1 != 0)`; `as` truncates toward zero like the C
    // cast (and maps NaN to 0, where C is undefined).
    if var1 as i32 == 0 {
        return 0.0;
    }
    calc_pres = ((calc_pres - (var2 / 4096.0)) * 6250.0) / var1;
    var1 = (f32::from(c.p9) * calc_pres * calc_pres) / 2147483648.0;
    var2 = calc_pres * (f32::from(c.p8) / 32768.0);
    let var3 = (calc_pres / 256.0)
        * (calc_pres / 256.0)
        * (calc_pres / 256.0)
        * (f32::from(c.p10) / 131072.0);
    calc_pres + (var1 + var2 + var3 + (f32::from(c.p7) * 128.0)) / 16.0
}

/// Bosch `calc_humidity`: returns %RH clamped to `0.0..=100.0`.
fn calc_humidity(c: &CalibrationData, adc: u16, t_fine: f32) -> f32 {
    let temp_comp = t_fine / 5120.0;
    let var1 = f32::from(adc) - ((f32::from(c.h1) * 16.0) + ((f32::from(c.h3) / 2.0) * temp_comp));
    let var2 = var1
        * ((f32::from(c.h2) / 262144.0)
            * (1.0
                + ((f32::from(c.h4) / 16384.0) * temp_comp)
                + ((f32::from(c.h5) / 1048576.0) * temp_comp * temp_comp)));
    let var3 = f32::from(c.h6) / 16384.0;
    let var4 = f32::from(c.h7) / 2097152.0;
    let calc_hum = var2 + ((var3 + (var4 * temp_comp)) * var2 * var2);
    // Bosch clamps with two branches; `clamp` is identical for non-NaN
    // values, and NaN cannot occur: every input is a finite integer.
    calc_hum.clamp(0.0, 100.0)
}

/// Bosch `calc_gas_resistance_low`: returns ohms, or `None` if the result is
/// not finite (unreachable for in-range inputs; a no-NaN guard).
fn calc_gas_resistance_ohm(c: &CalibrationData, adc: u16, range: u8) -> Option<f32> {
    let range = usize::from(range & GAS_RANGE_MASK);
    let gas_res_f = f32::from(adc);
    let gas_range_f = (1u32 << range) as f32;
    let var1 = 1340.0 + (5.0 * f32::from(c.range_sw_err));
    let var2 = var1 * (1.0 + GAS_K1[range] / 100.0);
    let var3 = 1.0 + (GAS_K2[range] / 100.0);
    let r = 1.0 / (var3 * 0.000000125 * gas_range_f * (((gas_res_f - 512.0) / var2) + 1.0));
    if r.is_finite() {
        Some(r)
    } else {
        None
    }
}

/// Compensates a [`RawReading`] into physical units (Bosch v4.4.8 float
/// path: `calc_temperature`, `calc_pressure`, `calc_humidity`,
/// `calc_gas_resistance_low`).
///
/// Gas resistance is `None` unless both `gas_valid_r` and `heat_stab_r` are
/// set.
/// Marker values from a skipped channel are not detected (relevant only to
/// callers with their own transport; the driver never skips a channel).
/// Never panics and, for any `RawReading` and any `CalibrationData`
/// accepted by [`CalibrationData::from_raw`], returns finite values: the
/// pressure division is guarded exactly as in Bosch, humidity is clamped,
/// and a non-finite gas result maps to `None`.
#[must_use]
pub fn compensate(calib: &CalibrationData, raw: &RawReading) -> Reading {
    let t_fine = calc_t_fine(calib, raw.adc_temperature);
    let gas_resistance_ohm = if raw.gas_valid && raw.heat_stab {
        calc_gas_resistance_ohm(calib, raw.adc_gas, raw.gas_range)
    } else {
        None
    };
    Reading {
        temperature_c: t_fine / 5120.0,
        pressure_hpa: calc_pressure_pa(calib, raw.adc_pressure, t_fine) / 100.0,
        humidity_rh: calc_humidity(calib, raw.adc_humidity, t_fine),
        gas_resistance_ohm,
    }
}

// ─── Heater and timing ───────────────────────────────────────────────────────

/// Computes the [`REG_RES_HEAT_0`] byte for a heater target temperature
/// (Bosch v4.4.8 float `calc_res_heat`).
///
/// * `target_c` — heater target in °C, capped at 400 °C (and floored at 0)
/// * `ambient_c` — ambient temperature in °C, e.g. the last measured value
///
/// Bosch takes whole degrees (`uint16_t temp`, `int8_t amb_temp`); passing
/// whole-degree values reproduces its result exactly. The final float-to-`u8`
/// conversion saturates (`as`), where the C cast of an out-of-range value is
/// undefined; a NaN input yields `0`.
#[must_use]
pub fn res_heat(calib: &CalibrationData, target_c: f32, ambient_c: f32) -> u8 {
    let temp = target_c.clamp(0.0, 400.0);
    let var1 = (f32::from(calib.gh1) / 16.0) + 49.0;
    let var2 = ((f32::from(calib.gh2) / 32768.0) * 0.0005) + 0.00235;
    let var3 = f32::from(calib.gh3) / 1024.0;
    let var4 = var1 * (1.0 + (var2 * temp));
    let var5 = var4 + (var3 * ambient_c);
    (3.4 * ((var5
        * (4.0 / (4.0 + f32::from(calib.res_heat_range)))
        * (1.0 / (1.0 + (f32::from(calib.res_heat_val) * 0.002))))
        - 25.0)) as u8
}

/// Returns the forced-mode TPH measurement duration in microseconds (Bosch
/// v4.4.8 `bme68x_get_meas_dur`, non-parallel mode).
///
/// `cycles(t) + cycles(p) + cycles(h)` times 1963 µs, plus 477 µs x 4 for
/// TPH switching, 477 µs x 5 for the gas measurement, and 1000 µs wake-up.
/// The heater duration ([`HEATER_DURATION_MS`]) is not included.
///
/// ```
/// use tamer::bme680::{measurement_duration_us, Oversampling};
///
/// let us = measurement_duration_us(Oversampling::X2, Oversampling::X16, Oversampling::X1);
/// assert_eq!(us, 42_590);
/// ```
#[must_use]
pub const fn measurement_duration_us(
    osrs_t: Oversampling,
    osrs_p: Oversampling,
    osrs_h: Oversampling,
) -> u32 {
    let cycles = osrs_t.cycles() + osrs_p.cycles() + osrs_h.cycles();
    cycles * 1963 + 477 * 4 + 477 * 5 + 1000
}

// ─── Driver ──────────────────────────────────────────────────────────────────

#[cfg(feature = "hal")]
use embedded_hal::delay::DelayNs;
#[cfg(feature = "hal")]
use embedded_hal::i2c::{ErrorKind, I2c};

/// Error returned by the [`Bme680`] driver.
///
/// `E` is the HAL's own I2C error type, kept intact in [`Bus`](Self::Bus).
/// There is deliberately no `From<E>` impl: map bus errors explicitly with
/// `Error::Bus` to keep `?` conversions unambiguous.
#[cfg(feature = "hal")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    /// The I2C bus reported an error.
    Bus(E),
    /// The chip-ID register returned an unexpected value.
    WrongChipId {
        /// The value read from [`REG_CHIP_ID`].
        found: u8,
    },
    /// The sensor is a variant this driver does not implement.
    ///
    /// Chip ID `0x61` is shared by the BME680 and the BME688; only the
    /// BME680 low-gas path is transcribed.
    UnsupportedVariant {
        /// The value read from [`REG_VARIANT_ID`].
        found: u8,
    },
    /// The calibration blocks read from the device are degenerate.
    Calibration(CalibrationError),
    /// The sensor did not finish within the `timeout_ms` budget.
    Timeout,
    /// `timeout_ms` is shorter than the conversion needs; no bus traffic occurred.
    BudgetTooShort {
        /// The minimum `timeout_ms` for the requested settings, in milliseconds.
        required_ms: u32,
    },
}

#[cfg(feature = "hal")]
impl<E: embedded_hal::i2c::Error> Error<E> {
    /// Returns `true` if this is a bus error of kind
    /// [`ErrorKind::NoAcknowledge`], i.e. no device answered.
    #[must_use]
    pub fn is_nack(&self) -> bool {
        match self {
            Self::Bus(e) => matches!(e.kind(), ErrorKind::NoAcknowledge(_)),
            _ => false,
        }
    }
}

#[cfg(feature = "hal")]
impl<E: core::fmt::Debug> core::fmt::Display for Error<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Bus(e) => write!(f, "I2C bus error: {e:?}"),
            Self::WrongChipId { found } => write!(
                f,
                "unexpected chip ID {found:#04x}: expected {CHIP_ID_VALUE:#04x} \
                 — a different device answers at this address"
            ),
            Self::UnsupportedVariant { found } => write!(
                f,
                "unsupported sensor variant {found:#04x} (BME688?): \
                 only the BME680 low-gas path is implemented"
            ),
            Self::Calibration(e) => write!(f, "invalid calibration data: {e}"),
            Self::Timeout => write!(f, "measurement did not complete within the budget"),
            Self::BudgetTooShort { required_ms } => {
                write!(
                    f,
                    "timeout budget too short: need at least {required_ms} ms"
                )
            }
        }
    }
}

/// Blocking BME680 driver over an `embedded_hal` I2C bus (features `bme680` and `hal`).
///
/// The driver owns the bus; hand it back with [`release`](Self::release).
/// All protocol knowledge comes from this module's constants and pure
/// functions, so the esp-hal and esp-idf tiers share it unchanged.
///
/// ```
/// use tamer::bme680::{self, Bme680};
/// use tamer::mock::{MockI2c, NoopDelay, Transaction};
///
/// let a = bme680::I2C_ADDR;
/// let coeff1 = [0x11u8; 25];
/// let mut coeff2 = [0x22u8; 16];
/// coeff2[15] = 0x00; // variant register: BME680
/// let script = [
///     Transaction::Write { address: a, bytes: &[0xE0, 0xB6] },
///     Transaction::WriteRead { address: a, write: &[0xD0], response: &[0x61] },
///     Transaction::WriteRead { address: a, write: &[0x89], response: &coeff1 },
///     Transaction::WriteRead { address: a, write: &[0xE1], response: &coeff2 },
///     Transaction::WriteRead { address: a, write: &[0x02], response: &[0x10] },
///     Transaction::WriteRead { address: a, write: &[0x00], response: &[0x00] },
///     Transaction::WriteRead { address: a, write: &[0x04], response: &[0x00] },
/// ];
/// let sensor = Bme680::new(MockI2c::new(&script), a, &mut NoopDelay).unwrap();
/// sensor.release().done();
/// ```
#[cfg(feature = "hal")]
#[derive(Debug)]
pub struct Bme680<I2C> {
    i2c: I2C,
    address: u8,
    calib: CalibrationData,
    ambient_c: f32,
}

#[cfg(feature = "hal")]
impl<I2C: I2c> Bme680<I2C> {
    /// Soft-resets the device at `address`, verifies its identity, and reads calibration.
    ///
    /// Pass [`I2C_ADDR`] or [`I2C_ADDR_ALT`]; the caller keeps VDD / VDDIO
    /// stable for the 2 ms datasheet start-up time first.
    /// The bus sequence is:
    ///
    /// 1. write `[REG_SOFT_RESET, SOFT_RESET_CMD]`, then wait [`RESET_DELAY_MS`];
    /// 2. read [`REG_CHIP_ID`] and compare with [`CHIP_ID_VALUE`];
    /// 3. read [`COEFF1_LEN`] bytes at [`REG_COEFF1`] and [`COEFF2_LEN`] bytes at [`REG_COEFF2`];
    ///    the last byte of the second block (`0xF0`) is [`REG_VARIANT_ID`], so the
    ///    variant check needs no extra bus transaction;
    /// 4. read one byte each at [`REG_RES_HEAT_RANGE`], [`REG_RES_HEAT_VAL`] and [`REG_RANGE_SW_ERR`].
    ///
    /// # Errors
    ///
    /// On any failure the bus is handed back next to the error, so the caller
    /// can retry or reuse it: [`Error::Bus`] if a transfer fails (a missing
    /// device surfaces as a NACK, see [`Error::is_nack`]),
    /// [`Error::WrongChipId`] if the ID is not [`CHIP_ID_VALUE`],
    /// [`Error::UnsupportedVariant`] if [`REG_VARIANT_ID`] is not
    /// [`VARIANT_ID_BME680`] (a BME688 shares the chip ID), or
    /// [`Error::Calibration`] if a calibration block is all `0x00` / `0xFF`.
    pub fn new<D: DelayNs>(
        mut i2c: I2C,
        address: u8,
        delay: &mut D,
    ) -> Result<Self, (Error<I2C::Error>, I2C)> {
        match Self::init(&mut i2c, address, delay) {
            Ok(calib) => Ok(Self {
                i2c,
                address,
                calib,
                ambient_c: 25.0,
            }),
            Err(e) => Err((e, i2c)),
        }
    }

    fn init<D: DelayNs>(
        i2c: &mut I2C,
        address: u8,
        delay: &mut D,
    ) -> Result<CalibrationData, Error<I2C::Error>> {
        i2c.write(address, &[REG_SOFT_RESET, SOFT_RESET_CMD])
            .map_err(Error::Bus)?;
        delay.delay_ms(RESET_DELAY_MS);
        let mut id = [0u8; 1];
        i2c.write_read(address, &[REG_CHIP_ID], &mut id)
            .map_err(Error::Bus)?;
        if id[0] != CHIP_ID_VALUE {
            return Err(Error::WrongChipId { found: id[0] });
        }
        let mut coeff1 = [0u8; COEFF1_LEN];
        i2c.write_read(address, &[REG_COEFF1], &mut coeff1)
            .map_err(Error::Bus)?;
        let mut coeff2 = [0u8; COEFF2_LEN];
        i2c.write_read(address, &[REG_COEFF2], &mut coeff2)
            .map_err(Error::Bus)?;
        // coeff2 spans 0xE1..=0xF0, so its last byte is REG_VARIANT_ID.
        let variant = coeff2[COEFF2_LEN - 1];
        if variant != VARIANT_ID_BME680 {
            return Err(Error::UnsupportedVariant { found: variant });
        }
        let mut range = [0u8; 1];
        i2c.write_read(address, &[REG_RES_HEAT_RANGE], &mut range)
            .map_err(Error::Bus)?;
        let mut val = [0u8; 1];
        i2c.write_read(address, &[REG_RES_HEAT_VAL], &mut val)
            .map_err(Error::Bus)?;
        let mut sw_err = [0u8; 1];
        i2c.write_read(address, &[REG_RANGE_SW_ERR], &mut sw_err)
            .map_err(Error::Bus)?;
        CalibrationData::from_raw(
            &coeff1,
            &coeff2,
            res_heat_range_from_reg(range[0]),
            val[0] as i8,
            range_sw_err_from_reg(sw_err[0]),
        )
        .map_err(Error::Calibration)
    }

    /// Runs one forced-mode conversion and returns the compensated reading.
    ///
    /// `timeout_ms` budgets only the driver's own [`DelayNs`] waits (the
    /// conversion time plus 1 ms poll steps), not wall-clock or bus time.
    /// The conversion time is
    /// `required_ms = (measurement_duration_us + 150_000 if read_gas).div_ceil(1000)`.
    /// With `read_gas` the fixed heater profile ([`HEATER_TARGET_C`] for
    /// [`HEATER_DURATION_MS`]) runs, with the last measured temperature (25 °C
    /// before the first) as the ambient input.
    ///
    /// The register sequence is, after the budget check:
    ///
    /// 0. sleep-mode recovery: read [`REG_CTRL_MEAS`]; if the mode bits are not
    ///    `00`, write the same byte with them cleared ([`with_sleep_mode`]) and
    ///    read [`REG_CTRL_MEAS`] every 1 ms (consuming the same remaining
    ///    budget as the data poll) until the mode reads `00`;
    /// 1. write `[REG_CONFIG, config_byte(filter)]` and `[REG_CTRL_HUM, ctrl_hum_byte(osrs_h)]`;
    /// 2. with `read_gas`: write `[REG_RES_HEAT_0, res_heat]`, `[REG_GAS_WAIT_0, gas_wait(150)]`,
    ///    `[REG_CTRL_GAS_0, 0x00]`, `[REG_CTRL_GAS_1, 0x10]`;
    ///    without: write `[REG_CTRL_GAS_0, 0x08]`, `[REG_CTRL_GAS_1, 0x00]`;
    /// 3. drain: read [`DATA_LEN`] bytes at [`REG_MEAS_STATUS_0`] and discard them;
    /// 4. trigger: write `[REG_CTRL_MEAS, ctrl_meas_byte(osrs_t, osrs_p)]`;
    /// 5. wait `required_ms`, then read one status byte at [`REG_MEAS_STATUS_0`]
    ///    every 1 ms until [`is_new_data`] is set and `measuring` /
    ///    `gas_measuring` are clear (`gas_measuring` is ignored without `read_gas`);
    /// 6. read [`DATA_LEN`] bytes at [`REG_MEAS_STATUS_0`].
    ///
    /// Step 0 mirrors Bosch `bme68x_set_conf`, which forces sleep first: a
    /// previous [`Error::Timeout`] can leave the sensor mid-conversion, and
    /// config registers must only be written in sleep mode.
    /// The drain (step 3) discards a stale sample whether `new_data` clears
    /// on read or on trigger.
    /// Gas resistance is `None` unless `read_gas` is set, `gas_valid_r` is set
    /// and `heat_stab_r` is set.
    ///
    /// # Errors
    ///
    /// [`Error::BudgetTooShort`] if `timeout_ms < required_ms` (checked before
    /// any bus write), [`Error::Timeout`] if the sensor is not ready when the
    /// remaining budget (`timeout_ms - required_ms`) is spent (while waiting
    /// for sleep mode or for the data), or
    /// [`Error::Bus`] if a transfer fails.
    #[allow(clippy::too_many_arguments)] // mirrors the Measurement contract in the feature doc
    pub fn measure<D: DelayNs>(
        &mut self,
        delay: &mut D,
        osrs_t: Oversampling,
        osrs_p: Oversampling,
        osrs_h: Oversampling,
        filter: IirFilter,
        read_gas: bool,
        timeout_ms: u32,
    ) -> Result<Reading, Error<I2C::Error>> {
        let gas_us = if read_gas {
            // 150 ms heater plateau; at most 249_517 us in total, no overflow.
            u32::from(HEATER_DURATION_MS) * 1000
        } else {
            0
        };
        let required_ms = (measurement_duration_us(osrs_t, osrs_p, osrs_h) + gas_us).div_ceil(1000);
        if timeout_ms < required_ms {
            return Err(Error::BudgetTooShort { required_ms });
        }
        let mut remaining = timeout_ms - required_ms;

        let mut ctrl = [0u8; 1];
        self.i2c
            .write_read(self.address, &[REG_CTRL_MEAS], &mut ctrl)
            .map_err(Error::Bus)?;
        if op_mode(ctrl[0]) != 0 {
            self.write_reg(REG_CTRL_MEAS, with_sleep_mode(ctrl[0]))?;
            loop {
                self.i2c
                    .write_read(self.address, &[REG_CTRL_MEAS], &mut ctrl)
                    .map_err(Error::Bus)?;
                if op_mode(ctrl[0]) == 0 {
                    break;
                }
                if remaining == 0 {
                    return Err(Error::Timeout);
                }
                delay.delay_ms(1);
                remaining = remaining.saturating_sub(1);
            }
        }

        self.write_reg(REG_CONFIG, config_byte(filter))?;
        self.write_reg(REG_CTRL_HUM, ctrl_hum_byte(osrs_h))?;
        if read_gas {
            let rh = res_heat(&self.calib, HEATER_TARGET_C, self.ambient_c);
            self.write_reg(REG_RES_HEAT_0, rh)?;
            self.write_reg(REG_GAS_WAIT_0, gas_wait(HEATER_DURATION_MS))?;
        }
        // PENDING(hardware): Bosch sets the heater bit by read-modify-write of
        // ctrl_gas_0 bit 3; this plain write assumes the other bits are
        // reserved/zero (datasheet-derived, not in the feature-doc contract).
        self.write_reg(REG_CTRL_GAS_0, ctrl_gas_0_byte(read_gas))?;
        self.write_reg(REG_CTRL_GAS_1, ctrl_gas_1_byte(read_gas))?;
        // PENDING(hardware): confirm new_data clears on read vs on trigger (feature doc open question)
        self.read_data()?;
        self.write_reg(REG_CTRL_MEAS, ctrl_meas_byte(osrs_t, osrs_p))?;

        delay.delay_ms(required_ms);
        loop {
            let mut status = [0u8; 1];
            self.i2c
                .write_read(self.address, &[REG_MEAS_STATUS_0], &mut status)
                .map_err(Error::Bus)?;
            let s = status[0];
            let gas_done = !read_gas || !is_gas_measuring(s);
            if is_new_data(s) && !is_measuring(s) && gas_done {
                break;
            }
            if remaining == 0 {
                return Err(Error::Timeout);
            }
            delay.delay_ms(1);
            remaining = remaining.saturating_sub(1);
        }

        let raw = parse_raw(&self.read_data()?);
        let mut reading = compensate(&self.calib, &raw);
        if !read_gas {
            reading.gas_resistance_ohm = None;
        }
        self.ambient_c = reading.temperature_c;
        Ok(reading)
    }

    fn write_reg(&mut self, reg: u8, value: u8) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[reg, value])
            .map_err(Error::Bus)
    }

    fn read_data(&mut self) -> Result<[u8; DATA_LEN], Error<I2C::Error>> {
        let mut buf = [0u8; DATA_LEN];
        self.i2c
            .write_read(self.address, &[REG_MEAS_STATUS_0], &mut buf)
            .map_err(Error::Bus)?;
        Ok(buf)
    }

    /// Consumes the driver and returns the I2C bus.
    #[must_use]
    pub fn release(self) -> I2C {
        self.i2c
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Reference fixture ───────────────────────────────────────────────
    //
    // Provenance: SYNTHETIC calibration, not a device dump. The calibration
    // bytes describe a plausible unit; the raw ADC values were solved so the
    // unit reads about 25 °C / 1000 hPa / 50 %RH / 50 kΩ. The expected
    // values come from an independent transcription of the Bosch BME68x
    // SensorAPI v4.4.8 float path (tag v4.4.8, `bme68x.c`, `BME68X_USE_FPU`
    // branch, BME680 low gas variant) with every operation rounded to IEEE
    // binary32. Units: °C, hPa, %RH, Ω.
    // TODO(hardware): replace with a real device dump.

    const COEFF1: [u8; 25] = [
        0x00, 0x66, 0x66, 0x03, 0x00, 0x72, 0x8D, 0x2E, 0xD7, 0x58, 0x00, 0xD4, 0x1C, 0x79, 0xFF,
        0x28, 0x1E, 0x00, 0x00, 0xF6, 0xF5, 0xEA, 0xF7, 0x1E, 0x00,
    ];
    const COEFF2: [u8; 16] = [
        0x3F, 0x2D, 0x32, 0x00, 0x2D, 0x14, 0x78, 0x9C, 0x9D, 0x65, 0x20, 0xEA, 0xE2, 0x12, 0x00,
        0x00,
    ];
    const RES_HEAT_RANGE_REG: u8 = 0x10;
    const RES_HEAT_VAL_REG: u8 = 0xF4;
    const RANGE_SW_ERR_REG: u8 = 0xE0;
    const DATA: [u8; 15] = [
        0x80, 0x00, 0x55, 0xD6, 0xB0, 0x79, 0x24, 0x60, 0x58, 0x76, 0x00, 0x00, 0x00, 0xD5, 0xF7,
    ];

    fn reference_calib() -> CalibrationData {
        CalibrationData::from_raw(
            &COEFF1,
            &COEFF2,
            res_heat_range_from_reg(RES_HEAT_RANGE_REG),
            RES_HEAT_VAL_REG as i8,
            range_sw_err_from_reg(RANGE_SW_ERR_REG),
        )
        .unwrap()
    }

    fn assert_close(actual: f32, expected: f32, tol: f32) {
        assert!(
            (actual - expected).abs() <= tol,
            "{actual} not within {tol} of {expected}"
        );
    }

    fn assert_rel(actual: f32, expected: f32, rel: f32) {
        assert!(
            ((actual - expected) / expected).abs() <= rel,
            "{actual} not within {rel} relative of {expected}"
        );
    }

    /// Builds a data block from raw ADC values and the gas flag byte.
    fn block(adc_p: u32, adc_t: u32, adc_h: u16, adc_gas: u16, gas_lsb_flags: u8) -> [u8; 15] {
        let mut b = [0u8; 15];
        b[0] = 0x80;
        b[2] = (adc_p >> 12) as u8;
        b[3] = (adc_p >> 4) as u8;
        b[4] = ((adc_p & 0x0F) << 4) as u8;
        b[5] = (adc_t >> 12) as u8;
        b[6] = (adc_t >> 4) as u8;
        b[7] = ((adc_t & 0x0F) << 4) as u8;
        b[8] = (adc_h >> 8) as u8;
        b[9] = adc_h as u8;
        b[13] = (adc_gas >> 2) as u8;
        b[14] = ((adc_gas & 0x03) << 6) as u8 | gas_lsb_flags;
        b
    }

    #[test]
    fn reference_calibration_fields() {
        let c = reference_calib();
        assert_eq!(
            c,
            CalibrationData {
                t1: 26013,
                t2: 26214,
                t3: 3,
                p1: 36210,
                p2: -10450,
                p3: 88,
                p4: 7380,
                p5: -135,
                p6: 30,
                p7: 40,
                p8: -2570,
                p9: -2070,
                p10: 30,
                h1: 813,
                h2: 1010,
                h3: 0,
                h4: 45,
                h5: 20,
                h6: 120,
                h7: -100,
                gh1: -30,
                gh2: -5600,
                gh3: 18,
                res_heat_range: 1,
                res_heat_val: -12,
                range_sw_err: -2,
            }
        );
    }

    #[test]
    fn reference_raw_decode() {
        let r = parse_raw(&DATA);
        assert_eq!(r.adc_temperature(), 496_198);
        assert_eq!(r.adc_pressure(), 351_595);
        assert_eq!(r.adc_humidity(), 22_646);
        assert_eq!(r.adc_gas(), 855);
        assert_eq!(r.gas_range(), 7);
        assert!(r.gas_valid());
        assert!(r.heat_stab());
        assert!(is_new_data(DATA[0]));
    }

    #[test]
    fn reference_t_fine() {
        let c = reference_calib();
        assert_close(calc_t_fine(&c, 496_198), 127_999.92, 0.01);
    }

    #[test]
    fn reference_compensation_matches_bosch_float_path() {
        let reading = compensate(&reference_calib(), &parse_raw(&DATA));
        assert_close(reading.temperature_c(), 24.999985, 0.01);
        assert_close(reading.pressure_hpa(), 1000.00104, 0.01);
        assert_close(reading.humidity_rh(), 49.99954, 0.01);
        assert_rel(reading.gas_resistance_ohm().unwrap(), 50004.21, 0.001);
    }

    #[test]
    fn reference_compensation_is_tight() {
        // Much tighter than the feature-doc tolerances: a wrong algorithm
        // family (e.g. the integer path in f32) fails here.
        let reading = compensate(&reference_calib(), &parse_raw(&DATA));
        assert_close(reading.temperature_c(), 24.999985, 0.0005);
        assert_close(reading.pressure_hpa(), 1000.00104, 0.0005);
        assert_close(reading.humidity_rh(), 49.99954, 0.0005);
        assert_rel(reading.gas_resistance_ohm().unwrap(), 50004.21, 0.00001);
    }

    #[test]
    fn reference_res_heat_exact() {
        let c = reference_calib();
        assert_eq!(res_heat(&c, 320.0, 25.0), 142);
        assert_eq!(res_heat(&c, HEATER_TARGET_C, 25.0), 0x8E);
        assert_eq!(res_heat(&c, 320.0, 0.0), 141);
        assert_eq!(res_heat(&c, 320.0, -10.0), 141);
        assert_eq!(res_heat(&c, 320.0, 40.0), 143);
        assert_eq!(res_heat(&c, 200.0, 25.0), 107);
    }

    #[test]
    fn res_heat_caps_target_at_400() {
        let c = reference_calib();
        assert_eq!(res_heat(&c, 500.0, 25.0), 166);
        assert_eq!(res_heat(&c, 500.0, 25.0), res_heat(&c, 400.0, 25.0));
    }

    #[test]
    fn res_heat_nan_is_zero_not_panic() {
        let c = reference_calib();
        assert_eq!(res_heat(&c, 320.0, f32::NAN), 0);
    }

    #[test]
    fn measurement_duration_matches_bosch() {
        use Oversampling::*;
        assert_eq!(measurement_duration_us(X2, X16, X1), 42_590);
        assert_eq!(measurement_duration_us(X1, X1, X1), 11_182);
        assert_eq!(measurement_duration_us(X16, X16, X16), 99_517);
        assert_eq!(measurement_duration_us(X4, X8, X2), 14 * 1963 + 5_293);
    }

    // ── Calibration parsing ─────────────────────────────────────────────

    #[test]
    fn calibration_distinct_value_per_field() {
        // Every byte holds a distinct value, so a swapped or shifted offset
        // shows up as a wrong field.
        let mut b1 = [0u8; 25];
        for (i, b) in b1.iter_mut().enumerate() {
            *b = 0x10 + i as u8;
        }
        let mut b2 = [0u8; 16];
        for (i, b) in b2.iter_mut().enumerate() {
            *b = 0x80 + i as u8;
        }
        let c = CalibrationData::from_raw(&b1, &b2, 2, -5, 3).unwrap();
        assert_eq!(c.t2, i16::from_le_bytes([0x11, 0x12]));
        assert_eq!(c.t3, 0x13);
        assert_eq!(c.p1, u16::from_le_bytes([0x15, 0x16]));
        assert_eq!(c.p2, i16::from_le_bytes([0x17, 0x18]));
        assert_eq!(c.p3, 0x19);
        assert_eq!(c.p4, i16::from_le_bytes([0x1B, 0x1C]));
        assert_eq!(c.p5, i16::from_le_bytes([0x1D, 0x1E]));
        assert_eq!(c.p7, 0x1F);
        assert_eq!(c.p6, 0x20);
        assert_eq!(c.p8, i16::from_le_bytes([0x23, 0x24]));
        assert_eq!(c.p9, i16::from_le_bytes([0x25, 0x26]));
        assert_eq!(c.p10, 0x27);
        assert_eq!(c.h2, (0x80 << 4) | (0x81 >> 4));
        assert_eq!(c.h1, (0x82 << 4) | (0x81 & 0x0F));
        assert_eq!(c.h3, 0x83u8 as i8);
        assert_eq!(c.h4, 0x84u8 as i8);
        assert_eq!(c.h5, 0x85u8 as i8);
        assert_eq!(c.h6, 0x86);
        assert_eq!(c.h7, 0x87u8 as i8);
        assert_eq!(c.t1, u16::from_le_bytes([0x88, 0x89]));
        assert_eq!(c.gh2, i16::from_le_bytes([0x8A, 0x8B]));
        assert_eq!(c.gh1, 0x8Cu8 as i8);
        assert_eq!(c.gh3, 0x8Du8 as i8);
        assert_eq!(c.res_heat_range, 2);
        assert_eq!(c.res_heat_val, -5);
        assert_eq!(c.range_sw_err, 3);
    }

    #[test]
    fn humidity_nibbles_split_e2() {
        // E1 = 0x12, E2 = 0xAB, E3 = 0x34: E2's low nibble 0xB belongs to
        // par_h1, its high nibble 0xA to par_h2.
        let mut b2 = COEFF2;
        b2[0] = 0x12;
        b2[1] = 0xAB;
        b2[2] = 0x34;
        let c = CalibrationData::from_raw(&COEFF1, &b2, 1, -12, -2).unwrap();
        assert_eq!(c.h1, 0x34B);
        assert_eq!(c.h2, 0x12A);
    }

    #[test]
    fn negative_sw_err_and_heat_val() {
        assert_eq!(range_sw_err_from_reg(0xE0), -2);
        assert_eq!(range_sw_err_from_reg(0xEF), -2); // low nibble ignored
        assert_eq!(range_sw_err_from_reg(0x80), -8);
        assert_eq!(range_sw_err_from_reg(0x70), 7);
        assert_eq!(range_sw_err_from_reg(0x0F), 0);
        assert_eq!(res_heat_range_from_reg(0x10), 1);
        assert_eq!(res_heat_range_from_reg(0xFF), 3);
        assert_eq!(res_heat_range_from_reg(0xCF), 0);

        let c = reference_calib();
        assert_eq!(c.range_sw_err, -2);
        assert_eq!(c.res_heat_val, -12);
        // The sign reaches the gas formula: var1 = 1340 + 5 * sw_err, so for
        // adc_gas > 512 a negative sw_err gives fewer ohms than sw_err = 0,
        // and the unsigned misdecode (0xE0 >> 4 = 14) gives more.
        let mut zero = c;
        zero.range_sw_err = 0;
        let mut unsigned = c;
        unsigned.range_sw_err = 14;
        let neg = calc_gas_resistance_ohm(&c, 855, 7).unwrap();
        let none = calc_gas_resistance_ohm(&zero, 855, 7).unwrap();
        let bug = calc_gas_resistance_ohm(&unsigned, 855, 7).unwrap();
        assert!(neg < none && none < bug);
    }

    #[test]
    fn degenerate_calibration_rejected() {
        let zeros1 = [0x00; 25];
        let zeros2 = [0x00; 16];
        let ones1 = [0xFF; 25];
        let ones2 = [0xFF; 16];
        assert_eq!(
            CalibrationData::from_raw(&zeros1, &COEFF2, 1, 0, 0),
            Err(CalibrationError::AllZeros)
        );
        assert_eq!(
            CalibrationData::from_raw(&COEFF1, &zeros2, 1, 0, 0),
            Err(CalibrationError::AllZeros)
        );
        assert_eq!(
            CalibrationData::from_raw(&ones1, &COEFF2, 1, 0, 0),
            Err(CalibrationError::AllOnes)
        );
        assert_eq!(
            CalibrationData::from_raw(&COEFF1, &ones2, 1, 0, 0),
            Err(CalibrationError::AllOnes)
        );
        assert_eq!(
            CalibrationData::from_raw(&ones1, &ones2, 0, -1, -1),
            Err(CalibrationError::AllOnes)
        );
    }

    #[test]
    fn calibration_error_display() {
        assert!(CalibrationError::AllZeros.to_string().contains("0x00"));
        assert!(CalibrationError::AllOnes.to_string().contains("0xFF"));
    }

    #[test]
    fn zero_pressure_divisor_gives_zero_not_nan() {
        // par_p1 = 0 makes the Bosch divisor truncate to zero.
        let mut b1 = COEFF1;
        b1[5] = 0;
        b1[6] = 0;
        let c = CalibrationData::from_raw(&b1, &COEFF2, 1, -12, -2).unwrap();
        let reading = compensate(&c, &parse_raw(&DATA));
        assert_eq!(reading.pressure_hpa(), 0.0);
        assert!(reading.temperature_c().is_finite());
        assert!(reading.humidity_rh().is_finite());
    }

    #[test]
    fn extreme_inputs_stay_finite() {
        let c = reference_calib();
        for &(p, t, h, g) in &[
            (0u32, 0u32, 0u16, 0u16),
            (0xF_FFFF, 0xF_FFFF, 0xFFFF, 0x3FF),
            (0x8_0000, 0x8_0000, 0x8000, 512),
        ] {
            for range in 0..16u8 {
                let r = compensate(&c, &parse_raw(&block(p, t, h, g, 0x30 | range)));
                assert!(r.temperature_c().is_finite());
                assert!(r.pressure_hpa().is_finite());
                assert!((0.0..=100.0).contains(&r.humidity_rh()));
                assert!(r.gas_resistance_ohm().unwrap().is_finite());
            }
        }
    }

    // ── Edges ───────────────────────────────────────────────────────────

    #[test]
    fn humidity_clamps_at_0_and_100() {
        let c = reference_calib();
        let low = compensate(&c, &parse_raw(&block(351_595, 496_198, 0, 855, 0x37)));
        assert_eq!(low.humidity_rh(), 0.0);
        let high = compensate(&c, &parse_raw(&block(351_595, 496_198, 0xFFFF, 855, 0x37)));
        assert_eq!(high.humidity_rh(), 100.0);
    }

    #[test]
    fn all_16_gas_ranges_match_bosch() {
        // Same oracle as the reference fixture, adc_gas = 855, per range.
        const EXPECTED: [f32; 16] = [
            6359832.5, 3179916.2, 1589958.1, 794979.06, 397092.4, 196955.38, 99372.38, 50004.21,
            24867.963, 12421.548, 6208.2236, 3102.1912, 1552.6935, 774.7423, 388.17337, 194.08669,
        ];
        let c = reference_calib();
        for (range, &expected) in EXPECTED.iter().enumerate() {
            let raw = parse_raw(&block(351_595, 496_198, 22_646, 855, 0x30 | range as u8));
            assert_eq!(raw.gas_range(), range as u8);
            let ohm = compensate(&c, &raw).gas_resistance_ohm().unwrap();
            assert_rel(ohm, expected, 0.00001);
        }
    }

    #[test]
    fn gas_none_without_valid_or_stable() {
        let c = reference_calib();
        for flags in [0x00u8, 0x10, 0x20] {
            let raw = parse_raw(&block(351_595, 496_198, 22_646, 855, flags | 0x07));
            assert_eq!(
                compensate(&c, &raw).gas_resistance_ohm(),
                None,
                "{flags:#x}"
            );
        }
        let raw = parse_raw(&block(351_595, 496_198, 22_646, 855, 0x37));
        assert!(compensate(&c, &raw).gas_resistance_ohm().is_some());
    }

    // ── Encodings and register bytes ────────────────────────────────────

    #[test]
    fn oversampling_encodings() {
        use Oversampling::*;
        let all = [X1, X2, X4, X8, X16];
        for (i, os) in all.iter().enumerate() {
            assert_eq!(os.bits(), i as u8 + 1);
        }
        let cycles: [u32; 5] = [1, 2, 4, 8, 16];
        for (os, n) in all.iter().zip(cycles) {
            assert_eq!(os.cycles(), n);
        }
    }

    #[test]
    fn iir_filter_encodings() {
        use IirFilter::*;
        let all = [
            Off, Coeff1, Coeff3, Coeff7, Coeff15, Coeff31, Coeff63, Coeff127,
        ];
        for (i, f) in all.iter().enumerate() {
            assert_eq!(f.bits(), i as u8);
            assert_eq!(config_byte(*f), (i as u8) << 2);
            assert_eq!(config_byte(*f) & !0x1C, 0);
        }
    }

    #[test]
    fn ctrl_byte_layouts() {
        use Oversampling::*;
        assert_eq!(ctrl_meas_byte(X2, X16), 0x55); // 010 101 01
        assert_eq!(ctrl_meas_byte(X1, X1), 0x25); // 001 001 01
        assert_eq!(ctrl_meas_byte(X16, X1), 0xA5); // 101 001 01
        assert_eq!(ctrl_hum_byte(X1), 0b001);
        assert_eq!(ctrl_hum_byte(X16), 0b101);
        assert_eq!(ctrl_gas_0_byte(true), 0x00);
        assert_eq!(ctrl_gas_0_byte(false), 0x08);
        assert_eq!(ctrl_gas_1_byte(true), 0x10);
        assert_eq!(ctrl_gas_1_byte(false), 0x00);
        assert_eq!(config_byte(IirFilter::Coeff3), 0x08);
    }

    #[test]
    fn op_mode_helpers() {
        assert_eq!(op_mode(0x55), 0b01);
        assert_eq!(op_mode(0x54), 0b00);
        assert_eq!(op_mode(0xFF), 0b11);
        assert_eq!(with_sleep_mode(0x55), 0x54);
        assert_eq!(with_sleep_mode(0xA7), 0xA4);
        assert_eq!(
            with_sleep_mode(ctrl_meas_byte(Oversampling::X2, Oversampling::X16)),
            0x54
        );
        assert_eq!(op_mode(with_sleep_mode(0xFF)), 0);
    }

    #[test]
    fn gas_wait_matches_bosch() {
        assert_eq!(gas_wait(HEATER_DURATION_MS), 0x65);
        assert_eq!(gas_wait(0), 0x00);
        assert_eq!(gas_wait(63), 0x3F);
        assert_eq!(gas_wait(64), 0x50); // 64 / 4 = 16, x4
        assert_eq!(gas_wait(100), 0x59); // 25, x4
        assert_eq!(gas_wait(0xFBF), 0xFE); // 4031 -> 62, x64
        assert_eq!(gas_wait(0xFC0), 0xFF);
        assert_eq!(gas_wait(u16::MAX), 0xFF);
    }

    #[test]
    fn status_bits() {
        assert!(is_new_data(0x80));
        assert!(!is_new_data(0x7F));
        assert!(is_gas_measuring(0x40));
        assert!(!is_gas_measuring(0xBF));
        assert!(is_measuring(0x20));
        assert!(!is_measuring(0xDF));
        assert!(is_measurement_ready(0x80));
        assert!(is_measurement_ready(0x8F)); // gas_meas_index ignored
        assert!(!is_measurement_ready(0x00));
        assert!(!is_measurement_ready(0xA0));
        assert!(!is_measurement_ready(0xC0));
        assert!(!is_measurement_ready(0x60));
    }

    #[test]
    fn parse_raw_shifts() {
        let mut b = [0u8; 15];
        b[2] = 0xAB;
        b[3] = 0xCD;
        b[4] = 0xEF; // low nibble dropped
        b[5] = 0x12;
        b[6] = 0x34;
        b[7] = 0x5F;
        b[8] = 0x67;
        b[9] = 0x89;
        b[13] = 0xFE;
        b[14] = 0xAC; // 10 1 0 1100: gas lsb 0b10, valid, not stable, range 12
        let r = parse_raw(&b);
        assert_eq!(r.adc_pressure(), 0xABCDE);
        assert_eq!(r.adc_temperature(), 0x12345);
        assert_eq!(r.adc_humidity(), 0x6789);
        assert_eq!(r.adc_gas(), (0xFE << 2) | 0b10);
        assert_eq!(r.gas_range(), 12);
        assert!(r.gas_valid());
        assert!(!r.heat_stab());

        b[14] = 0x53; // 01 0 1 0011: gas lsb 0b01, not valid, stable, range 3
        let r = parse_raw(&b);
        assert_eq!(r.adc_gas(), (0xFE << 2) | 0b01);
        assert_eq!(r.gas_range(), 3);
        assert!(!r.gas_valid());
        assert!(r.heat_stab());

        let r = parse_raw(&[0xFF; 15]);
        assert_eq!(r.adc_pressure(), 0xF_FFFF);
        assert_eq!(r.adc_temperature(), 0xF_FFFF);
        assert_eq!(r.adc_humidity(), 0xFFFF);
        assert_eq!(r.adc_gas(), 0x3FF);
        assert_eq!(r.gas_range(), 15);
    }

    #[test]
    fn block_helper_round_trips() {
        assert_eq!(block(351_595, 496_198, 22_646, 855, 0x37), DATA);
    }

    // ── Driver (MockI2c) ────────────────────────────────────────────────

    #[cfg(feature = "hal")]
    mod driver {
        use super::*;
        use crate::mock::{MockI2c, MockI2cError, Transaction};
        use embedded_hal::delay::DelayNs;

        const A: u8 = I2C_ADDR;

        /// Counts `DelayNs` time so tests can assert the budget accounting.
        struct CountingDelay {
            total_ns: u64,
        }

        impl DelayNs for CountingDelay {
            fn delay_ns(&mut self, ns: u32) {
                self.total_ns += u64::from(ns);
            }
        }

        fn delay() -> CountingDelay {
            CountingDelay { total_ns: 0 }
        }

        fn ms(n: u64) -> u64 {
            n * 1_000_000
        }

        fn w<'a>(bytes: &'a [u8]) -> Transaction<'a> {
            Transaction::Write { address: A, bytes }
        }

        fn rd<'a>(write: &'a [u8], response: &'a [u8]) -> Transaction<'a> {
            Transaction::WriteRead {
                address: A,
                write,
                response,
            }
        }

        const INIT: [Transaction<'static>; 7] = [
            Transaction::Write {
                address: A,
                bytes: &[REG_SOFT_RESET, SOFT_RESET_CMD],
            },
            Transaction::WriteRead {
                address: A,
                write: &[REG_CHIP_ID],
                response: &[CHIP_ID_VALUE],
            },
            Transaction::WriteRead {
                address: A,
                write: &[REG_COEFF1],
                response: &COEFF1,
            },
            Transaction::WriteRead {
                address: A,
                write: &[REG_COEFF2],
                response: &COEFF2,
            },
            Transaction::WriteRead {
                address: A,
                write: &[REG_RES_HEAT_RANGE],
                response: &[RES_HEAT_RANGE_REG],
            },
            Transaction::WriteRead {
                address: A,
                write: &[REG_RES_HEAT_VAL],
                response: &[RES_HEAT_VAL_REG],
            },
            Transaction::WriteRead {
                address: A,
                write: &[REG_RANGE_SW_ERR],
                response: &[RANGE_SW_ERR_REG],
            },
        ];

        /// A script that starts with the `new()` sequence, then `$t...`.
        macro_rules! script {
            ($($t:expr),* $(,)?) => {
                [INIT[0], INIT[1], INIT[2], INIT[3], INIT[4], INIT[5], INIT[6], $($t),*]
            };
        }

        const STATUS: [u8; 1] = [REG_MEAS_STATUS_0];
        // X2 / X16 / X1 with filter Coeff3: config 0x08, ctrl_hum 0x01, ctrl_meas 0x55.
        const T: Oversampling = Oversampling::X2;
        const P: Oversampling = Oversampling::X16;
        const H: Oversampling = Oversampling::X1;
        const F: IirFilter = IirFilter::Coeff3;
        // 42_590 us -> 43 ms; with the 150 ms heater plateau -> 193 ms.
        const REQ: u32 = 43;
        const REQ_GAS: u32 = 193;

        const CTRL: [u8; 1] = [REG_CTRL_MEAS];

        /// The sleep-recovery read: the sensor is already in sleep mode.
        fn sleep_ok() -> Transaction<'static> {
            rd(&CTRL, &[0x54])
        }

        fn open<'a>(script: &'a [Transaction<'a>]) -> Bme680<MockI2c<'a>> {
            Bme680::new(MockI2c::new(script), A, &mut delay()).unwrap()
        }

        fn expected() -> Reading {
            compensate(&reference_calib(), &parse_raw(&DATA))
        }

        #[test]
        fn new_runs_the_documented_sequence() {
            let script = script!();
            let mut d = delay();
            let sensor = Bme680::new(MockI2c::new(&script), A, &mut d).unwrap();
            assert_eq!(d.total_ns, ms(10));
            sensor.release().done();
        }

        #[test]
        fn wrong_chip_id_returns_the_bus() {
            let script = [INIT[0], rd(&[REG_CHIP_ID], &[0x60])];
            let Err((e, bus)) = Bme680::new(MockI2c::new(&script), A, &mut delay()) else {
                panic!("expected an error");
            };
            assert_eq!(e, Error::WrongChipId { found: 0x60 });
            assert!(!e.is_nack());
            bus.done();
        }

        #[test]
        fn nack_on_reset_keeps_the_bus_error() {
            let script = [Transaction::Error(MockI2cError::nack())];
            let Err((e, bus)) = Bme680::new(MockI2c::new(&script), A, &mut delay()) else {
                panic!("expected an error");
            };
            assert_eq!(e, Error::Bus(MockI2cError::nack()));
            assert!(e.is_nack());
            bus.done();
        }

        #[test]
        fn bus_fault_is_not_a_nack() {
            let script = [INIT[0], Transaction::Error(MockI2cError::bus())];
            let Err((e, bus)) = Bme680::new(MockI2c::new(&script), A, &mut delay()) else {
                panic!("expected an error");
            };
            assert!(!e.is_nack());
            bus.done();
        }

        #[test]
        fn degenerate_calibration_is_rejected() {
            let zeros = [0u8; 25];
            let script = [
                INIT[0],
                INIT[1],
                rd(&[REG_COEFF1], &zeros),
                INIT[3],
                INIT[4],
                INIT[5],
                INIT[6],
            ];
            let Err((e, bus)) = Bme680::new(MockI2c::new(&script), A, &mut delay()) else {
                panic!("expected an error");
            };
            assert_eq!(e, Error::Calibration(CalibrationError::AllZeros));
            bus.done();
        }

        #[test]
        fn budget_too_short_is_rejected_before_any_bus_write() {
            let script = script!();
            let mut sensor = open(&script);
            let mut d = delay();
            let e = sensor.measure(&mut d, T, P, H, F, true, REQ_GAS - 1);
            assert_eq!(
                e,
                Err(Error::BudgetTooShort {
                    required_ms: REQ_GAS
                })
            );
            let e = sensor.measure(&mut d, T, P, H, F, false, REQ - 1);
            assert_eq!(e, Err(Error::BudgetTooShort { required_ms: REQ }));
            assert_eq!(d.total_ns, 0);
            sensor.release().done();
        }

        #[test]
        fn wrong_variant_returns_the_bus() {
            let mut coeff2 = COEFF2;
            coeff2[15] = 0x01;
            let script = [INIT[0], INIT[1], INIT[2], rd(&[REG_COEFF2], &coeff2)];
            let Err((e, bus)) = Bme680::new(MockI2c::new(&script), A, &mut delay()) else {
                panic!("expected an error");
            };
            assert_eq!(e, Error::UnsupportedVariant { found: 0x01 });
            assert!(!e.is_nack());
            bus.done();
        }

        #[test]
        fn measure_after_a_timeout_forces_sleep_first() {
            let busy = rd(&CTRL, &[0x55]);
            let script = script!(
                busy,
                w(&[REG_CTRL_MEAS, 0x54]),
                busy,
                sleep_ok(),
                w(&[REG_CONFIG, 0x08]),
                w(&[REG_CTRL_HUM, 0x01]),
                w(&[REG_CTRL_GAS_0, 0x08]),
                w(&[REG_CTRL_GAS_1, 0x00]),
                rd(&STATUS, &[0u8; 15]),
                w(&[REG_CTRL_MEAS, 0x55]),
                rd(&STATUS, &[0x80]),
                rd(&STATUS, &DATA),
            );
            let mut sensor = open(&script);
            let mut d = delay();
            // One 1 ms wait for sleep, then the conversion wait.
            let r = sensor.measure(&mut d, T, P, H, F, false, REQ + 1).unwrap();
            assert_eq!(r.temperature_c(), expected().temperature_c());
            assert_eq!(d.total_ns, ms(u64::from(REQ) + 1));
            sensor.release().done();
        }

        #[test]
        fn sensor_stuck_in_forced_mode_times_out_without_config_writes() {
            let busy = rd(&CTRL, &[0x55]);
            let script = script!(busy, w(&[REG_CTRL_MEAS, 0x54]), busy, busy, busy);
            let mut sensor = open(&script);
            let mut d = delay();
            // Budget of 2 ms beyond the conversion time: three reads, two waits.
            let e = sensor.measure(&mut d, T, P, H, F, false, REQ + 2);
            assert_eq!(e, Err(Error::Timeout));
            assert_eq!(d.total_ns, ms(2));
            sensor.release().done();
        }

        #[test]
        fn timeout_when_new_data_never_sets() {
            let idle = rd(&STATUS, &[0x00]);
            let script = script!(
                sleep_ok(),
                w(&[REG_CONFIG, 0x08]),
                w(&[REG_CTRL_HUM, 0x01]),
                w(&[REG_CTRL_GAS_0, 0x08]),
                w(&[REG_CTRL_GAS_1, 0x00]),
                rd(&STATUS, &[0u8; 15]),
                w(&[REG_CTRL_MEAS, 0x55]),
                idle,
                idle,
                idle,
                idle,
            );
            let mut sensor = open(&script);
            let mut d = delay();
            // Budget of 3 ms beyond the conversion time: four polls, three waits.
            let e = sensor.measure(&mut d, T, P, H, F, false, REQ + 3);
            assert_eq!(e, Err(Error::Timeout));
            assert_eq!(d.total_ns, ms(u64::from(REQ) + 3));
            sensor.release().done();
        }

        #[test]
        fn gas_skipped_has_no_heater_writes_and_no_gas() {
            // DATA carries a valid, stable gas sample; it must be ignored.
            let script = script!(
                sleep_ok(),
                w(&[REG_CONFIG, 0x08]),
                w(&[REG_CTRL_HUM, 0x01]),
                w(&[REG_CTRL_GAS_0, 0x08]),
                w(&[REG_CTRL_GAS_1, 0x00]),
                rd(&STATUS, &[0u8; 15]),
                w(&[REG_CTRL_MEAS, 0x55]),
                rd(&STATUS, &[0x80]),
                rd(&STATUS, &DATA),
            );
            let mut sensor = open(&script);
            let mut d = delay();
            let r = sensor.measure(&mut d, T, P, H, F, false, REQ).unwrap();
            assert_eq!(r.gas_resistance_ohm(), None);
            assert_eq!(r.temperature_c(), expected().temperature_c());
            assert_eq!(d.total_ns, ms(u64::from(REQ)));
            sensor.release().done();
        }

        #[test]
        fn gas_read_writes_the_heater_profile_and_returns_ohms() {
            let script = script!(
                sleep_ok(),
                w(&[REG_CONFIG, 0x08]),
                w(&[REG_CTRL_HUM, 0x01]),
                w(&[REG_RES_HEAT_0, 142]),
                w(&[REG_GAS_WAIT_0, 0x65]),
                w(&[REG_CTRL_GAS_0, 0x00]),
                w(&[REG_CTRL_GAS_1, 0x10]),
                rd(&STATUS, &[0u8; 15]),
                w(&[REG_CTRL_MEAS, 0x55]),
                // gas_measuring still set on the first poll, then done.
                rd(&STATUS, &[0x80 | 0x40]),
                rd(&STATUS, &[0x80]),
                rd(&STATUS, &DATA),
            );
            let mut sensor = open(&script);
            let mut d = delay();
            let r = sensor
                .measure(&mut d, T, P, H, F, true, REQ_GAS + 1)
                .unwrap();
            assert_eq!(r, expected());
            assert!(r.gas_resistance_ohm().is_some());
            assert_eq!(d.total_ns, ms(u64::from(REQ_GAS) + 1));
            sensor.release().done();
        }

        #[test]
        fn heater_ambient_follows_the_last_measured_temperature() {
            // A clearly warmer first reading, so the carried-over ambient
            // changes the res_heat byte away from the 25 C default (142).
            let warm = block(351_595, 540_000, 22_646, 855, 0x37);
            let warm_c = compensate(&reference_calib(), &parse_raw(&warm)).temperature_c();
            let rh2 = res_heat(&reference_calib(), HEATER_TARGET_C, warm_c);
            assert_ne!(rh2, 142);
            let heat2 = [REG_RES_HEAT_0, rh2];
            let script = script!(
                sleep_ok(),
                w(&[REG_CONFIG, 0x08]),
                w(&[REG_CTRL_HUM, 0x01]),
                w(&[REG_RES_HEAT_0, 142]),
                w(&[REG_GAS_WAIT_0, 0x65]),
                w(&[REG_CTRL_GAS_0, 0x00]),
                w(&[REG_CTRL_GAS_1, 0x10]),
                rd(&STATUS, &[0u8; 15]),
                w(&[REG_CTRL_MEAS, 0x55]),
                rd(&STATUS, &[0x80]),
                rd(&STATUS, &warm),
                sleep_ok(),
                w(&[REG_CONFIG, 0x08]),
                w(&[REG_CTRL_HUM, 0x01]),
                w(&heat2),
                w(&[REG_GAS_WAIT_0, 0x65]),
                w(&[REG_CTRL_GAS_0, 0x00]),
                w(&[REG_CTRL_GAS_1, 0x10]),
                rd(&STATUS, &[0u8; 15]),
                w(&[REG_CTRL_MEAS, 0x55]),
                rd(&STATUS, &[0x80]),
                rd(&STATUS, &DATA),
            );
            let mut sensor = open(&script);
            let mut d = delay();
            sensor.measure(&mut d, T, P, H, F, true, REQ_GAS).unwrap();
            sensor.measure(&mut d, T, P, H, F, true, REQ_GAS).unwrap();
            sensor.release().done();
        }

        fn gas_flag_script(data: &[u8; 15]) -> Option<Reading> {
            let script = script!(
                sleep_ok(),
                w(&[REG_CONFIG, 0x08]),
                w(&[REG_CTRL_HUM, 0x01]),
                w(&[REG_RES_HEAT_0, 142]),
                w(&[REG_GAS_WAIT_0, 0x65]),
                w(&[REG_CTRL_GAS_0, 0x00]),
                w(&[REG_CTRL_GAS_1, 0x10]),
                rd(&STATUS, &[0u8; 15]),
                w(&[REG_CTRL_MEAS, 0x55]),
                rd(&STATUS, &[0x80]),
                rd(&STATUS, data),
            );
            let mut sensor = open(&script);
            let r = sensor
                .measure(&mut delay(), T, P, H, F, true, REQ_GAS)
                .unwrap();
            sensor.release().done();
            Some(r)
        }

        #[test]
        fn gas_invalid_or_heater_unstable_gives_none() {
            let mut invalid = DATA;
            invalid[14] &= !0x20;
            let r = gas_flag_script(&invalid).unwrap();
            assert_eq!(r.gas_resistance_ohm(), None);
            assert_eq!(r.temperature_c(), expected().temperature_c());

            let mut unstable = DATA;
            unstable[14] &= !0x10;
            assert_eq!(
                gas_flag_script(&unstable).unwrap().gas_resistance_ohm(),
                None
            );
        }

        #[test]
        fn stale_sample_is_drained_before_the_fresh_one_is_read() {
            // PENDING(hardware): confirm new_data clears on read vs on trigger (feature doc open question)
            // The drain response carries an old sample with new_data set; the
            // driver must discard it and return the post-trigger block.
            let stale = block(351_595, 480_000, 22_646, 855, 0x37);
            let script = script!(
                sleep_ok(),
                w(&[REG_CONFIG, 0x08]),
                w(&[REG_CTRL_HUM, 0x01]),
                w(&[REG_CTRL_GAS_0, 0x08]),
                w(&[REG_CTRL_GAS_1, 0x00]),
                rd(&STATUS, &stale),
                w(&[REG_CTRL_MEAS, 0x55]),
                rd(&STATUS, &[0x80 | 0x20]),
                rd(&STATUS, &[0x80]),
                rd(&STATUS, &DATA),
            );
            let mut sensor = open(&script);
            let mut d = delay();
            let r = sensor.measure(&mut d, T, P, H, F, false, REQ + 1).unwrap();
            assert_eq!(r.temperature_c(), expected().temperature_c());
            assert_ne!(
                r.temperature_c(),
                compensate(&reference_calib(), &parse_raw(&stale)).temperature_c()
            );
            assert_eq!(d.total_ns, ms(u64::from(REQ) + 1));
            sensor.release().done();
        }

        #[test]
        fn error_display_is_informative() {
            extern crate std;
            use std::string::ToString;
            let e: Error<MockI2cError> = Error::BudgetTooShort { required_ms: 43 };
            assert!(e.to_string().contains("43"));
            let e: Error<MockI2cError> = Error::WrongChipId { found: 0x60 };
            assert!(e.to_string().contains("0x60"));
            let e: Error<MockI2cError> = Error::UnsupportedVariant { found: 0x01 };
            assert!(e.to_string().contains("0x01"));
        }
    }

    // ── Bosch SensorAPI reference vectors ───────────────────────────────
    //
    // Provenance: Bosch BME68x SensorAPI v4.4.8 (`bme68x.c`, @date 2023-02-07),
    // `BME68X_USE_FPU` float path, BME680 low gas variant, run through the
    // *public* API (`bme68x_init`, `bme68x_get_data`, `bme68x_set_heatr_conf`,
    // `bme68x_get_meas_dur`) against a stubbed register file by the C harness
    // in /tmp/bme680-oracle/c/harness.c on 2026-10-30 (Apple clang, `-O0
    // -ffp-contract=off`). Register images are SYNTHETIC, not device dumps;
    // every float below is Bosch's `float` printed with the shortest digits that
    // parse back to the identical `f32` (`pressure_hpa` is Bosch's Pa / 100 in
    // f32, exactly what `compensate` computes; the Pa value is in the comment).
    //
    // Bosch computes `gas_resistance` even when `gasm_valid` / `heat_stab` are
    // clear and only reports the flags in `status`; this port returns `None`
    // there, so those vectors carry `None` with Bosch's number in a comment.
    mod bosch_vectors {
        use super::*;

        const TEMP_TOL_C: f32 = 0.01;
        const PRES_TOL_HPA: f32 = 0.01;
        const HUM_TOL_RH: f32 = 0.01;
        const GAS_TOL_REL: f32 = 0.001;

        struct SampleVector {
            name: &'static str,
            /// 15 bytes as read from `REG_MEAS_STATUS_0` (0x1D).
            data: [u8; 15],
            temperature_c: f32,
            pressure_hpa: f32,
            humidity_rh: f32,
            /// `None` when `gas_valid_r` or `heat_stab_r` is clear.
            gas_resistance_ohm: Option<f32>,
        }

        struct CalibVector {
            name: &'static str,
            /// 25 bytes as read from `REG_COEFF1` (0x89).
            coeff1: [u8; 25],
            /// 16 bytes as read from `REG_COEFF2` (0xE1).
            coeff2: [u8; 16],
            res_heat_val_reg: u8,
            res_heat_range_reg: u8,
            range_sw_err_reg: u8,
            /// `res_heat_0` byte Bosch writes for 320 °C at ambient 25, -10, 40 °C.
            res_heat_320: [(f32, u8); 3],
            samples: &'static [SampleVector],
        }

        impl CalibVector {
            fn calib(&self) -> CalibrationData {
                CalibrationData::from_raw(
                    &self.coeff1,
                    &self.coeff2,
                    res_heat_range_from_reg(self.res_heat_range_reg),
                    self.res_heat_val_reg as i8,
                    range_sw_err_from_reg(self.range_sw_err_reg),
                )
                .unwrap()
            }
        }

        /// Dataset 1 (`rust_fixture`): identical to the tamer bme680 test fixture (synthetic).
        /// Bosch `get_calib_data` parsed: t1=26013 t2=26214 t3=3 p1=36210 p2=-10450 p3=88 p4=7380 p5=-135
        /// p6=30 p7=40 p8=-2570 p9=-2070 p10=30 h1=813 h2=1010 h3=0 h4=45 h5=20 h6=120 h7=-100
        /// gh1=-30 gh2=-5600 gh3=18 res_heat_range=1 res_heat_val=-12 range_sw_err=-2.
        const DS1: CalibVector = CalibVector {
            name: "rust_fixture",
            coeff1: [
                0x00, 0x66, 0x66, 0x03, 0x00, 0x72, 0x8D, 0x2E, 0xD7, 0x58, 0x00, 0xD4, 0x1C, 0x79,
                0xFF, 0x28, 0x1E, 0x00, 0x00, 0xF6, 0xF5, 0xEA, 0xF7, 0x1E, 0x00,
            ],
            coeff2: [
                0x3F, 0x2D, 0x32, 0x00, 0x2D, 0x14, 0x78, 0x9C, 0x9D, 0x65, 0x20, 0xEA, 0xE2, 0x12,
                0x00, 0x00,
            ],
            res_heat_val_reg: 0xF4,
            res_heat_range_reg: 0x10,
            range_sw_err_reg: 0xE0,
            res_heat_320: [(25.0, 142), (-10.0, 141), (40.0, 143)],
            samples: &[
                // adc_t=496198 adc_p=351595 adc_h=22646 adc_gas=855 range=7 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=24.9999847 P=100000.102 Pa H=49.9995384 R=50004.2109 Ohm
                SampleVector {
                    name: "fixture_25c_1000hpa_50rh_r7",
                    data: [
                        0x80, 0x00, 0x55, 0xD6, 0xB0, 0x79, 0x24, 0x60, 0x58, 0x76, 0x00, 0x00,
                        0x00, 0xD5, 0xF7,
                    ],
                    temperature_c: 24.999985,
                    pressure_hpa: 1000.00104,
                    humidity_rh: 49.99954,
                    gas_resistance_ohm: Some(50004.21),
                },
                // adc_t=384206 adc_p=298799 adc_h=27858 adc_gas=845 range=3 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=-9.99991322 P=102999.992 Pa H=80.0006943 R=799759.5 Ohm
                SampleVector {
                    name: "cold_m10c_1030hpa_80rh_r3",
                    data: [
                        0x80, 0x00, 0x48, 0xF2, 0xF0, 0x5D, 0xCC, 0xE0, 0x6C, 0xD2, 0x00, 0x00,
                        0x00, 0xD3, 0x73,
                    ],
                    temperature_c: -9.999913,
                    pressure_hpa: 1029.9999,
                    humidity_rh: 80.000694,
                    gas_resistance_ohm: Some(799759.5),
                },
                // adc_t=560174 adc_p=397834 adc_h=15136 adc_gas=624 range=13 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=44.9999962 P=94999.9297 Pa H=9.99906921 R=900.006958 Ohm
                SampleVector {
                    name: "hot_dry_45c_950hpa_10rh_r13",
                    data: [
                        0x80, 0x00, 0x61, 0x20, 0xA0, 0x88, 0xC2, 0xE0, 0x3B, 0x20, 0x00, 0x00,
                        0x00, 0x9C, 0x3D,
                    ],
                    temperature_c: 44.999996,
                    pressure_hpa: 949.99927,
                    humidity_rh: 9.999069,
                    gas_resistance_ohm: Some(900.00696),
                },
                // adc_t=480202 adc_p=339177 adc_h=25083 adc_gas=69 range=2 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=20.0000553 P=101324.961 Pa H=65.0012207 R=2998872.25 Ohm
                SampleVector {
                    name: "high_res_20c_r2",
                    data: [
                        0x80, 0x00, 0x52, 0xCE, 0x90, 0x75, 0x3C, 0xA0, 0x61, 0xFB, 0x00, 0x00,
                        0x00, 0x11, 0x72,
                    ],
                    temperature_c: 20.000055,
                    pressure_hpa: 1013.24963,
                    humidity_rh: 65.00122,
                    gas_resistance_ohm: Some(2998872.2),
                },
                // adc_t=512193 adc_p=353432 adc_h=20896 adc_gas=855 range=7 gas_valid=0 heat_stab=1;
                // Bosch: rslt=0 status=0x90 T=29.9998817 P=100500.023 Pa H=39.9985962 R=50004.2109 Ohm
                SampleVector {
                    name: "gas_valid_clear_30c_r7",
                    data: [
                        0x80, 0x00, 0x56, 0x49, 0x80, 0x7D, 0x0C, 0x10, 0x51, 0xA0, 0x00, 0x00,
                        0x00, 0xD5, 0xD7,
                    ],
                    temperature_c: 29.999882,
                    pressure_hpa: 1005.00024,
                    humidity_rh: 39.998596,
                    gas_resistance_ohm: None, // Bosch still reports 50004.2109 Ohm with the flag clear
                },
                // adc_t=464205 adc_p=347867 adc_h=25936 adc_gas=986 range=15 gas_valid=1 heat_stab=0;
                // Bosch: rslt=0 status=0xA0 T=15.0000916 P=99000.0859 Pa H=69.9979477 R=179.992813 Ohm
                SampleVector {
                    name: "heat_stab_clear_15c_r15",
                    data: [
                        0x80, 0x00, 0x54, 0xED, 0xB0, 0x71, 0x54, 0xD0, 0x65, 0x50, 0x00, 0x00,
                        0x00, 0xF6, 0xAF,
                    ],
                    temperature_c: 15.000092,
                    pressure_hpa: 990.00085,
                    humidity_rh: 69.99795,
                    gas_resistance_ohm: None, // Bosch still reports 179.992813 Ohm with the flag clear
                },
            ],
        };

        /// Dataset 2 (`negative_small_coeffs`): negative t3/p3/p6/h3/h4, sw_err +3, res_heat_val +37, range 2, junk in padding registers.
        /// Bosch `get_calib_data` parsed: t1=26315 t2=26420 t3=-3 p1=35880 p2=-10180 p3=-88 p4=6400 p5=-210
        /// p6=-30 p7=55 p8=-3800 p9=-2650 p10=30 h1=760 h2=1035 h3=-5 h4=-45 h5=20 h6=120 h7=-100
        /// gh1=-28 gh2=-9830 gh3=18 res_heat_range=2 res_heat_val=37 range_sw_err=3.
        const DS2: CalibVector = CalibVector {
            name: "negative_small_coeffs",
            coeff1: [
                0xA5, 0x34, 0x67, 0xFD, 0x5A, 0x28, 0x8C, 0x3C, 0xD8, 0xA8, 0x3C, 0x00, 0x19, 0x2E,
                0xFF, 0x37, 0xE2, 0xC3, 0x99, 0x28, 0xF1, 0xA6, 0xF5, 0x1E, 0x77,
            ],
            coeff2: [
                0x40, 0xB8, 0x2F, 0xFB, 0xD3, 0x14, 0x78, 0x9C, 0xCB, 0x66, 0x9A, 0xD9, 0xE4, 0x12,
                0xEE, 0x00,
            ],
            res_heat_val_reg: 0x25,
            res_heat_range_reg: 0x2C,
            range_sw_err_reg: 0x3A,
            res_heat_320: [(25.0, 85), (-10.0, 84), (40.0, 86)],
            samples: &[
                // adc_t=500428 adc_p=370932 adc_h=22872 adc_gas=862 range=7 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=24.9998856 P=99999.9531 Pa H=49.999527 R=49987.9023 Ohm
                SampleVector {
                    name: "room_25c_1000hpa_50rh_r7",
                    data: [
                        0x80, 0x00, 0x5A, 0x8F, 0x40, 0x7A, 0x2C, 0xC0, 0x59, 0x58, 0x00, 0x00,
                        0x00, 0xD7, 0xB7,
                    ],
                    temperature_c: 24.999886,
                    pressure_hpa: 999.9995,
                    humidity_rh: 49.999527,
                    gas_resistance_ohm: Some(49987.902),
                },
                // adc_t=389291 adc_p=324623 adc_h=26554 adc_gas=849 range=4 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=-9.99992752 P=102500.023 Pa H=85.0025864 R=400013.656 Ohm
                SampleVector {
                    name: "cold_m10c_1025hpa_85rh_r4",
                    data: [
                        0x80, 0x00, 0x4F, 0x40, 0xF0, 0x5F, 0x0A, 0xB0, 0x67, 0xBA, 0x00, 0x00,
                        0x00, 0xD4, 0x74,
                    ],
                    temperature_c: -9.9999275,
                    pressure_hpa: 1025.0002,
                    humidity_rh: 85.00259,
                    gas_resistance_ohm: Some(400013.66),
                },
                // adc_t=579839 adc_p=450269 adc_h=14179 adc_gas=627 range=12 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=50.0001335 P=89999.9688 Pa H=7.99947977 R=1800.32947 Ohm
                SampleVector {
                    name: "hot_dry_50c_900hpa_8rh_r12",
                    data: [
                        0x80, 0x00, 0x6D, 0xED, 0xD0, 0x8D, 0x8F, 0xF0, 0x37, 0x63, 0x00, 0x00,
                        0x00, 0x9C, 0xFC,
                    ],
                    temperature_c: 50.000134,
                    pressure_hpa: 899.9997,
                    humidity_rh: 7.99948,
                    gas_resistance_ohm: Some(1800.3295),
                },
                // adc_t=490901 adc_p=360512 adc_h=23652 adc_gas=60 range=3 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=22.0001259 P=101325.008 Pa H=54.9974976 R=1500553.62 Ohm
                SampleVector {
                    name: "high_res_22c_r3",
                    data: [
                        0x80, 0x00, 0x58, 0x04, 0x00, 0x77, 0xD9, 0x50, 0x5C, 0x64, 0x00, 0x00,
                        0x00, 0x0F, 0x33,
                    ],
                    temperature_c: 22.000126,
                    pressure_hpa: 1013.25006,
                    humidity_rh: 54.997498,
                    gas_resistance_ohm: Some(1500553.6),
                },
                // adc_t=509956 adc_p=368061 adc_h=20113 adc_gas=1023 range=14 gas_valid=0 heat_stab=1;
                // Bosch: rslt=0 status=0x90 T=27.9998627 P=101000.039 Pa H=35.0020638 R=354.566498 Ohm
                SampleVector {
                    name: "gas_valid_clear_28c_r14",
                    data: [
                        0x80, 0x00, 0x59, 0xDB, 0xD0, 0x7C, 0x80, 0x40, 0x4E, 0x91, 0x00, 0x00,
                        0x00, 0xFF, 0xDE,
                    ],
                    temperature_c: 27.999863,
                    pressure_hpa: 1010.00037,
                    humidity_rh: 35.002064,
                    gas_resistance_ohm: None, // Bosch still reports 354.566498 Ohm with the flag clear
                },
                // adc_t=436916 adc_p=364465 adc_h=28166 adc_gas=1023 range=7 gas_valid=1 heat_stab=0;
                // Bosch: rslt=0 status=0xA0 T=5.00002337 P=97999.9531 Pa H=90.0018005 R=45649.7031 Ohm
                SampleVector {
                    name: "heat_stab_clear_5c_r7",
                    data: [
                        0x80, 0x00, 0x58, 0xFB, 0x10, 0x6A, 0xAB, 0x40, 0x6E, 0x06, 0x00, 0x00,
                        0x00, 0xFF, 0xE7,
                    ],
                    temperature_c: 5.0000234,
                    pressure_hpa: 979.9995,
                    humidity_rh: 90.0018,
                    gas_resistance_ohm: None, // Bosch still reports 45649.7031 Ohm with the flag clear
                },
            ],
        };

        /// Dataset 3 (`range3_swerr_minus8`): negative h5/gh3, positive h7, sw_err -8, res_heat_val +60, range 3.
        /// Bosch `get_calib_data` parsed: t1=27100 t2=25800 t3=5 p1=37050 p2=-10560 p3=88 p4=8200 p5=-95
        /// p6=30 p7=38 p8=-1500 p9=-3100 p10=30 h1=890 h2=990 h3=2 h4=50 h5=-20 h6=125 h7=100
        /// gh1=-35 gh2=-14000 gh3=-18 res_heat_range=3 res_heat_val=60 range_sw_err=-8.
        const DS3: CalibVector = CalibVector {
            name: "range3_swerr_minus8",
            coeff1: [
                0x00, 0xC8, 0x64, 0x05, 0x00, 0xBA, 0x90, 0xC0, 0xD6, 0x58, 0x00, 0x08, 0x20, 0xA1,
                0xFF, 0x26, 0x1E, 0x00, 0x00, 0x24, 0xFA, 0xE4, 0xF3, 0x1E, 0x00,
            ],
            coeff2: [
                0x3D, 0xEA, 0x37, 0x02, 0x32, 0xEC, 0x7D, 0x64, 0xDC, 0x69, 0x50, 0xC9, 0xDD, 0xEE,
                0x00, 0x00,
            ],
            res_heat_val_reg: 0x3C,
            res_heat_range_reg: 0x3A,
            range_sw_err_reg: 0x80,
            res_heat_320: [(25.0, 50), (-10.0, 52), (40.0, 50)],
            samples: &[
                // adc_t=514865 adc_p=324389 adc_h=23608 adc_gas=847 range=7 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=24.9998665 P=100000 Pa H=49.997715 R=50012.3086 Ohm
                SampleVector {
                    name: "room_25c_1000hpa_50rh_r7",
                    data: [
                        0x80, 0x00, 0x4F, 0x32, 0x50, 0x7D, 0xB3, 0x10, 0x5C, 0x38, 0x00, 0x00,
                        0x00, 0xD3, 0xF7,
                    ],
                    temperature_c: 24.999866,
                    pressure_hpa: 1e+03,
                    humidity_rh: 49.997715,
                    gas_resistance_ohm: Some(50012.31),
                },
                // adc_t=394579 adc_p=261952 adc_h=29179 adc_gas=252 range=2 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=-11.9999132 P=104000.062 Pa H=75.0028076 R=2500000 Ohm
                SampleVector {
                    name: "cold_m12c_1040hpa_75rh_r2",
                    data: [
                        0x80, 0x00, 0x3F, 0xF4, 0x00, 0x60, 0x55, 0x30, 0x71, 0xFB, 0x00, 0x00,
                        0x00, 0x3F, 0x32,
                    ],
                    temperature_c: -11.999913,
                    pressure_hpa: 1040.0006,
                    humidity_rh: 75.00281,
                    gas_resistance_ohm: Some(2.5e+06),
                },
                // adc_t=563606 adc_p=361607 adc_h=16918 adc_gas=799 range=15 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=40.0000114 P=96000.0156 Pa H=12.0024319 R=199.989182 Ohm
                SampleVector {
                    name: "hot_dry_40c_960hpa_12rh_r15",
                    data: [
                        0x80, 0x00, 0x58, 0x48, 0x70, 0x89, 0x99, 0x60, 0x42, 0x16, 0x00, 0x00,
                        0x00, 0xC7, 0xFF,
                    ],
                    temperature_c: 40.00001,
                    pressure_hpa: 960.0002,
                    humidity_rh: 12.002432,
                    gas_resistance_ohm: Some(199.98918),
                },
                // adc_t=492115 adc_p=309682 adc_h=25305 adc_gas=294 range=4 gas_valid=1 heat_stab=1;
                // Bosch: rslt=0 status=0xB0 T=17.9999866 P=101325.07 Pa H=60.0020142 R=600139.188 Ohm
                SampleVector {
                    name: "high_res_18c_r4",
                    data: [
                        0x80, 0x00, 0x4B, 0x9B, 0x20, 0x78, 0x25, 0x30, 0x62, 0xD9, 0x00, 0x00,
                        0x00, 0x49, 0xB4,
                    ],
                    temperature_c: 17.999987,
                    pressure_hpa: 1013.25073,
                    humidity_rh: 60.002014,
                    gas_resistance_ohm: Some(600139.2),
                },
                // adc_t=547361 adc_p=336791 adc_h=20249 adc_gas=228 range=12 gas_valid=0 heat_stab=1;
                // Bosch: rslt=0 status=0x90 T=35.0000992 P=99500.0156 Pa H=29.9994926 R=2499.07715 Ohm
                SampleVector {
                    name: "gas_valid_clear_35c_r12",
                    data: [
                        0x80, 0x00, 0x52, 0x39, 0x70, 0x85, 0xA2, 0x10, 0x4F, 0x19, 0x00, 0x00,
                        0x00, 0x39, 0x1C,
                    ],
                    temperature_c: 35.0001,
                    pressure_hpa: 995.0002,
                    humidity_rh: 29.999493,
                    gas_resistance_ohm: None, // Bosch still reports 2499.07715 Ohm with the flag clear
                },
                // adc_t=433600 adc_p=287227 adc_h=31153 adc_gas=238 range=7 gas_valid=1 heat_stab=0;
                // Bosch: rslt=0 status=0xA0 T=0 P=101999.977 Pa H=94.9986954 R=80001.9688 Ohm
                SampleVector {
                    name: "heat_stab_clear_0c_r7",
                    data: [
                        0x80, 0x00, 0x46, 0x1F, 0xB0, 0x69, 0xDC, 0x00, 0x79, 0xB1, 0x00, 0x00,
                        0x00, 0x3B, 0xA7,
                    ],
                    temperature_c: 0.0,
                    pressure_hpa: 1019.99976,
                    humidity_rh: 94.998695,
                    gas_resistance_ohm: None, // Bosch still reports 80001.9688 Ohm with the flag clear
                },
            ],
        };

        const DATASETS: [&CalibVector; 3] = [&DS1, &DS2, &DS3];

        fn for_each_sample(mut f: impl FnMut(&CalibVector, &SampleVector, Reading)) {
            for ds in DATASETS {
                let calib = ds.calib();
                for s in ds.samples {
                    f(ds, s, compensate(&calib, &parse_raw(&s.data)));
                }
            }
        }

        #[test]
        fn vector_count() {
            let n: usize = DATASETS.iter().map(|d| d.samples.len()).sum();
            assert_eq!(n, 18);
        }

        #[test]
        fn temperature_matches_bosch() {
            for_each_sample(|ds, s, r| {
                let d = (r.temperature_c() - s.temperature_c).abs();
                assert!(
                    d <= TEMP_TOL_C,
                    "{}/{}: T {} vs Bosch {}",
                    ds.name,
                    s.name,
                    r.temperature_c(),
                    s.temperature_c
                );
            });
        }

        #[test]
        fn pressure_matches_bosch() {
            for_each_sample(|ds, s, r| {
                let d = (r.pressure_hpa() - s.pressure_hpa).abs();
                assert!(
                    d <= PRES_TOL_HPA,
                    "{}/{}: P {} vs Bosch {}",
                    ds.name,
                    s.name,
                    r.pressure_hpa(),
                    s.pressure_hpa
                );
            });
        }

        #[test]
        fn humidity_matches_bosch() {
            for_each_sample(|ds, s, r| {
                let d = (r.humidity_rh() - s.humidity_rh).abs();
                assert!(
                    d <= HUM_TOL_RH,
                    "{}/{}: H {} vs Bosch {}",
                    ds.name,
                    s.name,
                    r.humidity_rh(),
                    s.humidity_rh
                );
            });
        }

        #[test]
        fn gas_resistance_matches_bosch_and_flags_gate_it() {
            for_each_sample(
                |ds, s, r| match (r.gas_resistance_ohm(), s.gas_resistance_ohm) {
                    (Some(got), Some(want)) => {
                        let rel = ((got - want) / want).abs();
                        assert!(
                            rel <= GAS_TOL_REL,
                            "{}/{}: R {} vs Bosch {}",
                            ds.name,
                            s.name,
                            got,
                            want
                        );
                    }
                    (None, None) => {
                        let raw = parse_raw(&s.data);
                        assert!(
                            !(raw.gas_valid() && raw.heat_stab()),
                            "{}/{}",
                            ds.name,
                            s.name
                        );
                    }
                    (got, want) => panic!("{}/{}: gas {got:?} vs Bosch {want:?}", ds.name, s.name),
                },
            );
        }

        #[test]
        fn res_heat_matches_bosch_set_heatr_conf() {
            for ds in DATASETS {
                let calib = ds.calib();
                for (amb, want) in ds.res_heat_320 {
                    assert_eq!(res_heat(&calib, 320.0, amb), want, "{} @ {amb} C", ds.name);
                }
            }
        }

        #[test]
        fn measurement_duration_matches_bosch_get_meas_dur() {
            use Oversampling::*;
            assert_eq!(measurement_duration_us(X2, X16, X1), 42590);
            assert_eq!(measurement_duration_us(X1, X1, X1), 11182);
            assert_eq!(measurement_duration_us(X16, X16, X16), 99517);
        }
    }
}
