//! ESP32 — 128x64 OLED check with hot-plug (SSD1306 or SH1106, ESP-IDF / std)
//!
//! ESP-IDF (`std`) counterpart to `hal_c3_oled`.
//!
//! Bring-up check for the 128x64 I2C OLED on an ESP32 board.
//! Either controller may be plugged in, after boot, and swapped while running:
//! [`signwriter::Oled`] detects attach and detach (via
//! [`tamer::hotplug::Hotplug`]), tells an SSD1306 from an SH1106, initialises
//! it and flushes the framebuffer; this example draws through `embedded-graphics`
//! and holds no decode logic of its own (ADR-008, ADR-013).
//!
//! ## Pins (board profile ADR-014)
//!
//! | Part  | Pin(s)  | Notes                          |
//! |:------|:--------|:-------------------------------|
//! | OLED  | SDA 21 / SCL 22 (esp32-wroom) or SDA 22 / SCL 20 (feather-v2) | I2C0 at 100 kHz, address 0x3C  |
//!
//! Wire VCC, GND, SDA and SCL to the 4-pin OLED module.
//! The `feather-v2` profile uses the STEMMA QT socket and drives GPIO 2 high to power it
//! (GPIO 2 is a strapping pin; it is driven only after boot, so it does not affect boot mode).
//! Select it with `just build-example idf_esp_oled feather-v2`.
//! Most OLED modules carry their own SDA/SCL pull-ups; if the display does not
//! answer on a bare bus, fit external 4.7 kΩ pull-ups from SDA and SCL to 3V3.
//!
//! ## Behaviour
//!
//! `Oled::tick` is called every 10 ms with a millisecond clock.
//! While nothing is attached it probes every 200 ms; the display counts as
//! attached after 600 ms of continuous answers, and 3 failed bus operations in
//! a row count as a detach.
//! The picture is a 1-px border, `rustyfarian`, the tier name and a counter
//! `n=<seconds>` that changes every second while attached.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), not
//! `println!` (see `idf_c3_i2c_scan`).
//!
//! ## Log lines
//!
//! ```text
//! OLED waiting on esp32-wroom SDA 21 / SCL 22 (0x3C)   (<board> from the profile)
//! OLED ready addr=0x3C controller=Sh1106 size=128x64
//! OLED frame n=5          (every 5 s while attached)
//! OLED detached addr=0x3C
//! ```
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_esp_oled
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_esp_oled
//! ```

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use esp_idf_hal::{
    delay::FreeRtos,
    i2c::{config::Config, I2cDriver},
    peripherals::Peripherals,
    units::FromValueType,
};
use rustyfarian_esp_idf_peripherals::{
    board::{BOARD_NAME, I2C_PINS},
    board_i2c, board_i2c_power,
};
use signwriter::{Event, Oled, ADDRESS_PRIMARY};
use std::time::Instant;

const TICK_MS: u32 = 10;
const PROBE_PERIOD_MS: u64 = 200;
const ATTACH_DEBOUNCE_MS: u64 = 600;
const DETACH_ERRORS: u32 = 3;
const FRAME_PERIOD_MS: u64 = 1000;
const FRAME_LOG_EVERY: u32 = 5;
const TIER: &str = "idf";

fn now_ms(start: &Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// Draws the border, the two fixed texts and the counter into the buffer.
fn draw<D: DrawTarget<Color = BinaryColor>>(target: &mut D, n: u32) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let counter = format!("n={n}");

    target.clear(BinaryColor::Off)?;
    Rectangle::new(Point::zero(), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target)?;
    Text::with_baseline("rustyfarian", Point::new(8, 10), style, Baseline::Top).draw(target)?;
    Text::with_baseline(TIER, Point::new(8, 26), style, Baseline::Top).draw(target)?;
    Text::with_baseline(&counter, Point::new(8, 42), style, Baseline::Top).draw(target)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let config = Config::new().baudrate(100.kHz().into());
    let _bus_power = board_i2c_power!(peripherals);
    let (sda, scl) = board_i2c!(peripherals);
    let i2c = I2cDriver::new(peripherals.i2c0, sda, scl, &config)?;
    let start = Instant::now();

    log::info!(
        "OLED waiting on {} SDA {} / SCL {} (0x3C)",
        BOARD_NAME,
        I2C_PINS.0,
        I2C_PINS.1
    );

    let mut oled = Oled::new(
        i2c,
        ADDRESS_PRIMARY,
        ATTACH_DEBOUNCE_MS,
        DETACH_ERRORS,
        PROBE_PERIOD_MS,
    );
    let mut n = 0u32;
    let mut next_frame = 0u64;
    loop {
        let now = now_ms(&start);
        match oled.tick(now) {
            Some(Event::Attached(controller)) => {
                n = 0;
                next_frame = now + FRAME_PERIOD_MS;
                let _ = draw(&mut oled, n);
                log::info!(
                    "OLED ready addr=0x{:02X} controller={:?} size=128x64",
                    ADDRESS_PRIMARY,
                    controller
                );
            }
            Some(Event::Detached) => {
                log::info!("OLED detached addr=0x{:02X}", ADDRESS_PRIMARY);
            }
            None => {}
        }
        if oled.is_attached() && now >= next_frame {
            next_frame = now + FRAME_PERIOD_MS;
            n = n.wrapping_add(1);
            let _ = draw(&mut oled, n);
            if n.is_multiple_of(FRAME_LOG_EVERY) {
                log::info!("OLED frame n={}", n);
            }
        }
        FreeRtos::delay_ms(TICK_MS);
    }
}
