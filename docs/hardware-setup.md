# Hardware Setup Guide

This guide covers the physical wiring and pin configuration for running `rustyfarian-peripherals` input and output drivers on supported boards.
It is populated with per-board examples: rotary encoder, passive-piezo arpeggio, and active-buzzer Morse keying.
Add new sections per input/output type and board, using the table formats shown below — one wiring table per driver type per board.

---

## Supported boards

ESP32 (RISC-V and Xtensa), via the esp-hal (bare-metal) and esp-idf (std) tiers.
Specific dev boards are listed here as examples target them.

| Board                 | Chip     | Tier(s)          | Status                                                                                                                                                   |
|:----------------------|:---------|:-----------------|:---------------------------------------------------------------------------------------------------------------------------------------------------------|
| CrowPanel 1.28" HMI   | ESP32-S3 | esp-idf          | rotary encoder (idf_s3_rotary)                                                                                                                           |
| ESP32-C3 SuperMini    | ESP32-C3 | esp-hal, esp-idf | passive arpeggio (hal_c3_passive_buzzer_arpeggio, idf_c3_passive_buzzer_arpeggio); active Morse (hal_c3_active_buzzer_morse, idf_c3_active_buzzer_morse) |

---

## Wiring — debounced button (template)

Fill in when the debounce driver lands.

| Signal  | GPIO  | Pull                       | Notes                     |
|:--------|:------|:---------------------------|:--------------------------|
| Button  | —     | internal/external pull-up? | active-low vs active-high |

General notes to capture here when known: required pull direction, whether the
chip's internal pull resistors are used or external ones are needed, and any
contact-bounce timing observed (feeds the debounce window default in `tamer`).

---

## Wiring — EC11 rotary encoder (CrowPanel 1.28" HMI / ESP32-S3)

EC11 full-step encoder (4 quadrature states per detent) with integral push button.

| Signal      | GPIO    | Pull             | Notes                                           |
|:------------|:--------|:-----------------|:------------------------------------------------|
| A / CLK     | GPIO 45 | internal pull-up | quadrature channel A; persistent AnyEdge ISR    |
| B / DT      | GPIO 42 | internal pull-up | quadrature channel B; persistent AnyEdge ISR    |
| Button / SW | GPIO 41 | internal pull-up | active-low (pressed = LOW); polled for debounce |
| +           | 3V3     | —                | power                                           |
| −           | GND     | —                | ground                                          |

The A and B quadrature channels are monitored via persistent AnyEdge GPIO interrupts
registered directly against the ESP-IDF C API; every edge is captured regardless of
main-loop latency. Button timing (debounce, click, double-click, long-press) is
polled via [`Encoder::update`], so call it regularly (a 1 ms loop is typical).
The driver delegates all decode logic to `tamer::rotary::QuadratureDecoder` and
`tamer::button::ButtonDecoder`.

---

## Wiring — passive-piezo arpeggio (ESP32-C3 SuperMini)

Passive piezo driven by PWM frequency modulation to play melodies via `tamer::tone::ToneSequencer`.
Examples: `hal_c3_passive_buzzer_arpeggio` / `idf_c3_passive_buzzer_arpeggio`.

| Signal     | GPIO   | Notes                                     |
|:-----------|:-------|:------------------------------------------|
| + (signal) | GPIO 6 | LEDC PWM output; frequency tuned per note |
| − (ground) | GND    | ground                                    |

A passive piezo is a two-terminal element: the signal terminal is driven from GPIO 6 and the other returns to GND — there is no separate supply pin.
The passive piezo is driven by varying the PWM carrier frequency to produce the acoustic pitch.
All melody sequencing is pure logic in `tamer::tone`; only the PWM write happens in the example.

---

## Wiring — active-buzzer Morse keying (ESP32-C3 SuperMini)

Active electromagnetic buzzer (fixed internal oscillator) keyed on/off to transmit Morse code via `tamer::morse::MorseKeyer`.
Examples: `hal_c3_active_buzzer_morse` / `idf_c3_active_buzzer_morse`.

| Signal     | GPIO   | Notes                                  |
|:-----------|:-------|:---------------------------------------|
| + (signal) | GPIO 6 | on/off keying output; HIGH = buzzer on |
| − (ground) | GND    | ground                                 |

The TMB12A03 has a built-in oscillator, so keying GPIO 6 high sounds it and keying it low silences it.
All Morse timing and encoding is pure logic in `tamer::morse`; only the GPIO write happens in the example.

This is the canonical reference for driving an active buzzer from this repo's examples; the example doc-comments point here rather than repeat it.

Note: the TMB12A03 draws up to about 30 mA at 3 V, at the upper end of a C3 GPIO's safe source current.
The examples configure GPIO 6 for the maximum 40 mA drive strength, so a small, high-impedance unit runs directly off the pin as wired above.
For a much larger or lower-impedance buzzer beyond that margin, drive it through a small NPN transistor (GPIO 6 through a base resistor, buzzer on the collector, emitter to GND) with a flyback diode across the buzzer, rather than direct drive.

**Caveat:** The `Standard` speed preset (60 ms dot, ~20 WPM) is hardware-dependent.
Some TMB12A03 datasheets list acoustic response times up to 50 ms at the lowest operating voltage.
A 60 ms dot leaves little margin, especially with 5 ms polling.
Keep `Relaxed` (100 ms dot, ~12 WPM) as the default and verify `Standard` on your exact unit before relying on it in production.
