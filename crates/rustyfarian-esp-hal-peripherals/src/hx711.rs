//! HX711 load-cell amplifier support for the esp-hal tier.
//!
//! The pure scale pipeline, calibration and driver live in [`tamer::hx711`];
//! this module re-exports the pure items and adds [`MaskedSection`], the
//! [`InterruptMask`] that keeps the 24-bit pulse train atomic.
//! Build the driver itself from `tamer::hx711::Hx711` (tamer's `hal` feature)
//! over an esp-hal `Input` (DOUT) and `Output` (PD_SCK).
//!
//! Experimental: not yet verified on hardware.

pub use tamer::hx711::{
    is_saturated, sign_extend_24, Calibration, CalibrationError, Gain, InterruptMask,
    LoadCellScale, NoopInterruptMask, CALIBRATION_MAGIC, CALIBRATION_VERSION, POLL_INTERVAL_US,
    POWER_DOWN_US, PULSE_US, RAW_MAX, RAW_MIN, SETTLE_MS_10HZ, SETTLE_MS_80HZ,
};

/// Interrupt mask over [`critical_section`] for the HX711 pulse train.
///
/// esp-hal's provider masks interrupts on the calling core and takes a core
/// spinlock on dual-core chips, so an interrupt on this core cannot hold PD_SCK
/// high past [`POWER_DOWN_US`] and power the chip down mid-read (ADR-016).
/// The provider exists under esp-hal's `rt` feature, which the examples require.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaskedSection;

impl MaskedSection {
    /// Creates the mask.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl InterruptMask for MaskedSection {
    fn masked<R>(&self, f: impl FnOnce() -> R) -> R {
        critical_section::with(|_| f())
    }
}
