//! ESP32-C3 — Reed Switch Presence Sensor (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_reed`.
//! Reads a normally-open magnetic reed switch through an internal pull-up,
//! debounces it with [`DigitalPresence`], and logs present / absent transitions
//! via `log::info!`.
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
//! - 1 × normally-open magnetic reed switch module with 3.3 V-safe output
//! - 1 × magnet
//!
//! ## Wiring
//!
//! ```text
//! Reed module   ESP32-C3
//! -----------   --------
//! VCC           3V3
//! GND           GND
//! DO            GPIO 4
//! ```
//!
//! The internal pull-up keeps GPIO 4 HIGH while the reed switch module is idle.
//! Bringing the magnet close drives DO LOW.
//! This example treats LOW as [`Presence::Present`].
//!
//! GPIO 4 is a convenient non-strapping pin on common ESP32-C3 dev boards.
//! Adjust it for your board.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_reed
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_reed
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
    let reed = PinDriver::input(peripherals.pins.gpio4, Pull::Up)?;

    let mut presence = DigitalPresence::new(reed.is_high(), Polarity::ActiveLow, DEBOUNCE_MS);
    let start = Instant::now();

    log::info!("Reed switch ready on GPIO 4 — move the magnet to test.");

    loop {
        let now_ms = start.elapsed().as_millis() as u64;

        match presence.update(reed.is_high(), now_ms) {
            Some(Presence::Present) => {
                log::info!("t={} ms  present", now_ms);
            }
            Some(Presence::Absent) => {
                log::info!("t={} ms  absent", now_ms);
            }
            None => {}
        }

        FreeRtos::delay_ms(1);
    }
}
