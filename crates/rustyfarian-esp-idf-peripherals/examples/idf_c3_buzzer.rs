//! ESP32-C3 — Passive piezo buzzer sequenced by `tamer::tone::ToneSequencer`
//! (ESP-IDF / std)
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
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
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
//! Piezo buzzer       ESP32-C3
//! ─────────────      ────────
//! + (signal)          GPIO 6
//! − (ground)          GND
//! ```
//!
//! GPIO 6 is not a strapping pin (2/8/9), not the on-board WS2812 (8), not
//! USB (18/19), not the UART console (20/21), and not in-package SPI flash
//! (11-17) — a safe general-purpose PWM output on common ESP32-C3 boards.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_buzzer
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_buzzer
//! ```
//!
//! ## Caveats
//!
//! - Duty cycle here is a coarse "volume" proxy, not a true digital amplitude
//!   control: scaling duty away from ~50% reduces the delivered squarewave
//!   energy, which perceptibly (if crudely) affects loudness on most passive
//!   piezo buzzers — it is not calibrated, linear volume.
//! - ESP-IDF's LEDC driver picks its clock source (APB, XTAL, or the internal
//!   RC_FAST oscillator) **once**, at [`LedcTimerDriver::new`] time, and
//!   reuses that same source for every later
//!   [`set_frequency`](LedcTimerDriver::set_frequency) call — it does not
//!   re-run clock selection per retune. `LEDC_AUTO_CLK` tries APB first, then
//!   XTAL, then RC_FAST, taking the first clock whose divisor is valid. APB
//!   (80 MHz) is valid at 8-bit duty resolution down to roughly 305 Hz, which
//!   spans this melody's full 523–1047 Hz range — so APB (crystal/PLL-derived
//!   and stable), *not* RC_FAST, is what actually gets locked in here.
//!   Constructing the timer at [`MIN_FREQ_HZ`] (the melody's lowest note) is
//!   still the right defensive habit: if a future note dropped below ~305 Hz,
//!   constructing at a *higher* note first would lock in APB and then reject
//!   that lower note's retune outright, whereas constructing at the lowest
//!   note first gives `LEDC_AUTO_CLK` its best chance to pick a clock that
//!   spans the whole table (falling through to the less stable XTAL/RC_FAST
//!   only if even APB can't represent it — where pitch would drift audibly).

use esp_idf_hal::{
    delay::FreeRtos,
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver, Resolution},
    peripherals::Peripherals,
    units::Hertz,
};
use std::time::Instant;
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
// Lowest tone in MELODY — drives the LEDC timer construction (see "Caveats").
// Keep in sync with MELODY's lowest `frequency_hz`.
const MIN_FREQ_HZ: u32 = 523;

const POLL_INTERVAL_MS: u32 = 5;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;

    // Construct the timer at the melody's LOWEST frequency — see the "Caveats"
    // section above: the clock source is picked once here (via LEDC_AUTO_CLK)
    // and reused, not re-evaluated, by every later `set_frequency()` call. For
    // this 523–1047 Hz melody that clock is APB; constructing at the lowest
    // note keeps the choice valid if a future edit adds a lower note.
    let timer_config = TimerConfig::new()
        .frequency(Hertz(MIN_FREQ_HZ))
        .resolution(Resolution::Bits8);
    let mut timer_driver = LedcTimerDriver::new(peripherals.ledc.timer0, &timer_config)?;

    // Pass the timer driver BY REFERENCE, not by value. `LedcDriver::new` only
    // copies `max_duty`/`timer()` out of it (esp-idf-hal's `Borrow`-based
    // constructor) rather than retaining a borrow, so `timer_driver` stays
    // alive and mutable here for us to keep calling `set_frequency` on it per
    // note. Moving it in by value instead (as idf_c3_poti_led.rs does, since
    // that example never retunes) would drop it — via `LedcTimerDriver`'s
    // `Drop` impl — at the end of this call, permanently losing
    // `set_frequency` access.
    let mut led = LedcDriver::new(
        peripherals.ledc.channel0,
        &timer_driver,
        peripherals.pins.gpio6,
    )?;

    // The LEDC timer above is 8-bit, so max_duty is 256; scale a ~50% square
    // wave by the note's amplitude (0..=255) for a coarse "volume" proxy.
    let max_duty = led.get_max_duty();
    let half_duty = max_duty / 2;

    let mut sequencer = ToneSequencer::new(&MELODY, SequenceMode::Loop);
    let start = Instant::now();
    // as_millis() returns u128; safe to truncate — u64 overflows after ~585M years.
    sequencer.start(start.elapsed().as_millis() as u64);

    log::info!("Buzzer ready on GPIO 6 — power-on arpeggio looping.");

    let mut last: Option<ToneOutput> = None;

    loop {
        let now_ms = start.elapsed().as_millis() as u64;

        match sequencer.update(now_ms) {
            Some(SequenceEvent::NoteChanged(i)) => log::info!("t={} ms  note {}", now_ms, i),
            Some(SequenceEvent::Finished) => log::info!("t={} ms  sequence finished", now_ms),
            None => {}
        }

        let out = sequencer.output();

        if last != Some(out) {
            if out.frequency_hz == 0 {
                // Rest: silence without touching the timer's locked-in clock/frequency.
                if let Err(err) = led.set_duty(0) {
                    log::warn!("failed to silence buzzer: {:?}", err);
                }
            } else if let Err(err) = timer_driver.set_frequency(Hertz(out.frequency_hz)) {
                // Fallible by design (see "Caveats" above): a note whose frequency
                // does not fit the clock source locked in at construction must
                // never panic a device with no attached developer — log and skip
                // this note's retune (and its duty) rather than sound the wrong
                // pitch at the wrong volume.
                log::warn!(
                    "note frequency {} Hz not representable on the locked LEDC clock; skipping retune: {:?}",
                    out.frequency_hz,
                    err
                );
            } else {
                let duty = half_duty * u32::from(out.amplitude) / 255;
                if let Err(err) = led.set_duty(duty) {
                    log::warn!("failed to set buzzer duty: {:?}", err);
                }
            }

            // Advance `last` even when a retune above failed: a divisor/clock
            // failure is deterministic given the fixed clock+resolution, so
            // retrying the identical frequency on every 5 ms poll for the rest
            // of this note would never succeed — don't spin on it.
            last = Some(out);
        }

        FreeRtos::delay_ms(POLL_INTERVAL_MS);
    }
}
