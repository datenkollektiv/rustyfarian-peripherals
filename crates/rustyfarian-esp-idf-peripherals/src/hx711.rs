//! HX711 load-cell amplifier support for the ESP-IDF tier.
//!
//! The pure scale pipeline, calibration and driver live in [`tamer::hx711`];
//! this module re-exports the pure items and adds [`MaskedSection`], the
//! [`InterruptMask`] that keeps the 24-bit pulse train atomic.
//! Build the driver itself from `tamer::hx711::Hx711` (tamer's `hal` feature)
//! over an esp-idf-hal `PinDriver` input (DOUT) and output (PD_SCK).
//!
//! Experimental: not yet verified on hardware.

pub use tamer::hx711::{
    is_saturated, sign_extend_24, Calibration, CalibrationError, Gain, InterruptMask,
    LoadCellScale, NoopInterruptMask, CALIBRATION_MAGIC, CALIBRATION_VERSION, POLL_INTERVAL_US,
    POWER_DOWN_US, PULSE_US, RAW_MAX, RAW_MIN, SETTLE_MS_10HZ, SETTLE_MS_80HZ,
};

use esp_idf_hal::interrupt::IsrCriticalSection;

/// Interrupt mask over [`IsrCriticalSection`] for the HX711 pulse train.
///
/// It masks interrupts up to the FreeRTOS syscall level on the calling core only
/// (`vPortEnterCritical` on C3/C6, a `portMUX` spinlock on ESP32/S3), so a task
/// switch or a GPIO-touching interrupt cannot stretch a pulse past [`POWER_DOWN_US`].
/// Higher-level interrupts may still run, but none of them touches GPIO, and a
/// second core is not stopped.
/// It must never be `critical_section::with`: on this tier that is a FreeRTOS
/// recursive mutex, which does not mask interrupts (docs/project-lore.md,
/// "ESP-IDF — critical-section backend ISR safety"; ADR-016).
pub struct MaskedSection(IsrCriticalSection);

impl MaskedSection {
    /// Creates the mask.
    #[must_use]
    pub const fn new() -> Self {
        Self(IsrCriticalSection::new())
    }
}

impl Default for MaskedSection {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for MaskedSection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("MaskedSection")
    }
}

impl InterruptMask for MaskedSection {
    fn masked<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self.0.enter();
        f()
    }
}
