//! ESP32-C3 — B3F Tactile Push Button (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_b3f`.
//! Reads a B3F-style momentary tactile push button through an internal pull-up,
//! debounces it with [`EdgeDetector`], and logs press / release events plus a
//! running press count via `log::info!`.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), not
//! `println!`: bare `println!` writes to a buffered newlib stdout that never
//! flushes inside this infinite loop, so nothing would reach the serial
//! monitor. `log::info!` uses ESP-IDF's unbuffered `esp_log` path — the same
//! one the boot messages use.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 1 × B3F tactile push button (4-leg)
//!
//! ## Wiring
//!
//! ```text
//! B3F leg     ESP32-C3
//! ───────     ────────
//! one leg     GPIO 4
//! other leg   GND
//! ```
//!
//! The internal pull-up keeps GPIO 4 HIGH at rest.
//! Pressing the button shorts the line to GND, pulling it LOW.
//!
//! GPIO 4 is a convenient non-strapping pin on common ESP32-C3 dev boards
//! (DevKitM-1, SuperMini); adjust it for your board. Avoid GPIO 8 / GPIO 9
//! (strapping / BOOT). The internal pull-up (~45 kΩ) is enough for a button on
//! a short lead; a long or noisy lead may want an external pull-up. If a board
//! variant lacks an internal pull-up on the chosen pin, add an external one and
//! drop `Pull::Up` to `Pull::Floating`.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_b3f
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_b3f
//! ```

use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{PinDriver, Pull},
    peripherals::Peripherals,
};
use std::time::Instant;
use tamer::debounce::{Edge, EdgeDetector};

const DEBOUNCE_MS: u64 = 15;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    // Route output through ESP-IDF's unbuffered logger; see the module docs for
    // why `println!` would be silent here.
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let button = PinDriver::input(peripherals.pins.gpio4, Pull::Up)?;

    // Pull-up keeps the line HIGH at rest.
    let mut edge = EdgeDetector::new(true, DEBOUNCE_MS);
    let mut press_count: u32 = 0;
    let start = Instant::now();

    log::info!("B3F button ready on GPIO 4 — press to test.");

    loop {
        // as_millis() returns u128; safe to truncate — u64 overflows after ~585M years.
        let now_ms = start.elapsed().as_millis() as u64;

        match edge.update(button.is_high(), now_ms) {
            Some(Edge::Falling) => {
                press_count = press_count.wrapping_add(1);
                log::info!("t={} ms  pressed  (count={})", now_ms, press_count);
            }
            Some(Edge::Rising) => {
                log::info!("t={} ms  released", now_ms);
            }
            None => {}
        }

        // Poll at 1 ms — use FreeRtos::delay_ms so we yield to the scheduler.
        // Busy-wait style sleeps can starve IDLE and trigger the watchdog.
        FreeRtos::delay_ms(1);
    }
}
