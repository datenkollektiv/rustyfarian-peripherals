//! ESP32-C3 — IR Proximity Sensor (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_ir_proximity`.
//! Reads a digital reflective IR proximity sensor module output, debounces it
//! with [`DigitalPresence`], and logs floor / line transitions via
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
//! - 1 × digital reflective IR proximity sensor module with 3.3 V-safe output
//!
//! ## Wiring
//!
//! ```text
//! IR module   ESP32-C3
//! ---------   --------
//! VCC         3V3
//! GND         GND
//! DO          GPIO 4
//! ```
//!
//! Many reflective IR modules actively drive DO HIGH over a bright nearby floor
//! and LOW over a dark or black line.
//! This example treats HIGH as [`Presence::Present`], meaning "floor detected".
//! The black line is [`Presence::Absent`] because it reflects little IR.
//! If your module drives LOW over the floor, change [`Polarity::ActiveHigh`] to
//! [`Polarity::ActiveLow`].
//!
//! GPIO 4 is a convenient non-strapping pin on common ESP32-C3 dev boards.
//! Adjust it for your board.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_ir_proximity
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_ir_proximity
//! ```

use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{PinDriver, Pull},
    peripherals::Peripherals,
};
use std::time::Instant;
use tamer::presence::{DigitalPresence, Polarity, Presence};

const DEBOUNCE_MS: u64 = 5;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sensor = PinDriver::input(peripherals.pins.gpio4, Pull::Floating)?;

    let mut detector = DigitalPresence::new(sensor.is_high(), Polarity::ActiveHigh, DEBOUNCE_MS);
    let start = Instant::now();

    log::info!("IR proximity sensor ready on GPIO 4 — move it over a line to test.");

    loop {
        let now_ms = start.elapsed().as_millis() as u64;

        match detector.update(sensor.is_high(), now_ms) {
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
