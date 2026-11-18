//! ESP32-C3 — SHT3x Temperature and Humidity (ESP-IDF / std)
//!
//! Reads an SHT3x (SHT30/SHT31/SHT35) over I2C and logs one single-shot
//! measurement every two seconds: temperature and relative humidity.
//!
//! `tamer` owns everything that can be host-tested: the soft reset and status
//! read ([`Sht3x::init`]), the single-shot command, wait and read sequence
//! ([`Sht3x::measure`]), the CRC check and the fixed-point conversion.
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
//!
//! The driver waits with [`Ets`](esp_idf_hal::delay::Ets), a busy-wait, not
//! `FreeRtos`: a FreeRTOS delay can end up to one tick (10 ms at the default
//! 100 Hz) early, which would cut the 15.5 ms conversion wait short and make
//! the read NACK as not ready.
//!
//! Values are logged in fixed point (hundredths of a degree and of a percent),
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
//! Run `idf_c3_i2c_scan` first if the sensor does not answer.
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
//! just build-example idf_c3_sht3x
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_sht3x
//! ```

use esp_idf_hal::{
    delay::{Ets, FreeRtos},
    i2c::{config::Config, I2cDriver},
    peripherals::Peripherals,
    units::FromValueType,
};
use tamer::latch::OnceLatch;
use tamer::sht3x::{Error, Repeatability, Sht3x, I2C_ADDR, I2C_ADDR_ALT};

/// Sensor address: ADDR low gives 0x44 (`I2C_ADDR`), ADDR high gives 0x45 (`I2C_ADDR_ALT`).
const ADDRESS: u8 = I2C_ADDR;
/// Pause between measurements in milliseconds.
const PERIOD_MS: u32 = 2000;

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
        peripherals.pins.gpio6,
        peripherals.pins.gpio7,
        &config,
    ) {
        Ok(i2c) => i2c,
        Err(e) => {
            log::warn!("I2C bus setup failed: {:?}", e);
            park();
        }
    };
    // Busy-wait delay (not `FreeRtos`): exact, so the conversion wait is never short.
    let mut delay = Ets;

    let mut sensor = Sht3x::new(i2c, ADDRESS);
    match sensor.init(&mut delay) {
        Ok(status) => {
            log::info!(
                "SHT3x OK at address 0x{:02X}, status 0x{:04X}",
                ADDRESS,
                status.bits()
            );
        }
        Err(e) => {
            if e.is_nack() {
                log::warn!(
                    "no SHT3x answered at 0x{:02X}: check VCC/GND, SDA=GPIO6, SCL=GPIO7 \
                     and ADDR (ADDR high means address 0x{:02X}: try I2C_ADDR_ALT)",
                    ADDRESS,
                    I2C_ADDR_ALT
                );
            } else if matches!(e, Error::Bus(_)) {
                log::warn!("bus fault: {} — check pull-ups / shorted lines", e);
            } else {
                log::warn!("SHT3x init failed: {}", e);
            }
            park();
        }
    }

    let mut read_failed = OnceLatch::new();
    loop {
        match sensor.measure(&mut delay, Repeatability::High) {
            Ok(reading) => {
                if read_failed.rearm() {
                    log::info!("measurement recovered");
                }
                // Fixed-point log: split the magnitude so -0.05 keeps its sign.
                let t = reading.temperature_centi_c();
                let sign = if t < 0 { "-" } else { "" };
                let t_abs = t.unsigned_abs();
                let rh = reading.humidity_centi_rh();
                log::info!(
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
                    log::warn!("measurement failed: {}", e);
                }
            }
        }
        FreeRtos::delay_ms(PERIOD_MS);
    }
}
