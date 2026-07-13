//! ESP32-C3 — I2C Bus Scanner
//!
//! Bring-up diagnostic that probes every 7-bit I2C address in the valid range
//! (`0x08..=0x77`) and prints every address that ACKs.
//! This is not a peripheral driver — it has no decode/render logic to delegate
//! to `tamer`, so this example does not depend on it.
//! Use it to confirm wiring and discover device addresses before writing a
//! real driver example (e.g. an MPU6050 accelerometer/gyro).
//!
//! Each address is probed with a minimal zero-byte write: `esp-hal`'s I2C
//! master issues a START, the 7-bit address with the write bit, and a STOP,
//! without ever holding an actual data byte.
//! A device that is present on the bus pulls SDA low to ACK its address;
//! [`Error::AcknowledgeCheckFailed`](esp_hal::i2c::master::Error) means no
//! device answered at that address.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
//! - 1 × I2C device to scan for (e.g. an MPU6050 breakout module)
//! - jumper wires
//!
//! ## Wiring
//!
//! ```text
//! I2C device    ESP32-C3
//! ----------    --------
//! VCC           3V3
//! GND           GND
//! SDA           GPIO 4
//! SCL           GPIO 5
//! ```
//!
//! GPIO 4 / GPIO 5 are convenient non-strapping general-purpose pins on common
//! ESP32-C3 dev boards, reused here (and in the upcoming MPU6050 example) as
//! the shared SDA/SCL pair.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART
//! bridge.
//! Most breakout boards already carry SDA/SCL pull-ups; if using a bare sensor
//! IC, add external 4.7 kΩ pull-ups to 3V3.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_i2c_scan
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_i2c_scan
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{
    delay::Delay,
    i2c::master::{Config, Error, I2c},
    main,
    time::Rate,
};
use esp_println::println;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let mut i2c = I2c::new(
        peripherals.I2C0,
        Config::default().with_frequency(Rate::from_khz(100)),
    )
    .expect("valid I2C configuration")
    .with_sda(peripherals.GPIO4)
    .with_scl(peripherals.GPIO5);

    let delay = Delay::new();

    println!("Scanning I2C bus 0x08..=0x77 @ 100 kHz (SDA=GPIO4, SCL=GPIO5)...");

    let mut found = 0u32;
    let mut bus_errors = 0u32;
    for address in 0x08..=0x77u8 {
        // A zero-length write is a minimal address-only probe: esp-hal still
        // issues START + address byte + STOP and reports whether the device
        // acknowledged its address, without transmitting any data byte.
        match i2c.write(address, &[]) {
            Ok(()) => {
                println!("  found device at 0x{:02X}", address);
                found += 1;
            }
            Err(Error::AcknowledgeCheckFailed(_)) => {
                // No device at this address — expected for most of the range.
            }
            Err(err) => {
                println!("  error probing 0x{:02X}: {:?}", address, err);
                bus_errors += 1;
            }
        }
    }

    if found == 0 {
        println!("no devices found");
    } else {
        println!("scan complete: {} device(s) found", found);
    }
    if bus_errors > 0 {
        // A shorted line or missing pull-ups makes every probe fault, so the
        // count above alone would read as a clean "no devices found". Flag the
        // bus faults explicitly so a broken bus isn't mistaken for an empty one.
        println!(
            "warning: {} bus error(s) during scan — check wiring/pull-ups",
            bus_errors
        );
    }

    loop {
        delay.delay_millis(1000u32);
    }
}
