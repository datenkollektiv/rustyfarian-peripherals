//! ESP32-C3 — SSD1306 vs SH1106 identification spike
//!
//! Temporary detection spike.
//! Both controllers answer at 0x3C/0x3D and neither has an ID register, so this
//! example measures how each module reacts to a few harmless raw I2C probes.
//! The logs of an SSD1306 and an SH1106 are compared offline to find a reliable
//! runtime rule.
//! It is removed once the detector has landed in `signwriter`.
//! No driver crate is used and no decode logic lives here (ADR-008).
//!
//! ## Pins (ADR-012)
//!
//! | Part  | Pin(s)        | Notes                                         |
//! |:------|:--------------|:----------------------------------------------|
//! | OLED  | SDA 6 / SCL 7 | I2C0 at 100 kHz, address 0x3C (0x3D fallback) |
//!
//! The 4-pin pogo carries VCC, GND, SDA and SCL.
//!
//! ## Operator procedure
//!
//! 1. Flash and start the example, then plug in the SSD1306 module.
//! 2. Wait for `IDENTIFY done addr=0x3C`, then unplug the module (`OLED gone`).
//! 3. Plug in the SH1106 module and wait for `IDENTIFY done` again.
//! 4. Repeat with each module as often as needed; no reset is required.
//!
//! ## Behaviour
//!
//! A zero-length write probes 0x3C and then 0x3D every 200 ms until one ACKs.
//! The probes P1 to P3 then run three times (`run=1..3`).
//! Afterwards the address is polled until 3 consecutive probes miss.
//! Only harmless commands are sent: page and column addressing plus two RAM
//! data bytes.
//! The panel is never initialised (no charge pump, no display-on) and stays off.
//! Bus errors are logged, never unwrapped.
//!
//! - P1: plain 1-byte and 2-byte reads, and a read after control byte 0x00.
//! - P2: write 0xA5 to page 0, column 130 (the SH1106 has 132 columns, the
//!   SSD1306 only 128), set the column again and read back two bytes after
//!   control byte 0x40 (the first is a dummy read).
//! - P3: the same at column 0 with 0x5A.
//!
//! ## Log lines
//!
//! ```text
//! OLED present addr=0x3C
//! P1 run=1 read1 ok=0xNN        (or: P1 run=1 read1 err=<Debug>)
//! P1 run=1 read2 ok=0xNN,0xNN   (or: err=<Debug>)
//! P1 run=1 cmdread ok=0xNN      (or: err=<Debug>)
//! P2 run=1 col130 wrote=0xA5 read=0xNN,0xNN   (or: col130 err=<Debug>)
//! P3 run=1 col0 wrote=0x5A read=0xNN,0xNN     (or: col0 err=<Debug>)
//! IDENTIFY done addr=0x3C
//! OLED gone
//! ```
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_oled_identify
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_oled_identify
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use core::fmt::Debug;

use esp_hal::{
    delay::Delay,
    i2c::master::{Config as I2cConfig, I2c},
    main,
    time::Rate,
};
use esp_println::{println, Printer};

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

const PROBE_PERIOD_MS: u32 = 200;
const RUN_GAP_MS: u32 = 100;
const GONE_MISSES: u32 = 3;
const RUNS: u32 = 3;
const ADDRESSES: [u8; 2] = [0x3C, 0x3D];

const CONTROL_COMMAND: u8 = 0x00;
const CONTROL_DATA: u8 = 0x40;
/// Page 0 (`0xB0 | page`).
const SET_PAGE0: u8 = 0xB0;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

/// Zero-length-write probe of both OLED addresses; the first that ACKs wins.
fn probe<I: embedded_hal::i2c::I2c>(i2c: &mut I) -> Option<u8> {
    ADDRESSES
        .into_iter()
        .find(|&address| i2c.write(address, &[]).is_ok())
}

/// P1: plain reads and a read after the command control byte.
fn probe_p1<I: embedded_hal::i2c::I2c>(i2c: &mut I, address: u8, run: u32)
where
    I::Error: Debug,
{
    let mut one = [0u8; 1];
    match i2c.read(address, &mut one) {
        Ok(()) => logln!("P1 run={} read1 ok=0x{:02X}", run, one[0]),
        Err(err) => logln!("P1 run={} read1 err={:?}", run, err),
    }

    let mut two = [0u8; 2];
    match i2c.read(address, &mut two) {
        Ok(()) => logln!("P1 run={} read2 ok=0x{:02X},0x{:02X}", run, two[0], two[1]),
        Err(err) => logln!("P1 run={} read2 err={:?}", run, err),
    }

    let mut cmd = [0u8; 1];
    match i2c.write_read(address, &[CONTROL_COMMAND], &mut cmd) {
        Ok(()) => logln!("P1 run={} cmdread ok=0x{:02X}", run, cmd[0]),
        Err(err) => logln!("P1 run={} cmdread err={:?}", run, err),
    }
}

/// Sets page 0 and the SH1106-style column address (low nibble command
/// `0x0L`, high nibble command `0x1H`).
fn set_column<I: embedded_hal::i2c::I2c>(
    i2c: &mut I,
    address: u8,
    low: u8,
    high: u8,
) -> Result<(), I::Error> {
    i2c.write(address, &[CONTROL_COMMAND, SET_PAGE0, low, high])
}

/// P2/P3: write one RAM byte at the column, address it again and read back two
/// bytes after the data control byte.
fn probe_ram<I: embedded_hal::i2c::I2c>(
    i2c: &mut I,
    address: u8,
    probe_name: &str,
    column_name: &str,
    column: (u8, u8),
    value: u8,
    run: u32,
) where
    I::Error: Debug,
{
    let mut read = [0u8; 2];
    let result = set_column(i2c, address, column.0, column.1)
        .and_then(|()| i2c.write(address, &[CONTROL_DATA, value]))
        .and_then(|()| set_column(i2c, address, column.0, column.1))
        .and_then(|()| i2c.write_read(address, &[CONTROL_DATA], &mut read));
    match result {
        Ok(()) => logln!(
            "{} run={} {} wrote=0x{:02X} read=0x{:02X},0x{:02X}",
            probe_name,
            run,
            column_name,
            value,
            read[0],
            read[1]
        ),
        Err(err) => logln!("{} run={} {} err={:?}", probe_name, run, column_name, err),
    }
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    let mut i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(100)),
    )
    .expect("valid I2C configuration")
    .with_sda(peripherals.GPIO6)
    .with_scl(peripherals.GPIO7);

    logln!("IDENTIFY waiting on SDA 6 / SCL 7 (0x3C or 0x3D)");

    loop {
        let address = loop {
            if let Some(address) = probe(&mut i2c) {
                break address;
            }
            delay.delay_millis(PROBE_PERIOD_MS);
        };
        logln!("OLED present addr=0x{:02X}", address);

        for run in 1..=RUNS {
            probe_p1(&mut i2c, address, run);
            // Column 130 = 0x82: low nibble 0x02, high nibble 0x18.
            probe_ram(&mut i2c, address, "P2", "col130", (0x02, 0x18), 0xA5, run);
            probe_ram(&mut i2c, address, "P3", "col0", (0x00, 0x10), 0x5A, run);
            delay.delay_millis(RUN_GAP_MS);
        }
        logln!("IDENTIFY done addr=0x{:02X}", address);

        let mut misses = 0;
        while misses < GONE_MISSES {
            delay.delay_millis(PROBE_PERIOD_MS);
            if i2c.write(address, &[]).is_ok() {
                misses = 0;
            } else {
                misses += 1;
            }
        }
        logln!("OLED gone");
    }
}
