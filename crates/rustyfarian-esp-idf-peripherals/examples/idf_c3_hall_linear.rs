//! ESP32-C3 — 49E Linear Hall Effect Sensor (HW-477) — ESP-IDF baseline
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_hall_linear`.
//! This example is for a **49E-class *linear analog* (bipolar)** Hall sensor
//! (output rests at ~VCC/2, swings with pole and field strength). For a
//! **KY-003 / A3144 unipolar digital Hall *switch*** use the bare-metal
//! `hal_c3_hall_switch` example, which reads it as a debounced digital input.
//! Reads the analog output of a 49E linear Hall effect sensor via ADC1,
//! smooths the signal with
//! [`SlidingAverage`](tamer::smoothing::SlidingAverage), evaluates presence
//! via [`HallSensor`](tamer::hall::HallSensor), and logs diagnostics every
//! 200 ms.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), matching
//! the other ESP-IDF examples in this crate.
//!
//! ## How it works
//!
//! 1. **Calibration** — reads 50 samples over ~1 s with no magnet present
//!    to establish the ADC midpoint.
//! 2. **Detection loop** — continuously reads the ADC, smooths with
//!    [`SlidingAverage<8>`](tamer::smoothing::SlidingAverage), evaluates
//!    presence via [`HallSensor`](tamer::hall::HallSensor), and logs
//!    diagnostics every 200 ms.
//!
//! Adjust `THRESHOLD` to match your magnet strength and distance.
//! A lower value is more sensitive; start low and increase until false
//! positives disappear.
//!
//! ## Smoothing
//!
//! The detection loop uses [`SlidingAverage<8>`](tamer::smoothing::SlidingAverage)
//! to dampen ESP32-C3 ADC quantization noise.
//! This matches the bare-metal baseline (`hal_c3_hall_linear`) so the two
//! examples behave consistently.
//! Both raw and smoothed values are logged — read the smoothed (`avg`) column
//! when tuning `THRESHOLD`.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 49E linear Hall effect sensor (HW-477 v0.2)
//!
//! ## Wiring
//!
//! ```text
//! HW-477 pin  Signal  ESP32-C3
//! ──────────  ──────  ────────
//! VCC         3.3 V   3V3
//! GND         GND     GND
//! OUT         Analog  GPIO 4
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
//! just build-example idf_c3_hall_linear
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_hall_linear
//! ```

use esp_idf_hal::{
    adc::{
        attenuation::DB_12,
        oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
    },
    delay::FreeRtos,
    peripherals::Peripherals,
};
use tamer::{hall::HallSensor, presence::Presence, smoothing::SlidingAverage};

/// Minimum absolute deviation from midpoint to report Present.
/// Start low for debugging — increase once you see real sensor values.
const THRESHOLD: u16 = 100;

/// Number of no-magnet samples used for midpoint calibration.
const CAL_SAMPLES: usize = 50;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let adc = AdcDriver::new(peripherals.adc1)?;
    let config = AdcChannelConfig {
        attenuation: DB_12,
        ..Default::default()
    };
    let mut pin = AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &config)?;

    // --- Calibration phase ---
    // Read samples with no magnet present to find the resting midpoint.
    log::info!(
        "Calibrating — keep magnet away ({} samples)...",
        CAL_SAMPLES
    );

    let mut cal_buf = [0u16; CAL_SAMPLES];
    let mut cal_read_failed = false;

    for sample in cal_buf.iter_mut() {
        loop {
            match adc.read_raw(&mut pin) {
                Ok(raw) => {
                    if cal_read_failed {
                        log::info!("ADC read recovered during calibration");
                        cal_read_failed = false;
                    }
                    *sample = raw;
                    break;
                }
                Err(err) => {
                    if !cal_read_failed {
                        log::warn!(
                            "ADC read failed during calibration; suppressing repeated failures: {:?}",
                            err
                        );
                        cal_read_failed = true;
                    }
                }
            }
        }
        FreeRtos::delay_ms(20);
    }

    let mut sensor = HallSensor::new(THRESHOLD, 2048);
    if let Err(e) = sensor.calibrate_from_samples(&cal_buf) {
        log::warn!(
            "Calibration error: {:?}; continuing with default midpoint",
            e
        );
    }

    log::info!(
        "Calibrated: midpoint={}, threshold={}",
        sensor.midpoint(),
        THRESHOLD,
    );

    // --- Detection loop ---
    let mut smoother = SlidingAverage::<8>::new();
    let mut read_failed = false;

    loop {
        match adc.read_raw(&mut pin) {
            Ok(raw) => {
                if read_failed {
                    log::info!("ADC read recovered");
                    read_failed = false;
                }

                let smoothed = smoother.push(raw);
                let dev = sensor.deviation(smoothed);
                let presence = sensor.evaluate(smoothed);

                let tag = match presence {
                    Presence::Present if smoothed > sensor.midpoint() => "SOUTH",
                    Presence::Present => "NORTH",
                    Presence::Absent => "---",
                };

                log::info!(
                    "raw={:<5} avg={:<5} mid={:<5} dev={:<5} {}",
                    raw,
                    smoothed,
                    sensor.midpoint(),
                    dev,
                    tag,
                );
            }
            Err(err) => {
                if !read_failed {
                    log::warn!("ADC read failed; suppressing repeated failures: {:?}", err);
                    read_failed = true;
                }
            }
        }

        FreeRtos::delay_ms(200);
    }
}
