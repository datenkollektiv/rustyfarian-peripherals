//! ESP32 — 128x64 OLED check with hot-plug (SSD1306 or SH1106)
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
//! Select it with `just build-example hal_esp_oled feather-v2`.
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
//! just build-example hal_esp_oled
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_esp_oled
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use esp_hal::{
    delay::Delay,
    i2c::master::{Config as I2cConfig, I2c},
    main,
    time::{Instant, Rate},
};
use esp_println::{println, Printer};
use rustyfarian_esp_hal_peripherals::{
    board::{BOARD_NAME, I2C_PINS},
    board_i2c, board_i2c_power,
};
use signwriter::{Event, Oled, ADDRESS_PRIMARY};

/// Fixed-capacity line buffer; overlong lines are truncated, never panic.
struct Line {
    buf: [u8; 128],
    len: usize,
}

impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - 1 - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// Emits one log line with a single `Printer::write_bytes` call, so the last
/// segment cannot stay stuck in the USB-JTAG TX FIFO (project lore,
/// "esp-hal — console output").
fn log_line(args: core::fmt::Arguments<'_>) {
    use core::fmt::Write;
    let mut line = Line {
        buf: [0; 128],
        len: 0,
    };
    let _ = line.write_fmt(args);
    line.buf[line.len] = b'\n';
    Printer::write_bytes(&line.buf[..=line.len]);
}

macro_rules! logln {
    ($($arg:tt)*) => {
        log_line(format_args!($($arg)*))
    };
}

const TICK_MS: u32 = 10;
const PROBE_PERIOD_MS: u64 = 200;
const ATTACH_DEBOUNCE_MS: u64 = 600;
const DETACH_ERRORS: u32 = 3;
const FRAME_PERIOD_MS: u64 = 1000;
const FRAME_LOG_EVERY: u32 = 5;
const TIER: &str = "hal";

fn now_ms() -> u64 {
    Instant::now().duration_since_epoch().as_millis()
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

/// Draws the border, the two fixed texts and the counter into the buffer.
fn draw<D: DrawTarget<Color = BinaryColor>>(target: &mut D, n: u32) -> Result<(), D::Error> {
    use core::fmt::Write;
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let mut counter = Line {
        buf: [0; 128],
        len: 0,
    };
    let _ = write!(counter, "n={}", n);
    let counter = core::str::from_utf8(&counter.buf[..counter.len]).unwrap_or("n=?");

    target.clear(BinaryColor::Off)?;
    Rectangle::new(Point::zero(), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target)?;
    Text::with_baseline("rustyfarian", Point::new(8, 10), style, Baseline::Top).draw(target)?;
    Text::with_baseline(TIER, Point::new(8, 26), style, Baseline::Top).draw(target)?;
    Text::with_baseline(counter, Point::new(8, 42), style, Baseline::Top).draw(target)?;
    Ok(())
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    let _bus_power = board_i2c_power!(peripherals);
    let (sda, scl) = board_i2c!(peripherals);
    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(100)),
    )
    .expect("valid I2C configuration")
    .with_sda(sda)
    .with_scl(scl);

    logln!(
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
        let now = now_ms();
        match oled.tick(now) {
            Some(Event::Attached(controller)) => {
                n = 0;
                next_frame = now + FRAME_PERIOD_MS;
                let _ = draw(&mut oled, n);
                logln!(
                    "OLED ready addr=0x{:02X} controller={:?} size=128x64",
                    ADDRESS_PRIMARY,
                    controller
                );
            }
            Some(Event::Detached) => {
                logln!("OLED detached addr=0x{:02X}", ADDRESS_PRIMARY);
            }
            None => {}
        }
        if oled.is_attached() && now >= next_frame {
            next_frame = now + FRAME_PERIOD_MS;
            n = n.wrapping_add(1);
            let _ = draw(&mut oled, n);
            if n.is_multiple_of(FRAME_LOG_EVERY) {
                logln!("OLED frame n={}", n);
            }
        }
        delay.delay_millis(TICK_MS);
    }
}
