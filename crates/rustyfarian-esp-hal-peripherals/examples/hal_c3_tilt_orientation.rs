//! ESP32-C3 - Tilt Switch Orientation Sensor
//!
//! Minimal example for a ball-in-cylinder digital tilt switch.
//! Reads the switch through an internal pull-up, debounces it with
//! [`DigitalPresence`], and prints horizontal / vertical transitions via
//! `esp-println` using its auto console transport.
//! On C3/C6/S3 boards this follows the active USB Serial/JTAG monitor when
//! present, and otherwise falls back to UART.
//!
//! This exercises the pure [`tamer::presence`] logic over a real esp-hal pin.
//!
//! A single tilt switch is a binary threshold sensor, not a full IMU.
//! Mount it so the switch closes in the orientation you want to call vertical.
//! Rotate it slowly and let the ball settle before reading the state.
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
//! This example treats LOW as [`Presence::Present`], meaning "vertical".
//! If your mounting or module output is reversed, swap the labels or change
//! [`Polarity::ActiveLow`] to [`Polarity::ActiveHigh`].
//!
//! GPIO 4 is a convenient non-strapping general-purpose pin on common ESP32-C3
//! dev boards.
//! Adjust the pin for your board if needed.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART bridge.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_tilt_orientation
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_tilt_orientation
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

const DEBOUNCE_MS: u64 = 80;

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

    let mut orientation = DigitalPresence::new(tilt.is_high(), Polarity::ActiveLow, DEBOUNCE_MS);
    let delay = Delay::new();

    println!("Tilt orientation ready on GPIO 4 - rotate slowly to test.");

    loop {
        let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

        match orientation.update(tilt.is_high(), now_ms) {
            Some(Presence::Present) => {
                println!("  t={} ms  vertical", now_ms);
            }
            Some(Presence::Absent) => {
                println!("  t={} ms  horizontal", now_ms);
            }
            None => {}
        }

        delay.delay_millis(1u32);
    }
}
