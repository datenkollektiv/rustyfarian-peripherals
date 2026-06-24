//! ESP32-C3 — Digital Touch Button Presence Sensor (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_touch`.
//! Reads a digital capacitive touch button module output, debounces it with
//! [`DigitalPresence`], and logs touched / released transitions via
//! `log::info!`.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), not
//! `println!`.
//! Bare `println!` writes to a buffered newlib stdout that never flushes inside
//! this infinite loop, so nothing would reach the serial monitor.
//! `log::info!` uses ESP-IDF's unbuffered `esp_log` path.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 1 × digital capacitive touch button module with 3.3 V-safe output
//!
//! ## Wiring
//!
//! ```text
//! Touch module   ESP32-C3
//! ------------   --------
//! VCC            3V3
//! GND            GND
//! DO             GPIO 4
//! ```
//!
//! Many touch modules drive DO HIGH while touched and LOW while idle.
//! This example treats HIGH as [`Presence::Present`].
//! If your module drives LOW while touched, change [`Polarity::ActiveHigh`] to
//! [`Polarity::ActiveLow`].
//!
//! GPIO 4 is a convenient non-strapping pin on common ESP32-C3 dev boards.
//! Adjust it for your board.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_touch
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_touch
//! ```

use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{PinDriver, Pull},
    peripherals::Peripherals,
};
use std::time::Instant;
use tamer::presence::{DigitalPresence, Polarity, Presence};

const DEBOUNCE_MS: u64 = 20;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let touch = PinDriver::input(peripherals.pins.gpio4, Pull::Down)?;

    let mut presence = DigitalPresence::new(touch.is_high(), Polarity::ActiveHigh, DEBOUNCE_MS);
    let start = Instant::now();

    log::info!("Touch button ready on GPIO 4 — touch the pad to test.");

    loop {
        let now_ms = start.elapsed().as_millis() as u64;

        match presence.update(touch.is_high(), now_ms) {
            Some(Presence::Present) => {
                log::info!("t={} ms  touched", now_ms);
            }
            Some(Presence::Absent) => {
                log::info!("t={} ms  released", now_ms);
            }
            None => {}
        }

        FreeRtos::delay_ms(1);
    }
}
