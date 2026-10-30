//! ESP32-C3 — MPU6050 Accelerometer Tilt Angle
//!
//! Reads an MPU6050 over I2C and prints a filtered tilt angle about the X axis
//! at roughly 5 Hz.
//! This is not the ball tilt switch of the `hal_c3_tilt_*` examples — it is a
//! 6-axis IMU on the I2C bus from ADR-004.
//!
//! `tamer` owns everything that can be host-tested: the wake-up and
//! configuration sequence ([`Mpu6050::init`]), the `WHO_AM_I` identity check,
//! the still-and-level calibration ([`Mpu6050::calibrate`]), raw-register
//! parsing, the exponential-moving-average filter and the tilt math
//! ([`TiltEstimator`]).
//! This example owns only the hardware: the I2C bus, the delay provider and the
//! console output.
//! No I2C bus error panics; every failure prints a hint and parks the board.
//!
//! The sensor must be **level (Z axis up) and motionless** while the
//! calibration runs (about one second after the identity check).
//! The measured gravity offsets are subtracted from every later sample.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
//! - 1 × MPU6050 breakout module (e.g. GY-521)
//! - jumper wires
//!
//! ## Wiring
//!
//! ```text
//! MPU6050    ESP32-C3
//! -------    --------
//! VCC        3V3
//! GND        GND
//! SDA        GPIO 4
//! SCL        GPIO 5
//! AD0        GND (address 0x68; tie to 3V3 for 0x69)
//! ```
//!
//! This is the shared I2C0 bus of ADR-004 (SDA GPIO 4, SCL GPIO 5, 100 kHz).
//! Most breakout boards already carry SDA/SCL pull-ups; if using a bare sensor
//! IC, add external 4.7 kΩ pull-ups to 3V3.
//! Run `hal_c3_i2c_scan` first if the sensor does not answer.
//!
//! ## Expected output
//!
//! ```text
//! MPU6050 WHO_AM_I OK (0x68) at address 0x68
//! calibrating: keep the board still and level (Z up)...
//! offsets: ...
//! tilt: 0.1°
//! ```
//!
//! The angle reads about 0° when level and about ±90° when the board is tipped
//! about the X axis.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_mpu6050_tilt
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_mpu6050_tilt
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
use tamer::mpu6050::{Error, I2C_ADDR, I2C_ADDR_ALT, WHO_AM_I_VALUE};
use tamer::{Mpu6050, TiltEstimator};

/// Smoothing factor of the tilt filter (higher reacts faster, lower is calmer).
const ALPHA: f32 = 0.2;
/// Number of calibration samples and the pause between them.
const CAL_SAMPLES: u32 = 100;
const CAL_INTERVAL_MS: u32 = 10;
/// Sampling period in milliseconds (about 5 Hz).
const PERIOD_MS: u32 = 200;
/// Print only every Nth consecutive read failure to avoid flooding the console.
const ERROR_PRINT_EVERY: u32 = 25;

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
    let mut delay = Delay::new();

    let i2c = match I2c::new(
        peripherals.I2C0,
        Config::default().with_frequency(Rate::from_khz(100)),
    ) {
        Ok(i2c) => i2c.with_sda(peripherals.GPIO4).with_scl(peripherals.GPIO5),
        Err(e) => {
            println!("invalid I2C configuration: {:?}", e);
            park(&delay);
        }
    };

    let mut imu = Mpu6050::new(i2c, I2C_ADDR);

    match imu.init() {
        Ok(()) => println!(
            "MPU6050 WHO_AM_I OK (0x{:02X}) at address 0x{:02X}",
            WHO_AM_I_VALUE, I2C_ADDR
        ),
        Err(e) if e.is_nack() => {
            println!(
                "no MPU6050 answered at 0x{:02X}: check VCC/GND, SDA=GPIO4, SCL=GPIO5 \
                 and AD0 (AD0 high means address 0x{:02X}: try I2C_ADDR_ALT)",
                I2C_ADDR, I2C_ADDR_ALT
            );
            park(&delay);
        }
        Err(e @ Error::WrongIdentity { .. }) => {
            println!("{}", e);
            park(&delay);
        }
        Err(e) => {
            println!("bus fault: {} — check pull-ups / shorted lines", e);
            park(&delay);
        }
    }

    println!("calibrating: keep the board still and level (Z up)...");
    let offsets = match imu.calibrate(&mut delay, CAL_SAMPLES, CAL_INTERVAL_MS) {
        Ok(offsets) => offsets,
        Err(e) => {
            println!("calibration failed: {}", e);
            park(&delay);
        }
    };
    println!("offsets: {:?}", offsets);

    let mut tilt = TiltEstimator::new(offsets, ALPHA);
    let mut failures = 0u32;

    loop {
        match imu.read_raw() {
            Ok(raw) => {
                failures = 0;
                println!("tilt: {:.1}°", tilt.update(&raw));
            }
            Err(e) => {
                failures = failures.saturating_add(1);
                if failures % ERROR_PRINT_EVERY == 1 {
                    println!("read failed ({} in a row): {}", failures, e);
                }
            }
        }
        delay.delay_millis(PERIOD_MS);
    }
}
