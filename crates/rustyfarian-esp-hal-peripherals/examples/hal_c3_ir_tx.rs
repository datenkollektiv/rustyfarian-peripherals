//! ESP32-C3 — IR transmitter (RMT, 38 kHz carrier)
//!
//! Sends two fixed frames every 2 s through an IR LED: the NEC preset
//! (address 0x00, command 0x45) and an 8-bit "candle" style codec (header
//! 8400/4470 us, bits 560/540 and 1000/1000, trailer 560 us, LSB first).
//! All pulse timing comes from [`tamer::ir`]; the RMT only modulates.
//!
//! ## Status
//!
//! The 38 kHz / 33 % carrier is **not yet confirmed on hardware**.
//! The carrier counts in the RMT group clock (derived from esp-hal and ESP-IDF
//! sources).
//! Put a scope or a TSOP receiver on the LED output: about 38 kHz and 33 % duty
//! during each mark, nothing during spaces, line low at rest.
//! A carrier near 475 Hz means the channel clock is used instead.
//! See `docs/features/ir-codec-v1.md`.
//!
//! ## Components
//!
//! - ESP32-C3 development board
//! - 1 x IR LED (940 nm), 1 x NPN transistor, 1 x base resistor, 1 x LED series resistor
//!
//! ## Wiring
//!
//! GPIO 10 is the NPN output row of the bench rig (ADR-012), shared with the
//! buzzer examples, so do not connect both at once.
//!
//! ```text
//! GPIO 10 --[ 1 kOhm ]-- NPN base
//! 3V3 --[ 100 Ohm ]-- IR LED anode, cathode -- NPN collector, emitter -- GND
//! ```
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_ir_tx
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_ir_tx
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{delay::Delay, main, rmt::Rmt};
use esp_println::{println, Printer};
use rustyfarian_esp_hal_peripherals::ir::{
    BitOrder, IrTransmitter, IrTxConfig, MarkSpace, NecFrame, PulseBuf, PulseCodec, Tolerance,
    IR_RMT_CLOCK,
};

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

/// Candle-style 8-bit codec (timings from the real remote captures).
const fn candle() -> Result<PulseCodec, rustyfarian_esp_hal_peripherals::ir::CodecError> {
    PulseCodec::new(
        Some(MarkSpace::new(8400, 4470)),
        MarkSpace::new(560, 540),
        MarkSpace::new(1000, 1000),
        Some(560),
        8,
        BitOrder::LsbFirst,
        Tolerance::Percent(25),
    )
}

/// Park forever after a fatal error (no watchdog feed: the chip resets and retries).
fn halt() -> ! {
    loop {
        core::hint::spin_loop();
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
    let Ok(mut tx) = IrTransmitter::<64>::new(
        rmt.channel0,
        peripherals.GPIO10,
        clock,
        &IrTxConfig::default(),
    ) else {
        logln!("IR TX init failed");
        halt()
    };
    let Ok(candle) = candle() else {
        logln!("candle codec invalid");
        halt()
    };

    logln!("IR TX ready on GPIO 10 (38 kHz, 33 %) - sending every 2 s");

    let nec = PulseCodec::nec();
    let nec_word = NecFrame::new(0x00, 0x45).to_word();
    loop {
        match nec.encode::<67>(nec_word) {
            Ok(frame) => report("NEC 0x00/0x45", frame.as_slice(), tx.send(frame.as_slice())),
            Err(e) => logln!("NEC encode failed: {}", e),
        }
        delay.delay_millis(1000u32);

        let frame: Result<PulseBuf<20>, _> = candle.encode(0xC0);
        match frame {
            Ok(frame) => report("candle 0xC0", frame.as_slice(), tx.send(frame.as_slice())),
            Err(e) => logln!("candle encode failed: {}", e),
        }
        delay.delay_millis(1000u32);
    }
}

fn report(
    name: &str,
    pulses: &[u32],
    result: Result<(), rustyfarian_esp_hal_peripherals::ir::IrTxError>,
) {
    match result {
        Ok(()) => logln!("sent {} ({} pulses)", name, pulses.len()),
        Err(e) => logln!("send {} failed: {}", name, e),
    }
}
