//! ESP32-C3 — Blink a single LED on GPIO7 (esp-hal / `no_std`)
//!
//! Drives an external active-high LED on for one second and off for one second.
//!
//! ## Components
//!
//! - ESP32-C3 development board
//! - 1 x LED
//! - 1 x 330 Ω resistor
//!
//! ## Wiring
//!
//! ```text
//! LED                 ESP32-C3
//! ───                 ────────
//! anode (+)           GPIO 7 through 330 Ω resistor
//! cathode (-)         GND
//! ```
//!
//! ## Build and flash
//!
//! ```sh
//! just build-example hal_c3_blink
//! just flash hal_c3_blink
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
};
use esp_println::println;

const BLINK_INTERVAL_MS: u32 = 1_000;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut led = Output::new(peripherals.GPIO7, Level::Low, OutputConfig::default());
    let delay = Delay::new();

    loop {
        led.set_high();
        delay.delay_millis(BLINK_INTERVAL_MS);
        led.set_low();
        delay.delay_millis(BLINK_INTERVAL_MS);
    }
}
