//! ESP32-C3 — BME680 Temperature, Pressure, Humidity and Gas
//!
//! Reads a BME680 over I2C and prints one forced-mode measurement about every
//! three seconds: temperature, pressure, relative humidity and gas resistance.
//!
//! `tamer` owns everything that can be host-tested: the soft reset, identity and
//! variant check, calibration read ([`Bme680::new`]), the forced-mode conversion
//! sequence ([`Bme680::measure`]) and the compensation maths.
//! This example owns only the hardware: the I2C bus, the delay provider and the
//! console output.
//! No I2C bus error panics; init failures print a hint and park the board,
//! measurement failures are printed once per run via `OnceLatch`.
//!
//! The gas heater runs on every measurement, so the first readings after power-up
//! may report `gas: not valid` until the plate has stabilised.
//! The sensor self-heats slightly, so the temperature reads a little high.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 1 × BME680 breakout module (3.3 V I2C)
//! - jumper wires
//!
//! ## Wiring
//!
//! ```text
//! BME680     ESP32-C3
//! ------     --------
//! VCC        3V3
//! GND        GND
//! SDA        GPIO 6
//! SCL        GPIO 7
//! SDO        GND (address 0x76; tie to 3V3 for 0x77)
//! ```
//!
//! This is the shared I2C0 bus of ADR-004 (SDA GPIO 6, SCL GPIO 7, 100 kHz;
//! pins per ADR-012).
//! Add 4.7 kΩ pull-ups to 3V3 on SDA/SCL if the module lacks them.
//! The same four pins also serve an I2C OLED display or an MPU6050; see
//! `docs/hardware-setup.md`, "Wiring — shared I2C pogo header".
//! Run `hal_c3_i2c_scan` first if the sensor does not answer.
//!
//! ## Expected output
//!
//! ```text
//! BME680 OK at address 0x76
//! T: 23.4 degC  P: 1013.2 hPa  RH: 41.5 %  gas: 52310 ohm
//! ```
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_bme680
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_bme680
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
use tamer::bme680::{Bme680, Error, IirFilter, Oversampling, I2C_ADDR, I2C_ADDR_ALT};
use tamer::latch::OnceLatch;

/// Sensor address: SDO low gives 0x76 (`I2C_ADDR`), SDO high gives 0x77 (`I2C_ADDR_ALT`).
const ADDRESS: u8 = I2C_ADDR;
/// Pause between measurements in milliseconds.
const PERIOD_MS: u32 = 3000;
/// Budget for the driver's own waits per measurement (conversion plus heater).
const TIMEOUT_MS: u32 = 1000;

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
        Ok(i2c) => i2c.with_sda(peripherals.GPIO6).with_scl(peripherals.GPIO7),
        Err(e) => {
            println!("invalid I2C configuration: {:?}", e);
            park(&delay);
        }
    };

    let mut sensor = match Bme680::new(i2c, ADDRESS, &mut delay) {
        Ok(sensor) => {
            println!("BME680 OK at address 0x{:02X}", ADDRESS);
            sensor
        }
        Err((e, _i2c)) => {
            if e.is_nack() {
                println!(
                    "no BME680 answered at 0x{:02X}: check VCC/GND, SDA=GPIO6, SCL=GPIO7 \
                     and SDO (SDO high means address 0x{:02X}: try I2C_ADDR_ALT)",
                    ADDRESS, I2C_ADDR_ALT
                );
            } else if matches!(e, Error::Bus(_)) {
                println!("bus fault: {} — check pull-ups / shorted lines", e);
            } else {
                println!("{}", e);
            }
            park(&delay);
        }
    };

    let mut read_failed = OnceLatch::new();
    loop {
        match sensor.measure(
            &mut delay,
            Oversampling::X2,
            Oversampling::X4,
            Oversampling::X2,
            IirFilter::Coeff3,
            true,
            TIMEOUT_MS,
        ) {
            Ok(reading) => {
                if read_failed.rearm() {
                    println!("measurement recovered");
                }
                match reading.gas_resistance_ohm() {
                    Some(gas) => println!(
                        "T: {:.1} degC  P: {:.1} hPa  RH: {:.1} %  gas: {:.0} ohm",
                        reading.temperature_c(),
                        reading.pressure_hpa(),
                        reading.humidity_rh(),
                        gas
                    ),
                    None => println!(
                        "T: {:.1} degC  P: {:.1} hPa  RH: {:.1} %  gas: not valid (heater not stable yet)",
                        reading.temperature_c(),
                        reading.pressure_hpa(),
                        reading.humidity_rh()
                    ),
                }
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
