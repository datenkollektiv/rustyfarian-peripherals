//! ESP32-C3 — Potentiometer on ADC1 (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_poti`.
//! It is a self-calibrating analog-input example for a potentiometer.
//! It samples the wiper through ADC1, observes a short startup calibration sweep
//! with [`AnalogCalibration`], normalizes the raw 12-bit reading with
//! [`AnalogValue`], and logs only when the value changes by a meaningful
//! deadband.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), matching the
//! other ESP-IDF examples in this crate.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 1 × potentiometer, 10 kΩ is a good default
//!
//! ## Wiring
//!
//! ```text
//! Potentiometer     ESP32-C3
//! ─────────────     ────────
//! outer leg         3V3
//! wiper/middle      GPIO 4
//! outer leg         GND
//! ```
//!
//! GPIO 4 is ADC1-capable on ESP32-C3 and is a convenient non-strapping pin on
//! common ESP32-C3 development boards.
//! Keep the signal between GND and 3V3.
//! Avoid feeding 5 V into the ADC pin.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_poti
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_poti
//! ```

use esp_idf_hal::{
    adc::{
        attenuation::DB_12,
        oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
    },
    delay::FreeRtos,
    peripherals::Peripherals,
};
use tamer::analog::{AnalogCalibration, AnalogRange, AnalogValue};

const ADC_MAX: u16 = 4095;
const DEADBAND_COUNTS: u16 = 32;
const CALIBRATION_SAMPLES: u16 = 200;
const CALIBRATION_DELAY_MS: u32 = 25;
const MIN_CALIBRATION_SPAN: u16 = 512;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let adc = AdcDriver::new(peripherals.adc1)?;
    let config = AdcChannelConfig {
        attenuation: DB_12,
        ..Default::default()
    };
    let mut poti_pin = AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &config)?;
    let default_range = AnalogRange::zero_to(ADC_MAX);

    log::info!("Waiting for first ADC sample on GPIO 4...");

    let initial_raw = loop {
        match adc.read_raw(&mut poti_pin) {
            Ok(raw) => break raw,
            Err(err) => {
                log::warn!("ADC read failed while initializing; retrying: {:?}", err);
                FreeRtos::delay_ms(250);
            }
        }
    };

    log::info!(
        "Calibration: turn the potentiometer end-to-end for {} seconds.",
        (u32::from(CALIBRATION_SAMPLES) * CALIBRATION_DELAY_MS) / 1000
    );

    let mut calibration = AnalogCalibration::from_sample(initial_raw);
    let mut calibration_read_failed = false;

    for _ in 0..CALIBRATION_SAMPLES {
        match adc.read_raw(&mut poti_pin) {
            Ok(raw) => {
                if calibration_read_failed {
                    log::info!("ADC read recovered during calibration");
                    calibration_read_failed = false;
                }

                calibration.observe(raw);
            }
            Err(err) => {
                if !calibration_read_failed {
                    log::warn!(
                        "ADC read failed during calibration; suppressing repeated failures: {:?}",
                        err
                    );
                    calibration_read_failed = true;
                }
            }
        }

        FreeRtos::delay_ms(CALIBRATION_DELAY_MS);
    }

    let range = if let Some(range) = calibration.range_with_min_span(MIN_CALIBRATION_SPAN) {
        log::info!("Calibration accepted; using calibrated range.");
        range
    } else {
        log::info!(
            "Calibration span below {} counts; falling back to full ADC range.",
            MIN_CALIBRATION_SPAN
        );
        default_range
    };
    let deadband = range.raw_delta_to_normalized(DEADBAND_COUNTS);

    log::info!(
        "Calibration raw min={:?} max={:?} span={:?}; using range {}..{}",
        calibration.min(),
        calibration.max(),
        calibration.span(),
        range.min(),
        range.max()
    );

    let mut poti = AnalogValue::new(initial_raw, range, deadband);
    let initial = poti.stable_value();
    let mut read_failed = false;

    log::info!(
        "Potentiometer ready on GPIO 4: raw={} normalized={} percent={}%",
        initial.raw(),
        initial.normalized(),
        initial.percent()
    );

    loop {
        match adc.read_raw(&mut poti_pin) {
            Ok(raw) => {
                if read_failed {
                    log::info!("ADC read recovered");
                    read_failed = false;
                }

                if let Some(sample) = poti.update(raw) {
                    log::info!(
                        "raw={} normalized={} percent={}%",
                        sample.raw(),
                        sample.normalized(),
                        sample.percent()
                    );
                }
            }
            Err(err) => {
                if !read_failed {
                    log::warn!("ADC read failed; suppressing repeated failures: {:?}", err);
                    read_failed = true;
                }
            }
        }

        FreeRtos::delay_ms(25);
    }
}
