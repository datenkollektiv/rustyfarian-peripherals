//! ESP32-C3 — Active buzzer keyed in Morse by `tamer::morse::MorseKeyer`
//!
//! Keys the message "CQ" in a loop through a plain digital output pin.
//! All Morse timing and encoding logic lives in the pure
//! [`MorseKeyer`](tamer::morse::MorseKeyer) — this example only reads its
//! [`output()`](tamer::morse::MorseKeyer::output) each poll and, when that
//! output changes, drives GPIO6 high or low.
//! Unlike the passive-piezo arpeggio example, there is no LEDC/PWM here: an
//! active buzzer has its own internal oscillator and only needs to be gated
//! on and off.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
//! - 1 x active electromagnetic buzzer (e.g. TMB12A03)
//!
//! An active buzzer has its own internal oscillator, so keying GPIO6 high
//! sounds it and keying it low silences it.
//!
//! ## Wiring
//!
//! ```text
//! Active buzzer (TMB12A03)   ESP32-C3
//! ─────────────────────      ────────
//! + (signal)                 GPIO 6
//! − (ground)                 GND
//! ```
//!
//! GPIO 6 is not a strapping pin (2/8/9), not the on-board WS2812 (8), not
//! USB (18/19), not the UART console (20/21), and not in-package SPI flash
//! (11-17) — the same safe general-purpose output the passive arpeggio
//! example uses.
//!
//! GPIO6 is configured for the maximum 40 mA drive strength (see
//! `OutputConfig` below) to source the TMB12A03's up-to-30 mA draw directly,
//! as wired above.
//! See `docs/hardware-setup.md` for the full wiring guidance, including the
//! transistor alternative for larger or lower-impedance buzzers.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_active_buzzer_morse
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_active_buzzer_morse
//! ```
//!
//! ## Caveats
//!
//! - `Speed::Standard` (60 ms dot) is hardware-dependent; `Speed::Relaxed`
//!   (100 ms) is the default. See `docs/hardware-setup.md` for the acoustic
//!   response-time caveat.
//! - esp-hal's `set_high`/`set_low` are infallible, so the cached `last`
//!   state always advances after a write — unlike the esp-idf tier, there is
//!   no write-failure retry path to exercise here.

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{
    delay::Delay,
    gpio::{DriveStrength, Level, Output, OutputConfig},
    main,
    time::Instant,
};
use esp_println::println;
use tamer::morse::{KeyState, MorseEvent, MorseKeyer, MorseMode};

// The message keyed in a loop.
const MESSAGE: &str = "CQ";

// Keying speed, local to this example — not part of `tamer`. `Relaxed` is the
// default because `Standard` is hardware-dependent; see the "Caveats" doc
// section above.
#[allow(
    dead_code,
    reason = "Standard is a documented, selectable alternative to SPEED"
)]
#[derive(Clone, Copy)]
enum Speed {
    Relaxed,
    Standard,
}

impl Speed {
    const fn unit_ms(self) -> u64 {
        match self {
            Speed::Relaxed => 100,
            Speed::Standard => 60,
        }
    }
}

const SPEED: Speed = Speed::Relaxed;

const POLL_INTERVAL_MS: u32 = 5;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // `OutputConfig::default()` resolves to push-pull drive; `with_drive_strength`
    // selects the maximum 40 mA drive strength so the buzzer's up-to-30 mA
    // draw can be sourced directly off the pin (see the "Wiring" doc note).
    let mut buzzer = Output::new(
        peripherals.GPIO6,
        Level::Low,
        OutputConfig::default().with_drive_strength(DriveStrength::_40mA),
    );
    let delay = Delay::new();

    let mut keyer = MorseKeyer::new(MESSAGE.as_bytes(), SPEED.unit_ms(), MorseMode::Loop);
    let now0_ms: u64 = Instant::now().duration_since_epoch().as_millis();
    keyer.start(now0_ms);

    // The first character is already loaded by `start()` and never fires its
    // own `CharacterStarted` event — log it explicitly here.
    println!("t={} ms  keying \"{}\"", now0_ms, MESSAGE);

    let mut last: Option<KeyState> = None;

    loop {
        let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

        match keyer.update(now_ms) {
            Some(MorseEvent::CharacterStarted(i)) => {
                println!("t={} ms  character at byte {}", now_ms, i)
            }
            Some(MorseEvent::Finished) => println!("t={} ms  message finished", now_ms),
            None => {}
        }

        let out = keyer.output();

        if last != Some(out) {
            match out {
                KeyState::On => buzzer.set_high(),
                KeyState::Off => buzzer.set_low(),
            }
            // Infallible write, so `last` always advances.
            last = Some(out);
        }

        delay.delay_millis(POLL_INTERVAL_MS);
    }
}
