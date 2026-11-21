//! ESP32-C6 — IR transmitter (RMT, 38 kHz carrier)
//!
//! Sends the NEC preset (address 0x00, command 0x45) with a 700 ms gap between
//! frames (a frame starts about every 770 ms; `send` blocks for the frame) through an
//! IR emitter.
//! All pulse timing comes from [`tamer::ir`]; the RMT only modulates.
//! Twin of `hal_c3_ir_tx`: same frames, same log lines apart from the pin.
//!
//! ## Status
//!
//! The 38 kHz / 33 % carrier is **not yet confirmed on hardware**.
//! On the C6, esp-hal 1.2.2 clocks the RMT from PLL_F80M (80 MHz, group
//! divider 1), and the carrier counts in that group clock (derived from esp-hal
//! and ESP-IDF sources).
//! The boot log prints the RMT clock, the carrier tick pair and the carrier
//! each clock would give: about 38 kHz if the pair counts in the group clock,
//! about 475 Hz if it counts in the 1 MHz channel clock.
//! A scope or a demodulating receiver on a second board tells which.
//! See `docs/features/ir-codec-v1.md`.
//!
//! ## Components
//!
//! - ESP32-C6-DevKitC-1
//! - 1 x IR LED (940 nm) behind a driver: an NPN stage, or an emitter module
//!   with a GND / VCC / SIG header
//!
//! ## Wiring
//!
//! GPIO 3 is the C6 5V pogo (ADR-015): 5 V supply, 3V3 logic; J1 pins
//! 15 / 14 / 13 are GND / 5V / GPIO 3 in pogo order.
//!
//! ```text
//! module GND -- J1-15 GND
//! module VCC -- J1-14 5V
//! module SIG -- J1-13 GPIO 3
//! ```
//!
//! A bare LED takes the same NPN stage as `hal_c3_ir_tx`, from GPIO 3.
//! A module that makes its own 38 kHz expects an unmodulated envelope on SIG,
//! so it cannot show this example's carrier.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c6_ir_tx
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c6_ir_tx
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{delay::Delay, main, rmt::Rmt};
use esp_println::{println, Printer};
use rustyfarian_esp_hal_peripherals::ir::{
    carrier_ticks, IrTransmitter, IrTxConfig, IrTxError, NecFrame, PulseCodec, IR_RMT_CLOCK,
};

/// Consumer-IR carrier: 38 kHz, 33 % of the period high.
const CARRIER_HZ: u32 = 38_000;
const CARRIER_DUTY: u8 = 33;
/// The RMT channel clock after the bridge's divider: 1 tick per microsecond.
const CHANNEL_CLOCK_HZ: u32 = 1_000_000;
/// Gap after each frame; `send` blocks for the frame first (about 67 ms for NEC).
///
/// Kept clear of 1000 ms: a receiver that re-arms a 1 s `receive` timeout
/// right after each frame would time out inside every next frame and drop its start.
const FRAME_GAP_MS: u32 = 700;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    halt()
}

/// Fixed-capacity line buffer; overlong lines are truncated, never panic.
struct Line {
    buf: [u8; 256],
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
        buf: [0; 256],
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

/// Park forever after a fatal error (no watchdog feed: the chip resets and retries).
fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// `clock / period`, rounded to the nearest Hz; `None` for an empty period.
fn carrier_at(clock_hz: u32, period_ticks: u32) -> Option<u32> {
    (clock_hz.checked_add(period_ticks / 2)?).checked_div(period_ticks)
}

/// Logs the carrier tick pair and the carrier each RMT clock would produce.
fn report_carrier(clock_hz: u32) {
    let Some((high, low)) = carrier_ticks(clock_hz, CARRIER_HZ, CARRIER_DUTY) else {
        logln!("carrier does not fit the RMT clock");
        return;
    };
    logln!(
        "carrier {} Hz {} %: high {} low {} ticks",
        CARRIER_HZ,
        CARRIER_DUTY,
        high,
        low
    );
    let period = u32::from(high) + u32::from(low);
    if let Some(hz) = carrier_at(clock_hz, period) {
        logln!("carrier if group clock: {} Hz", hz);
    }
    if let Some(hz) = carrier_at(CHANNEL_CLOCK_HZ, period) {
        logln!("carrier if channel clock (1 MHz): {} Hz", hz);
    }
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    let Ok(rmt) = Rmt::new(peripherals.RMT, IR_RMT_CLOCK) else {
        logln!("RMT init failed");
        halt()
    };
    let clock = rmt.frequency();
    logln!("IR TX on GPIO 3, RMT clock {} Hz", clock.as_hz());
    report_carrier(clock.as_hz());

    let Ok(mut tx) = IrTransmitter::<64>::new(
        rmt.channel0,
        peripherals.GPIO3,
        clock,
        &IrTxConfig::new(CARRIER_HZ, CARRIER_DUTY),
    ) else {
        logln!("IR TX init failed");
        halt()
    };

    logln!(
        "IR TX ready on GPIO 3 - sending NEC 0x00/0x45, {} ms gap between frames",
        FRAME_GAP_MS
    );

    let nec = PulseCodec::nec();
    let nec_word = NecFrame::new(0x00, 0x45).to_word();
    loop {
        match nec.encode::<67>(nec_word) {
            Ok(frame) => report("NEC 0x00/0x45", frame.as_slice(), tx.send(frame.as_slice())),
            Err(e) => logln!("NEC encode failed: {}", e),
        }
        delay.delay_millis(FRAME_GAP_MS);
    }
}

fn report(name: &str, pulses: &[u32], result: Result<(), IrTxError>) {
    match result {
        Ok(()) => logln!("sent {} ({} pulses)", name, pulses.len()),
        Err(e) => logln!("send {} failed: {}", name, e),
    }
}
