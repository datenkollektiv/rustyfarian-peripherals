//! ESP32-C3 — SHT3x Temperature and Humidity
//!
//! Reads an SHT3x (SHT30/SHT31/SHT35) over I2C and prints one single-shot
//! measurement every two seconds: temperature and relative humidity.
//!
//! `tamer` owns everything that can be host-tested: the soft reset and status
//! read ([`Sht3x::init`]), the single-shot command, wait and read sequence
//! ([`Sht3x::measure`]), the CRC check and the fixed-point conversion.
//! This example owns only the hardware: the I2C bus, the delay provider and the
//! console output.
//! No I2C bus error panics; init failures print a hint and park the board,
//! measurement failures are printed once per run via `OnceLatch`.
//!
//! Values are printed in fixed point (hundredths of a degree and of a percent),
//! so no floating-point formatting is linked in.
//! The sensor self-heats slightly when measured often, so the temperature can
//! read a little high.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 1 × SHT3x breakout module (SHT30/SHT31/SHT35, 3.3 V I2C)
//! - jumper wires
//!
//! ## Wiring
//!
//! ```text
//! SHT3x      ESP32-C3
//! -----      --------
//! VCC        3V3
//! GND        GND
//! SDA        GPIO 6
//! SCL        GPIO 7
//! ADDR       GND (address 0x44; tie to 3V3 for 0x45)
//! ```
//!
//! This is the shared I2C0 bus of ADR-004 (SDA GPIO 6, SCL GPIO 7, 100 kHz;
//! pins per ADR-012).
//! Add 4.7 kΩ pull-ups to 3V3 on SDA/SCL if the module lacks them.
//! With ADDR high, set `ADDRESS` to `sht3x::I2C_ADDR_ALT` and rebuild.
//! The same four pins also serve an I2C OLED display, an MPU6050 or a BME680; see
//! `docs/hardware-setup.md`, "Wiring — shared I2C pogo header".
//! Run `hal_c3_i2c_scan` first if the sensor does not answer.
//!
//! ## Expected output
//!
//! ```text
//! SHT3x OK at address 0x44, status 0x8010
//! T: 23.45 degC  RH: 41.20 %
//! ```
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_sht3x
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_sht3x
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{
    delay::Delay,
    i2c::master::{Config, I2c},
    main,
    time::Rate,
};
use esp_println::println;
use tamer::latch::OnceLatch;
use tamer::sht3x::{Error, Repeatability, Sht3x, I2C_ADDR, I2C_ADDR_ALT};

/// Sensor address: ADDR low gives 0x44 (`I2C_ADDR`), ADDR high gives 0x45 (`I2C_ADDR_ALT`).
const ADDRESS: u8 = I2C_ADDR;
/// Pause between measurements in milliseconds.
const PERIOD_MS: u32 = 2000;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

/// Idle forever so a failed bring-up stays visible instead of reboot-looping.
fn park(delay: &Delay) -> ! {
    loop {
        delay.delay_millis(1000u32);
    }
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    // Busy-wait delay: exact, so the driver's conversion wait is never short.
    let mut delay = Delay::new();

    let i2c = match I2c::new(
        peripherals.I2C0,
        Config::default().with_frequency(Rate::from_khz(100)),
    ) {
        Ok(i2c) => i2c.with_sda(peripherals.GPIO6).with_scl(peripherals.GPIO7),
        Err(e) => {
            println!("invalid I2C configuration: {:?}", e);
            park(&delay);
        }
    };

    let mut sensor = Sht3x::new(i2c, ADDRESS);
    match sensor.init(&mut delay) {
        Ok(status) => {
            println!(
                "SHT3x OK at address 0x{:02X}, status 0x{:04X}",
                ADDRESS,
                status.bits()
            );
        }
        Err(e) => {
            if e.is_nack() {
                println!(
                    "no SHT3x answered at 0x{:02X}: check VCC/GND, SDA=GPIO6, SCL=GPIO7 \
                     and ADDR (ADDR high means address 0x{:02X}: try I2C_ADDR_ALT)",
                    ADDRESS, I2C_ADDR_ALT
                );
            } else if matches!(e, Error::Bus(_)) {
                println!("bus fault: {} — check pull-ups / shorted lines", e);
            } else {
                println!("SHT3x init failed: {}", e);
            }
            park(&delay);
        }
    }

    let mut read_failed = OnceLatch::new();
    loop {
        match sensor.measure(&mut delay, Repeatability::High) {
            Ok(reading) => {
                if read_failed.rearm() {
                    println!("measurement recovered");
                }
                // Fixed-point print: split the magnitude so -0.05 keeps its sign.
                let t = reading.temperature_centi_c();
                let sign = if t < 0 { "-" } else { "" };
                let t_abs = t.unsigned_abs();
                let rh = reading.humidity_centi_rh();
                println!(
                    "T: {}{}.{:02} degC  RH: {}.{:02} %",
                    sign,
                    t_abs / 100,
                    t_abs % 100,
                    rh / 100,
                    rh % 100
                );
            }
            Err(e) => {
                if read_failed.trip() {
                    println!("measurement failed: {}", e);
                }
            }
        }
        delay.delay_millis(PERIOD_MS);
    }
}
