//! One-line, calibrated ADC1 reader for the bare-metal tier.
//!
//! [`AdcReader`] owns an ADC1 instance plus one analog pin and implements
//! [`tamer::analog::AnalogRead`], so it plugs straight into
//! [`AnalogInput`] and the other pure `tamer`
//! analog helpers.
//! It is bus setup only: all range, deadband and smoothing logic stays in `tamer`.
//!
//! This module also re-exports everything from [`tamer::analog`], because the
//! local module otherwise shadows the glob-re-exported `tamer::analog` path.
//!
//! # Why calibration is baked in
//!
//! The esp-hal ADC driver defaults to an *uncalibrated* scheme (init code 0).
//! On the ESP32-C3 that reads 1.67 V as roughly 4030 counts and clips one
//! pole of a bipolar sensor, whereas ESP-IDF loads the efuse calibration
//! automatically.
//! [`AdcReader`] therefore always loads the efuse init code (`AdcCalBasic`)
//! on C3/C6/S3, so both tiers use equivalent ADC initialization and return
//! comparable raw counts (C3, same wiring: 2363 hal vs 2345 idf), and no caller
//! can regress to the uncalibrated read.
//! The classic ESP32 has no init code to load, and its only scheme
//! (`AdcCalLine`) returns millivolts, so it reads raw counts uncalibrated,
//! as ESP-IDF does.
//! Only the C3 is hardware-proven; the ESP32, C6 and S3 paths are compile-verified.
//! See `docs/project-lore.md`, "esp-hal vs esp-idf — ADC init code".
//!
//! The attenuation is fixed at 11 dB, the esp-hal equivalent of ESP-IDF's
//! `DB_12`, giving roughly a 0 – 3.1 V input range.
//!
//! # Example
//!
//! ```ignore
//! use rustyfarian_esp_hal_peripherals::analog::{AdcReader, AnalogRead};
//!
//! let peripherals = esp_hal::init(esp_hal::Config::default());
//! let mut adc = AdcReader::new(peripherals.ADC1, peripherals.GPIO4);
//! let raw: u16 = adc.read_raw()?;
//! ```

use core::fmt;

use esp_hal::{
    analog::adc::{Adc, AdcChannel, AdcConfig, AdcPin, Attenuation},
    peripherals::ADC1,
    Blocking,
};

pub use tamer::analog::*;

/// Classic ESP32: no init code to load, and `AdcCalLine` would return mV, not counts.
#[cfg(esp32)]
type Cal<'d> = ();
/// C3 / C6 / S3: load the efuse init code (raw counts stay raw).
#[cfg(not(esp32))]
type Cal<'d> = esp_hal::analog::adc::AdcCalBasic<ADC1<'d>>;

/// Error returned when an ADC conversion fails.
///
/// A unit struct: `esp-hal` reports no cause beyond failure, and keeping it
/// opaque keeps `esp-hal` types out of this crate's public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdcError;

impl fmt::Display for AdcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ADC conversion failed")
    }
}

impl core::error::Error for AdcError {}

/// One-shot raw-count reader for a single ADC1 pin.
///
/// The efuse init code is always loaded where the chip has one, and the
/// attenuation is fixed at 11 dB; see the [module docs](self) for why.
/// Implements [`AnalogRead`] with [`AdcError`] as its error type.
///
/// Each reader takes the whole ADC1 peripheral, so only one `AdcReader` can exist at a time:
/// a Hall sensor and a potentiometer cannot be read side by side through two readers.
/// A shared multi-channel driver is left until a consumer needs it.
pub struct AdcReader<'d, PIN> {
    adc: Adc<'d, ADC1<'d>, Blocking>,
    pin: AdcPin<PIN, ADC1<'d>, Cal<'d>>,
}

impl<PIN> fmt::Debug for AdcReader<'_, PIN> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdcReader").finish_non_exhaustive()
    }
}

impl<'d, PIN: AdcChannel + esp_hal::gpio::AnalogPin> AdcReader<'d, PIN> {
    /// Configures ADC1 for one-shot reads of `pin` at 11 dB.
    ///
    /// Infallible: esp-hal ADC setup cannot fail.
    #[must_use]
    pub fn new(adc1: ADC1<'d>, pin: PIN) -> Self {
        let mut config = AdcConfig::new();
        let pin = config.enable_pin_with_cal::<_, Cal<'d>>(pin, Attenuation::_11dB);
        let adc = Adc::new(adc1, config);
        Self { adc, pin }
    }
}

impl<'d, PIN: AdcChannel> AnalogRead for AdcReader<'d, PIN> {
    type Error = AdcError;

    /// Reads one raw sample (0 – 4095), blocking until the conversion finishes.
    ///
    /// # Errors
    ///
    /// Returns [`AdcError`] if the ADC reports a failed conversion.
    fn read_raw(&mut self) -> Result<u16, AdcError> {
        nb::block!(self.adc.read_oneshot(&mut self.pin)).map_err(|()| AdcError)
    }
}
