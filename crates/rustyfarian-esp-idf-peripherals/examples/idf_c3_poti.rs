//! ESP32-C3 — Potentiometer on ADC1 (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_poti`.
//! Reads a potentiometer wiper through ADC1, normalizes the raw 12-bit reading
//! with [`AnalogValue`], and logs only when the value changes by a meaningful
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
use tamer::analog::{AnalogRange, AnalogValue};

const ADC_MAX: u16 = 4095;
const DEADBAND_COUNTS: u16 = 32;

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
    let range = AnalogRange::zero_to(ADC_MAX);
    let deadband = range.raw_delta_to_normalized(DEADBAND_COUNTS);

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
