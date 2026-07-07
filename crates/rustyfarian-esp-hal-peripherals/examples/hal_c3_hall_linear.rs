//! ESP32-C3 — 49E Linear Hall Effect Sensor (HW-477)
//!
//! This example is for a **49E-class *linear analog* (bipolar)** Hall sensor:
//! its output rests at ~VCC/2 with no field and swings up or down depending on
//! the magnet's pole and field strength, read here via the ADC.
//! If your part is instead a **KY-003 / A3144 unipolar digital Hall *switch***
//! (open-collector, idles HIGH, responds to only one pole), use
//! `hal_c3_hall_switch` instead — the ADC deviation model below is the wrong
//! abstraction for a snapping digital output.
//!
//! Diagnostic example for tuning magnet detection with a 49E linear Hall
//! effect sensor.
//! Reads the analog output via ADC1 and prints raw values, deviation from
//! midpoint, and presence state so you can observe the sensor's behaviour and
//! find a working threshold.
//!
//! ## How it works
//!
//! 1. **Calibration** — reads 50 samples over ~1 s with no magnet present
//!    to establish the ADC midpoint.
//! 2. **Detection loop** — continuously reads the ADC, smooths the signal
//!    with [`SlidingAverage`](tamer::smoothing::SlidingAverage), evaluates
//!    presence via [`HallSensor`](tamer::hall::HallSensor), and prints
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
//! This matches the ESP-IDF baseline (`idf_c3_hall_linear`) so the two
//! examples behave consistently.
//! Both raw and smoothed values are printed — read the smoothed (`avg`) column
//! when tuning `THRESHOLD`.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
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
//! just build-example hal_c3_hall_linear
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_hall_linear
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    delay::Delay,
    main,
};
use esp_println::println;
use tamer::{hall::HallSensor, presence::Presence, smoothing::SlidingAverage};

/// Minimum absolute deviation from midpoint to report Present.
/// Start low for debugging — increase once you see real sensor values.
const THRESHOLD: u16 = 100;

/// Number of no-magnet samples used for midpoint calibration.
const CAL_SAMPLES: usize = 50;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    // --- ADC setup ---
    // 11 dB attenuation gives full 0–3.3 V range (12-bit: 0–4095).
    let mut adc1_config = AdcConfig::new();
    let mut pin = adc1_config.enable_pin(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);

    // --- Calibration phase ---
    // Read samples with no magnet present to find the resting midpoint.
    println!(
        "Calibrating — keep magnet away ({} samples)...",
        CAL_SAMPLES
    );

    let mut cal_buf = [0u16; CAL_SAMPLES];
    let mut cal_read_failed = false;

    for sample in cal_buf.iter_mut() {
        loop {
            match nb::block!(adc1.read_oneshot(&mut pin)) {
                Ok(raw) => {
                    if cal_read_failed {
                        println!("ADC read recovered during calibration");
                        cal_read_failed = false;
                    }
                    *sample = raw;
                    break;
                }
                Err(_) => {
                    if !cal_read_failed {
                        println!(
                            "ADC read failed during calibration; suppressing repeated failures"
                        );
                        cal_read_failed = true;
                    }
                }
            }
        }
        delay.delay_millis(20u32);
    }

    let mut sensor = HallSensor::new(THRESHOLD, 2048);
    if let Err(e) = sensor.calibrate_from_samples(&cal_buf) {
        println!(
            "Calibration error: {:?}; continuing with default midpoint",
            e
        );
    }

    println!(
        "Calibrated: midpoint={}, threshold={}",
        sensor.midpoint(),
        THRESHOLD,
    );
    println!();

    // --- Detection loop ---
    let mut smoother = SlidingAverage::<8>::new();
    let mut read_failed = false;

    loop {
        match nb::block!(adc1.read_oneshot(&mut pin)) {
            Ok(raw) => {
                if read_failed {
                    println!("ADC read recovered");
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

                println!(
                    "raw={:<5} avg={:<5} mid={:<5} dev={:<5} {}",
                    raw,
                    smoothed,
                    sensor.midpoint(),
                    dev,
                    tag,
                );
            }
            Err(_) => {
                if !read_failed {
                    println!("ADC read failed; suppressing repeated failures");
                    read_failed = true;
                }
            }
        }

        delay.delay_millis(200u32);
    }
}
