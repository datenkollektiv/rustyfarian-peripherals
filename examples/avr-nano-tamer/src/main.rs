//! Arduino Nano — `tamer` button demo.
//!
//! Debounces a momentary button on D2 through `tamer::ButtonInput` (the
//! embedded-hal adapter over the pure `ButtonDecoder`), toggles the on-board
//! LED (D13) on every click, and reports the button events over serial. The
//! decode logic itself is host-tested in `tamer`; this binary only proves that
//! the `hal` feature links on the real `avr-none` target.
//!
//! ## Wiring
//!
//! ```text
//! Arduino Nano             Button
//! ────────────             ──────
//! D2  ──────────────────── one leg (internal pull-up enabled)
//! GND ──────────────────── other leg
//! ```
//!
//! ## Build
//!
//! From the workspace root: `just build-avr-example`.

#![no_std]
#![no_main]

use panic_halt as _;
use tamer::{ButtonEvent, ButtonInput};

/// Debounce window in loop ticks (one tick = one 1 ms delay).
const DEBOUNCE_TICKS: u64 = 20;
/// Hold time that counts as a long press, in ticks.
const LONG_PRESS_TICKS: u64 = 1000;
/// Window for a second tap to count as a double click, in ticks.
const DOUBLE_CLICK_TICKS: u64 = 300;

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 57600);

    let mut led = pins.d13.into_output();

    // Active-low: the button pulls D2 to GND through the internal pull-up.
    let button = pins.d2.into_pull_up_input();
    let mut input = ButtonInput::try_from_pin(
        button,
        true,
        DEBOUNCE_TICKS,
        LONG_PRESS_TICKS,
        DOUBLE_CLICK_TICKS,
    )
    .unwrap();

    let mut now: u64 = 0;
    loop {
        if let Some(event) = input.update(now).unwrap() {
            match event {
                ButtonEvent::Press => ufmt::uwriteln!(&mut serial, "press").unwrap(),
                ButtonEvent::Release => ufmt::uwriteln!(&mut serial, "release").unwrap(),
                ButtonEvent::Click => {
                    led.toggle();
                    ufmt::uwriteln!(&mut serial, "click").unwrap();
                }
                ButtonEvent::DoubleClick => ufmt::uwriteln!(&mut serial, "double click").unwrap(),
                ButtonEvent::LongPress => ufmt::uwriteln!(&mut serial, "long press").unwrap(),
            }
        }
        arduino_hal::delay_ms(1);
        now += 1;
    }
}
