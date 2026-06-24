//! ESP32-C3 - Tilt Switch Motion Sensor
//!
//! Minimal example for using a ball-in-cylinder digital tilt switch as a crude
//! motion or shake sensor.
//! Reads the switch through an internal pull-up, debounces it with
//! [`DigitalPresence`], and reports motion bursts via `esp-println` using its
//! auto console transport.
//! On C3/C6/S3 boards this follows the active USB Serial/JTAG monitor when
//! present, and otherwise falls back to UART.
//!
//! This exercises the pure [`tamer::presence`] logic over a real esp-hal pin.
//!
//! A tilt switch chatters when it is moved because the ball repeatedly opens
//! and closes the contact.
//! This example treats several debounced transitions inside a short window as
//! motion, then reports quiet after the transitions stop.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
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
//! GPIO 4 is a convenient non-strapping general-purpose pin on common ESP32-C3
//! dev boards.
//! Adjust the pin for your board if needed.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_tilt_motion
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_tilt_motion
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Pull},
    main,
    time::Instant,
};
use esp_println::println;
use tamer::presence::{DigitalPresence, Polarity, Presence};

const DEBOUNCE_MS: u64 = 8;
const MOTION_WINDOW_MS: u64 = 1_000;
const MOTION_EDGE_THRESHOLD: u8 = 4;
const QUIET_MS: u64 = 1_200;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let tilt = Input::new(
        peripherals.GPIO4,
        InputConfig::default().with_pull(Pull::Up),
    );

    let mut detector = DigitalPresence::new(tilt.is_high(), Polarity::ActiveLow, DEBOUNCE_MS);
    let mut window_started_ms = 0;
    let mut transition_count: u8 = 0;
    let mut last_transition_ms = 0;
    let mut moving = false;
    let delay = Delay::new();

    println!("Tilt motion ready on GPIO 4 - shake or tap the sensor to test.");

    loop {
        let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

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

            println!(
                "  t={} ms  transition={} count={}",
                now_ms, level, transition_count
            );

            if !moving && transition_count >= MOTION_EDGE_THRESHOLD {
                moving = true;
                println!("  t={} ms  motion", now_ms);
            }
        }

        if moving && now_ms.saturating_sub(last_transition_ms) >= QUIET_MS {
            moving = false;
            transition_count = 0;
            window_started_ms = now_ms;
            println!("  t={} ms  quiet", now_ms);
        }

        delay.delay_millis(1u32);
    }
}
