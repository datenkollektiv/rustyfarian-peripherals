//! ESP32-C3 — Potentiometer on ADC1
//!
//! Minimal analog-input example for a single potentiometer.
//! Reads the wiper through ADC1, normalizes the raw 12-bit reading with
//! [`AnalogValue`], and prints only when the value changes by a meaningful
//! deadband.
//!
//! This exercises the pure [`tamer::analog`] logic over a real esp-hal ADC
//! read.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
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
//! just build-example hal_c3_poti
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_poti
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
use tamer::analog::{AnalogRange, AnalogValue};

const ADC_MAX: u16 = 4095;
const DEADBAND_COUNTS: u16 = 32;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let mut adc1_config = AdcConfig::new();
    let mut poti_pin = adc1_config.enable_pin(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);
    let delay = Delay::new();
    let range = AnalogRange::zero_to(ADC_MAX);
    let deadband = range.raw_delta_to_normalized(DEADBAND_COUNTS);

    println!("Waiting for first ADC sample on GPIO 4...");

    let initial_raw = loop {
        match nb::block!(adc1.read_oneshot(&mut poti_pin)) {
            Ok(raw) => break raw,
            Err(_) => {
                println!("ADC read failed while initializing; retrying");
                delay.delay_millis(250u32);
            }
        }
    };

    let mut poti = AnalogValue::new(initial_raw, range, deadband);
    let initial = poti.stable_value();
    let mut read_failed = false;

    println!(
        "Potentiometer ready on GPIO 4: raw={} normalized={} percent={}%",
        initial.raw(),
        initial.normalized(),
        initial.percent()
    );

    loop {
        match nb::block!(adc1.read_oneshot(&mut poti_pin)) {
            Ok(raw) => {
                if read_failed {
                    println!("ADC read recovered");
                    read_failed = false;
                }

                if let Some(sample) = poti.update(raw) {
                    println!(
                        "raw={} normalized={} percent={}%",
                        sample.raw(),
                        sample.normalized(),
                        sample.percent()
                    );
                }
            }
            Err(_) => {
                if !read_failed {
                    println!("ADC read failed; suppressing repeated failures");
                    read_failed = true;
                }
            }
        }

        delay.delay_millis(25u32);
    }
}
