//! ESP32-C3 — I2C Bus Scanner (ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_i2c_scan`.
//! Bring-up diagnostic that probes every 7-bit I2C address in the valid range
//! (`0x08..=0x77`) and logs every address that ACKs.
//! This is not a peripheral driver — it has no decode/render logic to
//! delegate to `tamer`, so this example does not depend on it.
//! Use it to confirm wiring and discover device addresses before writing a
//! real driver example (e.g. an MPU6050 accelerometer/gyro).
//!
//! Each address is probed with a minimal zero-byte write: `esp-idf-hal`'s
//! [`I2cDriver::write`] issues a START, the 7-bit address with the write bit,
//! and a STOP, without ever transmitting an actual data byte.
//! A device that is present on the bus pulls SDA low to ACK its address; a
//! missing device causes the underlying `i2c_master_cmd_begin` call to fail
//! with `ESP_FAIL` (the legacy I2C driver's NACK code), surfaced here as an
//! [`EspError`](esp_idf_hal::sys::EspError).
//! Any other error code (e.g. `ESP_ERR_TIMEOUT` from a stuck/shorted bus) is
//! a genuine bus fault, not an absent device, and is logged rather than
//! silently swallowed — otherwise a scan of a broken bus would misreport
//! "no devices found".
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), not
//! `println!`.
//! Bare `println!` writes to a buffered newlib stdout that never flushes
//! inside this example's scan-then-idle loop, so nothing would reach the
//! serial monitor.
//! `log::info!` uses ESP-IDF's unbuffered `esp_log` path.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
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
//! GPIO 4 / GPIO 5 are convenient non-strapping general-purpose pins on
//! common ESP32-C3 dev boards, reused here (and in the upcoming MPU6050
//! example) as the shared SDA/SCL pair.
//! Avoid GPIO 8 / GPIO 9 and any pins used by your board console or USB-UART
//! bridge.
//! Most breakout boards already carry SDA/SCL pull-ups; if using a bare
//! sensor IC, add external 4.7 kΩ pull-ups to 3V3.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_i2c_scan
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_i2c_scan
//! ```

use esp_idf_hal::{
    delay::{FreeRtos, BLOCK},
    i2c::{config::Config, I2cDriver},
    peripherals::Peripherals,
    sys::ESP_FAIL,
    units::FromValueType,
};

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let config = Config::new().baudrate(100.kHz().into());
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio4,
        peripherals.pins.gpio5,
        &config,
    )?;

    log::info!("Scanning I2C bus 0x08..=0x77 @ 100 kHz (SDA=GPIO4, SCL=GPIO5)...");

    let mut found = 0u32;
    let mut bus_errors = 0u32;
    for address in 0x08..=0x77u8 {
        // A zero-length write is a minimal address-only probe: esp-idf-hal
        // still issues START + address byte + STOP and reports whether the
        // device acknowledged its address, without transmitting any data
        // byte.
        match i2c.write(address, &[], BLOCK) {
            Ok(()) => {
                log::info!("found device at 0x{:02X}", address);
                found += 1;
            }
            Err(err) if err.code() == ESP_FAIL => {
                // No device at this address — expected for most of the range.
                // The legacy `i2c_master_cmd_begin` driver reports a NACK
                // (missing/unresponsive address) as `ESP_FAIL`.
            }
            Err(err) => {
                // Any other code (e.g. ESP_ERR_TIMEOUT) is a genuine bus
                // fault — missing pull-ups, a shorted line, or wrong pins —
                // not an absent device. Surface it instead of silently
                // reporting "no devices found".
                log::warn!("bus error probing 0x{:02X}: {:?}", address, err);
                bus_errors += 1;
            }
        }
    }

    if found == 0 {
        log::info!("no devices found");
    } else {
        log::info!("scan complete: {} device(s) found", found);
    }
    if bus_errors > 0 {
        // A shorted line or missing pull-ups makes every probe fault, so the
        // count above alone would read as a clean "no devices found". Flag the
        // bus faults explicitly so a broken bus isn't mistaken for an empty one.
        log::warn!(
            "{} bus error(s) during scan — check wiring/pull-ups",
            bus_errors
        );
    }

    loop {
        FreeRtos::delay_ms(1000);
    }
}
