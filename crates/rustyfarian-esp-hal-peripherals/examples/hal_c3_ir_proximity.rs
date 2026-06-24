//! ESP32-C3 — IR Proximity Sensor
//!
//! Minimal example for a digital reflective IR proximity sensor module.
//! Reads the module output, debounces it with [`DigitalPresence`], and prints
//! floor / line transitions via `esp-println` using its auto console transport.
//! On C3/C6/S3 boards this follows the active USB Serial/JTAG monitor when
//! present, and otherwise falls back to UART.
//!
//! This exercises the pure [`tamer::presence`] logic over a real esp-hal pin.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
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
//! GPIO 4 is a convenient non-strapping general-purpose pin on common ESP32-C3
//! dev boards.
//! Adjust the pin for your board if needed.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_ir_proximity
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_ir_proximity
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

const DEBOUNCE_MS: u64 = 5;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let sensor = Input::new(
        peripherals.GPIO4,
        InputConfig::default().with_pull(Pull::None),
    );

    let mut detector = DigitalPresence::new(sensor.is_high(), Polarity::ActiveHigh, DEBOUNCE_MS);
    let delay = Delay::new();

    println!("IR proximity sensor ready on GPIO 4 — move it over a line to test.");

    loop {
        let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

        match detector.update(sensor.is_high(), now_ms) {
            Some(Presence::Present) => {
                println!("  t={} ms  present", now_ms);
            }
            Some(Presence::Absent) => {
                println!("  t={} ms  absent", now_ms);
            }
            None => {}
        }

        delay.delay_millis(1u32);
    }
}
