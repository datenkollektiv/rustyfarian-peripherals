//! ESP32-C3 — KY-003 / A3144 Unipolar Hall Switch (Digital Presence)
//!
//! Minimal example for a KY-003 module built around an A3144
//! (`3144EUA-S`/`3144LUA-S`) unipolar, open-collector, Schmitt-trigger digital
//! Hall switch.
//! Reads the switch through an internal pull-up, debounces it with
//! [`DigitalPresence`], and prints present / absent transitions via
//! `esp-println` using its auto console transport.
//! On C3/C6/S3 boards this follows the active USB Serial/JTAG monitor when
//! present, and otherwise falls back to UART.
//!
//! This exercises the pure [`tamer::presence`] logic over a real esp-hal pin.
//!
//! ## Hardware truths
//!
//! - This is a **DIGITAL switch**, NOT a linear analog sensor: it idles HIGH
//!   (held there by the module's onboard pull-up) and its output snaps LOW
//!   when a strong-enough field of the correct polarity is present.
//! - It is **UNIPOLAR**: only the pole facing the marked face of the sensor
//!   triggers it.
//!   The opposite magnetic pole produces no response at all — that is
//!   unipolar behavior by design, not a tuning issue.
//! - For linear / bipolar / field-strength sensing you need a real
//!   49E-class sensor — see the `hal_c3_hall_linear` example instead.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-MINI-1, ESP32-C3 SuperMini,
//!   ESP32-C3-DevKitM-1)
//! - 1 × KY-003 / A3144 Hall switch module
//! - 1 × magnet
//!
//! ## Wiring
//!
//! ```text
//! KY-003 module   ESP32-C3
//! -------------   --------
//! VCC             3V3
//! GND             GND
//! DO (or OUT)     GPIO 4
//! ```
//!
//! The internal pull-up and the module's own onboard pull-up both keep
//! GPIO 4 HIGH while idle.
//! A magnet of the correct polarity, held close to the marked face, drives
//! DO LOW.
//! This example treats LOW as [`Presence::Present`].
//!
//! GPIO 4 is a convenient non-strapping general-purpose pin on common ESP32-C3
//! dev boards.
//! Adjust the pin for your board if needed.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_hall_switch
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_hall_switch
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

    let hall = Input::new(
        peripherals.GPIO4,
        InputConfig::default().with_pull(Pull::Up),
    );

    let mut presence = DigitalPresence::new(hall.is_high(), Polarity::ActiveLow, DEBOUNCE_MS);
    let delay = Delay::new();

    println!("Hall switch ready on GPIO 4 — bring the magnet's marked pole close to test.");

    loop {
        let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

        match presence.update(hall.is_high(), now_ms) {
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
