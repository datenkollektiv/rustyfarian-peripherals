//! ESP32-C6 — Passive piezo buzzer sequenced by `tamer::tone::ToneSequencer`
//!
//! Plays a short ascending power-on arpeggio through LEDC PWM, looping
//! forever. All melody/timing logic lives in the pure
//! [`ToneSequencer`](tamer::tone::ToneSequencer) — this example only reads
//! its [`output()`](tamer::tone::ToneSequencer::output) each tick and, when
//! that output changes, reprograms the LEDC timer's frequency and the
//! channel's duty cycle. To sound a tone on a passive piezo the LEDC *timer*
//! frequency is set to the note's `frequency_hz` and duty is held near 50%
//! (scaled by the note's amplitude); a rest (`frequency_hz == 0`) sets duty
//! to 0 without touching the timer's frequency.
//!
//! This is the ESP32-C6 variant of `hal_c3_passive_buzzer_arpeggio.rs`; the
//! LEDC driving code is identical, only the chip feature, wiring notes and
//! clock figures differ.
//!
//! ## Components
//!
//! - ESP32-C6 development board (e.g. ESP32-C6-DevKitC-1)
//! - 1 × passive piezo buzzer
//!
//! A small, high-impedance piezo buzzer can be driven directly from the GPIO
//! pin as wired below. A loud/low-impedance buzzer can pull more current than
//! a GPIO should source — for those, drive it through a small NPN transistor
//! (GPIO through a base resistor, buzzer on the collector, emitter to GND)
//! instead of direct drive.
//!
//! ## Wiring
//!
//! ```text
//! Piezo buzzer       ESP32-C6
//! ─────────────      ────────
//! + (signal)          GPIO 6
//! − (ground)          GND
//! ```
//!
//! GPIO 6 is not a strapping pin (4/5/8/9/15), not the on-board WS2812 (8 on
//! the DevKitC-1), not USB Serial/JTAG (12/13), not the UART console (16/17),
//! and not in-package SPI flash (24-30) — a safe general-purpose PWM output on
//! common ESP32-C6 boards.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c6_passive_buzzer_arpeggio
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c6_passive_buzzer_arpeggio
//! ```
//!
//! ## Caveats
//!
//! - Duty cycle here is a coarse "volume" proxy, not a true digital amplitude
//!   control: scaling duty away from ~50% reduces the delivered squarewave
//!   energy, which perceptibly (if crudely) affects loudness on most passive
//!   piezo buzzers — it is not calibrated, linear volume.
//! - Every note rebuilds the LEDC [`Channel`](esp_hal::ledc::channel::Channel)
//!   from a fresh [`reborrow`](esp_hal::gpio::AnyPin::reborrow) of the same
//!   pin, rather than reusing one `Channel` across notes as
//!   `hal_c3_poti_led.rs` does. This is not a copy-paste bug: esp-hal's
//!   `Channel<'a, S>` *retains* the `&'a Timer` reference passed to
//!   `configure()` for the channel's whole lifetime, which makes retuning the
//!   *same* timer while a *long-lived* channel still holds it a compile error.
//!   Declaring the channel fresh inside each note's branch means its borrow of
//!   `timer` ends before the *next* note's `timer.configure()` call. See the
//!   C3 variant's doc comment for the full derivation against the pinned
//!   esp-hal sources.
//! - The LEDC timer uses [`Duty::Duty10Bit`](esp_hal::ledc::timer::config::Duty::Duty10Bit)
//!   (1024 duty levels) to stay identical to the C3 variant. esp-hal's
//!   `LowSpeed` LEDC timer on the ESP32-C6 has no RC_FAST/ref-tick fallback;
//!   the divisor is computed from `apb_clk_frequency()`, which with
//!   `Config::default()` is 40 MHz on the C6 (PLL 480 MHz → HP_ROOT 160 MHz →
//!   AHB/APB 40 MHz) versus 80 MHz on the C3. At 10-bit resolution that
//!   divisor model covers roughly 38 Hz-39 kHz, comfortably spanning this
//!   melody.
//! - **Unverified on hardware — expect the pitch to come out one octave high.**
//!   The pinned esp-hal 1.2.2 computes the C6 divisor from the 40 MHz APB
//!   figure above, but `LSGlobalClkSource::APBClk` on the C6 actually writes
//!   `PCR.LEDC_SCLK_CONF.ledc_sclk_sel = 1`, which the C6 PAC documents as the
//!   fixed 80 MHz `PLL_F80M` branch (and the esp-hal clock tree configures
//!   `ledc_sclk: PllF80m`). The hardware divides 80 MHz where the software
//!   assumed 40 MHz, so every requested frequency should sound doubled (C5
//!   as C6, etc.). This was established from source only; if a scope or tuner
//!   confirms it on a board, halve the note frequencies here (or report the
//!   mismatch upstream) and drop this caveat.

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embedded_hal::pwm::SetDutyCycle;
use esp_hal::{
    delay::Delay,
    gpio::Pin,
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    main,
    time::{Instant, Rate},
};
use esp_println::println;
use tamer::tone::{Note, SequenceEvent, SequenceMode, ToneOutput, ToneSequencer};

// Ascending power-on arpeggio (C5 E5 G5 C6), a short rest, then it loops.
// Durations are in milliseconds (we feed a millis clock to update()).
const MELODY: [Note; 5] = [
    Note::new(523, 160, 200),  // C5
    Note::new(659, 160, 200),  // E5
    Note::new(784, 160, 200),  // G5
    Note::new(1047, 240, 220), // C6 (held a touch longer, a touch louder)
    Note::rest(600),           // gap before the loop repeats
];
// SequenceMode::Loop so the jingle repeats forever — a clear, audible on-device test.

// Duty10Bit => 1024 duty levels; see the "Caveats" doc section above.
const MAX_DUTY: u32 = 1024;

const POLL_INTERVAL_MS: u32 = 5;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    // Owned outside the note loop: each note gets a fresh short-lived
    // `pin.reborrow()` rather than moving the pin itself (which `Channel`
    // would consume for good). See the "Caveats" doc section above.
    let mut pin = peripherals.GPIO6.degrade();
    let delay = Delay::new();

    let mut sequencer = ToneSequencer::new(&MELODY, SequenceMode::Loop);
    let now0_ms: u64 = Instant::now().duration_since_epoch().as_millis();
    sequencer.start(now0_ms);

    println!("Buzzer ready on GPIO 6 — power-on arpeggio looping.");

    let mut last: Option<ToneOutput> = None;

    loop {
        let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

        match sequencer.update(now_ms) {
            Some(SequenceEvent::NoteChanged(i)) => println!("t={} ms  note {}", now_ms, i),
            Some(SequenceEvent::Finished) => println!("t={} ms  sequence finished", now_ms),
            None => {}
        }

        let out = sequencer.output();

        if last != Some(out) {
            if out.frequency_hz == 0 {
                // Rest: rebuild the channel at duty 0, skipping the timer
                // retune entirely — there is no frequency to apply.
                let mut channel =
                    ledc.channel::<LowSpeed>(channel::Number::Channel0, pin.reborrow());
                match channel.configure(channel::config::Config {
                    timer: &timer,
                    duty_pct: 0,
                    drive_mode: esp_hal::gpio::DriveMode::PushPull,
                }) {
                    Ok(()) => {
                        if let Err(err) = channel.set_duty_cycle(0) {
                            println!("failed to silence buzzer duty: {:?}", err);
                        }
                    }
                    Err(err) => println!("failed to configure buzzer channel for rest: {:?}", err),
                }
            } else if let Err(err) = timer.configure(timer::config::Config {
                duty: timer::config::Duty::Duty10Bit,
                clock_source: timer::LSClockSource::APBClk,
                frequency: Rate::from_hz(out.frequency_hz),
            }) {
                // Fallible by design: a note frequency outside the timer's
                // representable divisor range must never panic a device with
                // no attached developer — log and skip this note's retune
                // (and its channel/duty) rather than sound the wrong pitch.
                println!(
                    "note frequency {} Hz not representable at Duty10Bit; skipping retune: {:?}",
                    out.frequency_hz, err
                );
            } else {
                // Rebuild the channel from a fresh pin reborrow, bound to the
                // just-reconfigured timer. Declaring it here (inside the loop
                // body) means its borrow of `timer` ends when this
                // iteration's `channel` goes out of scope — before the
                // *next* note's `timer.configure()` call above. See the
                // "Caveats" doc section for why this differs from
                // hal_c3_poti_led.rs.
                let mut channel =
                    ledc.channel::<LowSpeed>(channel::Number::Channel0, pin.reborrow());
                match channel.configure(channel::config::Config {
                    timer: &timer,
                    duty_pct: 0,
                    drive_mode: esp_hal::gpio::DriveMode::PushPull,
                }) {
                    Ok(()) => {
                        let duty = (MAX_DUTY / 2 * u32::from(out.amplitude) / 255) as u16;
                        if let Err(err) = channel.set_duty_cycle(duty) {
                            println!("failed to set buzzer duty: {:?}", err);
                        }
                    }
                    Err(err) => println!("failed to configure buzzer channel: {:?}", err),
                }
            }

            // Advance `last` even when a retune above failed: a divisor failure
            // is deterministic given the fixed clock+resolution, so retrying the
            // identical frequency on every 5 ms poll for the rest of this note
            // would never succeed — don't spin on it.
            last = Some(out);
        }

        delay.delay_millis(POLL_INTERVAL_MS);
    }
}
