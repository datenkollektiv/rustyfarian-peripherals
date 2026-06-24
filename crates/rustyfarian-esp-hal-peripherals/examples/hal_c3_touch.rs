//! ESP32-C3 — Digital Touch Button Presence Sensor
//!
//! Minimal example for a digital capacitive touch button module.
//! Reads the module output, debounces it with [`DigitalPresence`], and prints
//! touched / released transitions via `esp-println` using its auto console
//! transport.
//! On C3/C6/S3 boards this follows the active USB Serial/JTAG monitor when
//! present, and otherwise falls back to UART.
//!
//! This exercises the pure [`tamer::presence`] logic over a real esp-hal pin.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
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
//! GPIO 4 is a convenient non-strapping general-purpose pin on common ESP32-C3
//! dev boards.
//! Adjust the pin for your board if needed.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_touch
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_touch
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

const DEBOUNCE_MS: u64 = 20;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let touch = Input::new(
        peripherals.GPIO4,
        InputConfig::default().with_pull(Pull::Down),
    );

    let mut presence = DigitalPresence::new(touch.is_high(), Polarity::ActiveHigh, DEBOUNCE_MS);
    let delay = Delay::new();

    println!("Touch button ready on GPIO 4 — touch the pad to test.");

    loop {
        let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

        match presence.update(touch.is_high(), now_ms) {
            Some(Presence::Present) => {
                println!("  t={} ms  touched", now_ms);
            }
            Some(Presence::Absent) => {
                println!("  t={} ms  released", now_ms);
            }
            None => {}
        }

        delay.delay_millis(1u32);
    }
}
