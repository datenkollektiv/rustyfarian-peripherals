//! ESP32-C3 - Tilt Switch Motion Sensor (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_tilt_motion`.
//! Reads a ball-in-cylinder digital tilt switch through an internal pull-up,
//! debounces it with [`DigitalPresence`], and logs motion bursts via
//! `log::info!`.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), not
//! `println!`.
//! Bare `println!` writes to a buffered newlib stdout that never flushes inside
//! this infinite loop, so nothing would reach the serial monitor.
//! `log::info!` uses ESP-IDF's unbuffered `esp_log` path.
//!
//! A tilt switch chatters when it is moved because the ball repeatedly opens
//! and closes the contact.
//! This example treats several debounced transitions inside a short window as
//! motion, then reports quiet after the transitions stop.
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
//! Polarity does not matter much for motion detection, because both directions
//! are counted as movement.
//!
//! GPIO 4 is a convenient non-strapping pin on common ESP32-C3 dev boards.
//! Adjust it for your board.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_tilt_motion
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_tilt_motion
//! ```

use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{PinDriver, Pull},
    peripherals::Peripherals,
};
use std::time::Instant;
use tamer::presence::{DigitalPresence, Polarity, Presence};

const DEBOUNCE_MS: u64 = 8;
const MOTION_WINDOW_MS: u64 = 1_000;
const MOTION_EDGE_THRESHOLD: u8 = 4;
const QUIET_MS: u64 = 1_200;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let tilt = PinDriver::input(peripherals.pins.gpio4, Pull::Up)?;

    let mut detector = DigitalPresence::new(tilt.is_high(), Polarity::ActiveLow, DEBOUNCE_MS);
    let mut window_started_ms = 0;
    let mut transition_count: u8 = 0;
    let mut last_transition_ms = 0;
    let mut moving = false;
    let start = Instant::now();

    log::info!("Tilt motion ready on GPIO 4 - shake or tap the sensor to test.");

    loop {
        let now_ms = start.elapsed().as_millis() as u64;

        if let Some(state) = detector.update(tilt.is_high(), now_ms) {
            if now_ms.saturating_sub(window_started_ms) > MOTION_WINDOW_MS {
                window_started_ms = now_ms;
                transition_count = 0;
            }

            transition_count = transition_count.saturating_add(1);
            last_transition_ms = now_ms;

            let level = match state {
                Presence::Present => "closed",
                Presence::Absent => "open",
            };

            log::info!(
                "t={} ms  transition={} count={}",
                now_ms,
                level,
                transition_count
            );

            if !moving && transition_count >= MOTION_EDGE_THRESHOLD {
                moving = true;
                log::info!("t={} ms  motion", now_ms);
            }
        }

        if moving && now_ms.saturating_sub(last_transition_ms) >= QUIET_MS {
            moving = false;
            transition_count = 0;
            window_started_ms = now_ms;
            log::info!("t={} ms  quiet", now_ms);
        }

        FreeRtos::delay_ms(1);
    }
}
