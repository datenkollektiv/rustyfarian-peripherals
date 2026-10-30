# Hardware Setup Guide

This guide covers the physical wiring and pin configuration for running `rustyfarian-peripherals` input and output drivers on supported boards.
It is populated with per-board examples: rotary encoder, passive-piezo arpeggio, active-buzzer Morse keying, and MPU6050 tilt over I2C.
Add new sections per input/output type and board, using the table formats shown below — one wiring table per driver type per board.

---

## Supported boards

ESP32 (RISC-V and Xtensa), via the esp-hal (bare-metal) and esp-idf (std) tiers.
Specific dev boards are listed here as examples target them.

| Board               | Chip     | Tier(s)          | Status                                                                                                                                                                                                            |
|:--------------------|:---------|:-----------------|:------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| CrowPanel 1.28" HMI | ESP32-S3 | esp-hal, esp-idf | rotary encoder (hal_s3_rotary, idf_s3_rotary)                                                                                                                                                                     |
| ESP32-C3 SuperMini  | ESP32-C3 | esp-hal, esp-idf | passive arpeggio (hal_c3_passive_buzzer_arpeggio, idf_c3_passive_buzzer_arpeggio); active Morse (hal_c3_active_buzzer_morse, idf_c3_active_buzzer_morse); MPU6050 tilt (hal_c3_mpu6050_tilt, idf_c3_mpu6050_tilt) |
| ESP32-C6-DevKitC-1  | ESP32-C6 | esp-hal          | button (hal_c6_b3f); passive arpeggio (hal_c6_passive_buzzer_arpeggio)                                                                                                                                            |

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

## Wiring — passive-piezo arpeggio (ESP32-C3 SuperMini, ESP32-C6-DevKitC-1)

Passive piezo driven by PWM frequency modulation to play melodies via `tamer::tone::ToneSequencer`.
Examples: `hal_c3_passive_buzzer_arpeggio` / `idf_c3_passive_buzzer_arpeggio` / `hal_c6_passive_buzzer_arpeggio`.
The same GPIO 6 wiring applies on both chips; on the C6 it is likewise clear of strapping, USB, UART-console and flash pins.
The C6 example has not been run on a board yet; it is compile-verified only.
A source reading of esp-hal 1.2.2 suggests its notes may sound one octave high on the C6 (see the example's Caveats).

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

---

## Wiring — MPU6050 tilt (ESP32-C3 SuperMini)

MPU6050 6-axis IMU on I2C0, read by `tamer::mpu6050` and reduced to a tilt angle by `tamer::TiltEstimator`.
Examples: `hal_c3_mpu6050_tilt` / `idf_c3_mpu6050_tilt`.

| Signal | Pin on breakout | ESP32-C3 GPIO | Notes                                                  |
|:-------|:----------------|:--------------|:-------------------------------------------------------|
| VCC    | VCC             | 3V3           | 3.3 V supply                                           |
| GND    | GND             | GND           | ground                                                 |
| SDA    | SDA             | GPIO 4        | I2C0 data; 3.3 V logic, never connect to 5 V           |
| SCL    | SCL             | GPIO 5        | I2C0 clock at 100 kHz; 3.3 V logic                     |
| AD0    | AD0             | GND or open   | sets the address: low or open = 0x68; high = 0x69      |
| INT    | INT             | unused        | not wired; the examples poll at about 5 Hz             |

Most GY-521 breakouts already carry 4.7 kΩ pull-ups on SDA and SCL to 3V3, so no extra resistors are needed.
A bare MPU6050 chip needs external 4.7 kΩ pull-ups on both lines.
Check the board's schematic before adding resistors, because a second pair in parallel lowers the effective pull-up value.

The board must rest level, with the Z axis up, during calibration.
The 100-sample calibration takes the zero point from that pose, so moving the board during the sampling window biases every later reading.

After `WHO_AM_I` reports `0x68` and calibration completes, the example prints a tilt angle in degrees about five times per second.
The angle should read close to 0 when the board lies flat and change as the board is tilted; the exact line format is in the example's doc comment.

Validated 2026-10-30 with `hal_c3_mpu6050_tilt` on an ESP32-C3 SuperMini and a GY-521 with AD0 unwired: `WHO_AM_I` `0x68`, resting offsets of a few hundred LSB, a steady `0.0°` when level, and a smooth, jitter-free ramp while tilting.
`idf_c3_mpu6050_tilt` is compile-verified only until it has been run on the same wiring.

Isolated `read failed (1 in a row): … AcknowledgeCheckFailed` lines between good readings, together with the breakout's power LED dimming as the module moves, mean a loose VCC or GND jumper, not a bus or driver fault.
The sensor browns out for a moment and cannot ACK; reseat the supply jumpers (or solder the header) and the NACKs stop.
