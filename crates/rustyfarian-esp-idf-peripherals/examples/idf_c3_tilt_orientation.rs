//! ESP32-C3 - Tilt Switch Orientation Sensor (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_tilt_orientation`.
//! Reads a ball-in-cylinder digital tilt switch through an internal pull-up,
//! debounces it with [`DigitalPresence`], and logs horizontal / vertical
//! transitions via `log::info!`.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), not
//! `println!`.
//! Bare `println!` writes to a buffered newlib stdout that never flushes inside
//! this infinite loop, so nothing would reach the serial monitor.
//! `log::info!` uses ESP-IDF's unbuffered `esp_log` path.
//!
//! A single tilt switch is a binary threshold sensor, not a full IMU.
//! Mount it so the switch closes in the orientation you want to call vertical.
//! Rotate it slowly and let the ball settle before reading the state.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 1 x ball-in-cylinder digital tilt switch or 3.3 V-safe tilt module
//!
//! ## Wiring
//!
//! ```text
//! Tilt switch   ESP32-C3
//! -----------   --------
//! one side      GPIO 4
//! other side    GND
//! ```
//!
//! If you use a three-pin module, connect VCC to 3V3, GND to GND, and DO to
//! GPIO 4.
//! The internal pull-up keeps GPIO 4 HIGH while the switch is open.
//! When the ball closes the switch, GPIO 4 is pulled LOW.
//! This example treats LOW as [`Presence::Present`], meaning "vertical".
//! If your mounting or module output is reversed, swap the labels or change
//! [`Polarity::ActiveLow`] to [`Polarity::ActiveHigh`].
//!
//! GPIO 4 is a convenient non-strapping pin on common ESP32-C3 dev boards.
//! Adjust it for your board.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_tilt_orientation
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_tilt_orientation
//! ```

use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{PinDriver, Pull},
    peripherals::Peripherals,
};
use std::time::Instant;
use tamer::presence::{DigitalPresence, Polarity, Presence};

const DEBOUNCE_MS: u64 = 80;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let tilt = PinDriver::input(peripherals.pins.gpio4, Pull::Up)?;

    let mut orientation = DigitalPresence::new(tilt.is_high(), Polarity::ActiveLow, DEBOUNCE_MS);
    let start = Instant::now();

    log::info!("Tilt orientation ready on GPIO 4 - rotate slowly to test.");

    loop {
        let now_ms = start.elapsed().as_millis() as u64;

        match orientation.update(tilt.is_high(), now_ms) {
            Some(Presence::Present) => {
                log::info!("t={} ms  vertical", now_ms);
            }
            Some(Presence::Absent) => {
                log::info!("t={} ms  horizontal", now_ms);
            }
            None => {}
        }

        FreeRtos::delay_ms(1);
    }
}
