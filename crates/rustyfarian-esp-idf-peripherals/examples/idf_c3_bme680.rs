//! ESP32-C3 — BME680 Temperature, Pressure, Humidity and Gas (ESP-IDF / std)
//!
//! Reads a BME680 over I2C and logs one forced-mode measurement about every
//! three seconds: temperature, pressure, relative humidity and gas resistance.
//!
//! `tamer` owns everything with logic in it: the soft reset, identity and
//! variant check, calibration read ([`Bme680::new`]), the forced-mode conversion
//! sequence ([`Bme680::measure`]) and the compensation maths.
//! This example owns only the hardware: the I2C bus, the delay provider and the
//! log output.
//! Moving to the `esp-hal` tier means swapping those three pieces; the
//! `tamer` calls stay identical.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), not
//! `println!`, because bare `println!` writes to a buffered newlib stdout that
//! does not flush here.
//!
//! Bus errors never panic: init failures are logged with a hint and the example
//! parks idle, measurement failures are logged once per run via `OnceLatch`.
//! The gas heater runs on every measurement, so the first readings may report
//! `gas: not valid` until the plate has stabilised.
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
//! BME680        ESP32-C3
//! ------        --------
//! VCC           3V3
//! GND           GND
//! SDA           GPIO 4
//! SCL           GPIO 5
//! SDO           GND (address 0x76) or 3V3 (address 0x77)
//! ```
//!
//! The same four pins also serve an I2C OLED display or an MPU6050; see
//! `docs/hardware-setup.md`, "Wiring — shared I2C pogo header".
//! With SDO high, set `ADDRESS` to `bme680::I2C_ADDR_ALT`.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_bme680
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_bme680
//! ```

use esp_idf_hal::{
    delay::FreeRtos,
    i2c::{config::Config, I2cDriver},
    peripherals::Peripherals,
    units::FromValueType,
};
use tamer::bme680::{Bme680, Error, IirFilter, Oversampling, I2C_ADDR, I2C_ADDR_ALT};
use tamer::latch::OnceLatch;

/// Sensor address: SDO low gives 0x76 (`I2C_ADDR`), SDO high gives 0x77 (`I2C_ADDR_ALT`).
const ADDRESS: u8 = I2C_ADDR;
/// Pause between measurements in milliseconds.
const PERIOD_MS: u32 = 3000;
/// Budget for the driver's own waits per measurement (conversion plus heater).
const TIMEOUT_MS: u32 = 1000;

/// Parks the task forever so the watchdog-friendly idle loop keeps the log readable.
fn park() -> ! {
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let Some(peripherals) = Peripherals::take().ok() else {
        log::warn!("peripherals already taken");
        park();
    };
    let config = Config::new().baudrate(100.kHz().into());
    let i2c = match I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio4,
        peripherals.pins.gpio5,
        &config,
    ) {
        Ok(i2c) => i2c,
        Err(e) => {
            log::warn!("I2C bus setup failed: {:?}", e);
            park();
        }
    };
    let mut delay = FreeRtos;

    let mut sensor = match Bme680::new(i2c, ADDRESS, &mut delay) {
        Ok(sensor) => {
            log::info!("BME680 OK at address 0x{:02X}", ADDRESS);
            sensor
        }
        Err((e, _i2c)) => {
            if e.is_nack() {
                log::warn!(
                    "no BME680 answered at 0x{:02X}: check VCC/GND, SDA=GPIO4, SCL=GPIO5 \
                     and SDO (SDO high means address 0x{:02X}: try I2C_ADDR_ALT)",
                    ADDRESS,
                    I2C_ADDR_ALT
                );
            } else if matches!(e, Error::Bus(_)) {
                log::warn!("bus fault: {} — check pull-ups / shorted lines", e);
            } else {
                log::warn!("{}", e);
            }
            park();
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
                    log::info!("measurement recovered");
                }
                match reading.gas_resistance_ohm() {
                    Some(gas) => log::info!(
                        "T: {:.1} degC  P: {:.1} hPa  RH: {:.1} %  gas: {:.0} ohm",
                        reading.temperature_c(),
                        reading.pressure_hpa(),
                        reading.humidity_rh(),
                        gas
                    ),
                    None => log::info!(
                        "T: {:.1} degC  P: {:.1} hPa  RH: {:.1} %  gas: not valid (heater not stable yet)",
                        reading.temperature_c(),
                        reading.pressure_hpa(),
                        reading.humidity_rh()
                    ),
                }
            }
            Err(e) => {
                if read_failed.trip() {
                    log::warn!("measurement failed: {}", e);
                }
            }
        }
        FreeRtos::delay_ms(PERIOD_MS);
    }
}
