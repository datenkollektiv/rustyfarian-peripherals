//! ESP32-C3 — IR receiver (RMT RX)
//!
//! Captures frames from a demodulated IR receiver module (TSOP / VS1838 /
//! OPEN-SMART), prints the pulse count, decodes NEC through [`tamer::ir`], and
//! dumps the raw pulse list when decoding fails.
//! The RMT delimits frames with a 20 ms idle time, so an NEC repeat code shows
//! up as its own short frame (reported as "NEC repeat").
//! All decoding lives in `tamer`; this file only wires the pin.
//!
//! ## Components
//!
//! - ESP32-C3 development board
//! - 1 x 3-pin IR receiver module with 3.3 V-safe output (active low)
//!
//! ## Wiring
//!
//! ```text
//! IR module   ESP32-C3
//! ---------   --------
//! VCC         3V3
//! GND         GND
//! OUT         GPIO 4
//! ```
//!
//! GPIO 4 is the 3-pin pogo row of the bench rig (ADR-012).
//! A first mark seen truncated, or a frame split by a gap over 20 ms, is a
//! known limit of the RMT receive path (see `docs/features/ir-codec-v1.md`).
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_ir_rx
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_ir_rx
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{main, rmt::Rmt};
use esp_println::{println, Printer};
use rustyfarian_esp_hal_peripherals::ir::{
    is_nec_repeat, IrReceiver, IrRxConfig, NecFrame, PulseCodec, IR_RMT_CLOCK,
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

/// Raw pulses printed on a decode failure (keeps the line within one buffer).
const DUMP_PULSES: usize = 24;

/// Park forever after a fatal error (no watchdog feed: the chip resets and retries).
fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let Ok(rmt) = Rmt::new(peripherals.RMT, IR_RMT_CLOCK) else {
        logln!("RMT init failed");
        halt()
    };
    let clock = rmt.frequency();
    let Ok(mut rx) = IrReceiver::<64, 128>::new(
        rmt.channel2,
        peripherals.GPIO4,
        clock,
        &IrRxConfig::default(),
    ) else {
        logln!("IR RX init failed");
        halt()
    };

    let nec = PulseCodec::nec();
    logln!("IR RX ready on GPIO 4 - point a remote at the receiver");

    loop {
        match rx.receive(1000) {
            Ok(None) => {}
            Ok(Some(frame)) => {
                let n = frame.len();
                if is_nec_repeat(frame) {
                    logln!("{} pulses: NEC repeat", n);
                    continue;
                }
                match nec.decode(frame).map(NecFrame::from_word) {
                    Ok(Ok(f)) => logln!(
                        "{} pulses: NEC address=0x{:02X} command=0x{:02X}",
                        n,
                        f.address,
                        f.command
                    ),
                    _ => {
                        logln!(
                            "{} pulses: not NEC, first {}: {:?}",
                            n,
                            DUMP_PULSES,
                            &frame[..n.min(DUMP_PULSES)]
                        );
                    }
                }
            }
            Err(e) => logln!("receive error: {}", e),
        }
    }
}
