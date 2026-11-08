//! One-line raw-count ADC1 reader for the ESP-IDF tier.
//!
//! [`AdcReader`] owns the ADC1 unit and one analog channel and implements
//! [`tamer::analog::AnalogRead`], so it plugs straight into
//! [`AnalogInput`] and the other pure `tamer`
//! analog helpers.
//! It is bus setup only: all range, deadband and smoothing logic stays in `tamer`.
//!
//! This module also re-exports everything from [`tamer::analog`], because the
//! local module otherwise shadows the glob-re-exported `tamer::analog` path.
//!
//! # Raw counts on both tiers
//!
//! `read_raw` returns raw counts (no voltage calibration scheme).
//! ESP-IDF's one-shot driver loads the efuse init code (zero-volt offset)
//! itself, whereas the esp-hal twin must opt in; esp-hal's default init code 0
//! is what made the C3 report 1.67 V as roughly 4030 counts.
//! Both tiers' `AdcReader` load the init code where the chip has one, so both tiers
//! use equivalent ADC initialization and return comparable raw counts
//! (C3, same wiring: 2345 idf vs 2363 hal).
//! See `docs/project-lore.md`, "esp-hal vs esp-idf — ADC init code".
//!
//! The attenuation is fixed at `DB_12` (the esp-hal equivalent is 11 dB),
//! giving roughly a 0 – 3.1 V input range.
//!
//! # Example
//!
//! ```no_run
//! use rustyfarian_esp_idf_peripherals::analog::{AdcReader, AnalogRead};
//! use esp_idf_hal::peripherals::Peripherals;
//!
//! # fn main() -> anyhow::Result<()> {
//! let peripherals = Peripherals::take()?;
//! let mut adc = AdcReader::new(peripherals.adc1, peripherals.pins.gpio4)?;
//! let raw: u16 = adc.read_raw()?;
//! # let _ = raw;
//! # Ok(())
//! # }
//! ```

use core::fmt;

use esp_idf_hal::{
    adc::{
        attenuation::DB_12,
        oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
        ADC1, ADCU1,
    },
    gpio::ADCPin,
    sys::EspError,
};

pub use tamer::analog::*;

/// Error returned when ADC setup or a conversion fails.
///
/// Newtype over the underlying ESP-IDF error so `esp_idf_sys` types stay out
/// of this crate's public API; the cause is reachable via
/// [`std::error::Error::source`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdcError(EspError);

impl From<EspError> for AdcError {
    fn from(err: EspError) -> Self {
        Self(err)
    }
}

impl fmt::Display for AdcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ADC error: {}", self.0)
    }
}

impl std::error::Error for AdcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// One-shot raw-count reader for a single ADC1 channel.
///
/// The attenuation is fixed at `DB_12`; see the [module docs](self) for why.
/// Implements [`AnalogRead`] with [`AdcError`] as its error type.
///
/// Each reader takes the whole ADC1 peripheral, so only one `AdcReader` can exist at a time:
/// a Hall sensor and a potentiometer cannot be read side by side through two readers.
/// A shared multi-channel driver is left until a consumer needs it.
///
/// The channel driver owns its ADC driver, so there is no borrow between
/// fields and the reader can be moved freely.
pub struct AdcReader<'d, C: esp_idf_hal::adc::AdcChannel<AdcUnit = ADCU1>> {
    channel: AdcChannelDriver<'d, C, AdcDriver<'d, ADCU1>>,
}

impl<C: esp_idf_hal::adc::AdcChannel<AdcUnit = ADCU1>> fmt::Debug for AdcReader<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdcReader").finish_non_exhaustive()
    }
}

impl<'d, C: esp_idf_hal::adc::AdcChannel<AdcUnit = ADCU1>> AdcReader<'d, C> {
    /// Configures ADC1 for one-shot reads of `pin` at `DB_12`.
    ///
    /// # Errors
    ///
    /// Returns [`AdcError`] if the ADC unit cannot be claimed or the channel
    /// cannot be configured.
    pub fn new(adc1: ADC1<'d>, pin: impl ADCPin<AdcChannel = C> + 'd) -> Result<Self, AdcError> {
        let driver = AdcDriver::new(adc1)?;
        let config = AdcChannelConfig {
            attenuation: DB_12,
            ..Default::default()
        };
        let channel = AdcChannelDriver::new(driver, pin, &config)?;
        Ok(Self { channel })
    }
}

impl<'d, C: esp_idf_hal::adc::AdcChannel<AdcUnit = ADCU1>> AnalogRead for AdcReader<'d, C> {
    type Error = AdcError;

    /// Reads one raw sample (0 – 4095).
    ///
    /// # Errors
    ///
    /// Returns [`AdcError`] if the ESP-IDF one-shot read fails.
    fn read_raw(&mut self) -> Result<u16, AdcError> {
        Ok(self.channel.read_raw()?)
    }
}
