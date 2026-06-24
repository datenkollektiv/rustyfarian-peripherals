//! ESP32-C6 — B3F Tactile Push Button
//!
//! Minimal example for a single B3F-style momentary tactile push button
//! (e.g. Omron B3F-1000, 6×6 mm 4-leg through-hole).
//! Reads the button through an internal pull-up, debounces it with
//! [`EdgeDetector`], and prints press / release events plus a running press
//! count via `esp-println` using its auto console transport.
//! On C3/C6/S3 boards this follows the active USB Serial/JTAG monitor when
//! present, and otherwise falls back to UART.
//!
//! This exercises the pure [`tamer::debounce`] logic over a real esp-hal pin
//! (which implements `embedded_hal::digital::InputPin`).
//!
//! ## Components
//!
//! - ESP32-C6 development board (e.g. ESP32-C6-DevKitC-1)
//! - 1 × B3F tactile push button (4-leg)
//!
//! ## Wiring
//!
//! A B3F has four legs: the two legs on the same side are internally shorted,
//! the two pairs are bridged when the button is pressed. Use one leg from each
//! pair — they form a simple SPST momentary switch.
//!
//! ```text
//! B3F leg     ESP32-C6
//! ───────     ────────
//! one leg     GPIO 4
//! other leg   GND
//! ```
//!
//! The internal pull-up keeps GPIO 4 HIGH at rest. Pressing the button shorts
//! the line to GND, pulling it LOW.
//!
//! GPIO 4 is a convenient general-purpose pin on common ESP32-C6 dev boards.
//! Adjust the pin for your board if needed.
//! Avoid strapping pins and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c6_b3f
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c6_b3f
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
use tamer::debounce::{Edge, EdgeDetector};

const DEBOUNCE_MS: u64 = 15;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let button = Input::new(
        peripherals.GPIO4,
        InputConfig::default().with_pull(Pull::Up),
    );

    // Pull-up keeps the line HIGH at rest.
    let mut edge = EdgeDetector::new(true, DEBOUNCE_MS);
    let delay = Delay::new();
    let mut press_count: u32 = 0;

    println!("B3F button ready on GPIO 4 — press to test.");

    loop {
        let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

        match edge.update(button.is_high(), now_ms) {
            Some(Edge::Falling) => {
                // Active-low: falling edge = button pressed.
                press_count = press_count.wrapping_add(1);
                println!("  t={} ms  pressed  (count={})", now_ms, press_count);
            }
            Some(Edge::Rising) => {
                // Active-low: rising edge = button released.
                println!("  t={} ms  released", now_ms);
            }
            None => {}
        }

        // Poll at 1 ms so the debouncer can observe the input remaining stable
        // across the 15 ms debounce window (caller controls the clock).
        delay.delay_millis(1u32);
    }
}
