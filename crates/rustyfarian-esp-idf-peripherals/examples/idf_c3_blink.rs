//! ESP32-C3 — Blink a single LED on GPIO7 (ESP-IDF / `std`)
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
//! just build-example idf_c3_blink
//! just flash idf_c3_blink
//! ```

use esp_idf_hal::{delay::FreeRtos, gpio::PinDriver, peripherals::Peripherals};

const BLINK_INTERVAL_MS: u32 = 1_000;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();

    let peripherals = Peripherals::take()?;
    let mut led = PinDriver::output(peripherals.pins.gpio7)?;
    led.set_low()?;

    loop {
        led.set_high()?;
        FreeRtos::delay_ms(BLINK_INTERVAL_MS);
        led.set_low()?;
        FreeRtos::delay_ms(BLINK_INTERVAL_MS);
    }
}
