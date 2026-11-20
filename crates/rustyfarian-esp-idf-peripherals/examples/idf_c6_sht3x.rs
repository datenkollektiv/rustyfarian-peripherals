//! ESP32-C6 — SHT3x Temperature and Humidity (ESP-IDF / std)
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
//! - ESP32-C6-DevKitC-1
//! - 1 × SHT3x breakout module (SHT30/SHT31/SHT35, 3.3 V I2C)
//! - jumper wires
//!
//! ## Pins (board profile ADR-014)
//!
//! | Part  | Pin(s)          | Notes                         |
//! |:------|:----------------|:------------------------------|
//! | SHT3x | SDA 21 / SCL 22 | I2C0 at 100 kHz, address 0x44 |
//!
//! ## Wiring
//!
//! ```text
//! SHT3x      ESP32-C6-DevKitC-1
//! -----      ------------------
//! VCC        3V3 (J1-1, the only 3V3 pin)
//! GND        GND (J3-1)
//! SDA        GPIO 21 (J3-7)
//! SCL        GPIO 22 (J3-6)
//! ADDR       GND (address 0x44; tie to 3V3 for 0x45)
//! ```
//!
//! The pins come from the `c6-devkitc` board profile (ADR-014), the bus
//! `idf_c6_oled` uses too; `scripts/lib.sh` adds the profile to a C6 build.
//! The sensor is wired directly until the rig's I2C pogo and its 4.7 kΩ
//! pull-ups are fitted, so a module without its own pull-ups needs 4.7 kΩ
//! from SDA and SCL to 3V3 now.
//! With ADDR high, set `ADDRESS` to `sht3x::I2C_ADDR_ALT` and rebuild.
//! The board's wiring is in `bench/c6/README.md`.
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
//! just build-example idf_c6_sht3x
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c6_sht3x
//! ```

use esp_idf_hal::{
    delay::{Ets, FreeRtos},
    i2c::{config::Config, I2cDriver},
    peripherals::Peripherals,
    units::FromValueType,
};
use rustyfarian_esp_idf_peripherals::{board::I2C_PINS, board_i2c, board_i2c_power};
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
    // The board profile names the I2C pins (SDA 21 / SCL 22 on the C6 DevKitC-1).
    let _bus_power = board_i2c_power!(peripherals);
    let (sda, scl) = board_i2c!(peripherals);
    let i2c = match I2cDriver::new(peripherals.i2c0, sda, scl, &config) {
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
                    "no SHT3x answered at 0x{:02X}: check VCC/GND, SDA=GPIO{}, SCL=GPIO{} \
                     and ADDR (ADDR high means address 0x{:02X}: try I2C_ADDR_ALT)",
                    ADDRESS,
                    I2C_PINS.0,
                    I2C_PINS.1,
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
