# Hardware Setup Guide

This guide covers the physical wiring and pin configuration for running `rustyfarian-peripherals` input and output drivers on supported boards.
It is populated with per-board examples: rotary encoder, passive-piezo arpeggio, active-buzzer Morse keying, and MPU6050 tilt over I2C.
Add new sections per input/output type and board, using the table formats shown below — one wiring table per driver type per board.

---

## Supported boards

ESP32 (RISC-V and Xtensa), via the esp-hal (bare-metal) and esp-idf (std) tiers.
Specific dev boards are listed here as examples target them.

| Board                       | Chip             | Tier(s)          | Status                                                                                                                                                                                                                                                                                                                           |
|:----------------------------|:-----------------|:-----------------|:---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| CrowPanel 1.28" HMI         | ESP32-S3         | esp-hal, esp-idf | button (hal_s3_b3f, idf_s3_b3f); rotary encoder (hal_s3_rotary, idf_s3_rotary)                                                                                                                                                                                                                                                   |
| ESP32-C3 SuperMini          | ESP32-C3         | esp-hal, esp-idf | passive arpeggio (hal_c3_passive_buzzer_arpeggio, idf_c3_passive_buzzer_arpeggio); active Morse (hal_c3_active_buzzer_morse, idf_c3_active_buzzer_morse); BME680 (hal_c3_bme680, hal_c3_bme680_async, idf_c3_bme680); MPU6050 tilt (hal_c3_mpu6050_tilt, idf_c3_mpu6050_tilt); rotary encoder (hal_c3_rotary_isr, idf_c3_rotary) |
| ESP32-C6-DevKitC-1          | ESP32-C6         | esp-hal, esp-idf | button (hal_c6_b3f, idf_c6_b3f); passive arpeggio (hal_c6_passive_buzzer_arpeggio); rotary encoder (hal_c6_rotary_isr, idf_c6_rotary)                                                                                                                                                                                            |
| ESP32 WROOM-32 / Feather V2 | ESP32            | esp-hal, esp-idf | button (hal_esp_b3f, idf_esp_b3f); rotary encoder (hal_esp_rotary_isr, idf_esp_rotary)                                                                                                                                                                                                                                           |
| Arduino Nano                | ATmega328P (AVR) | tamer only       | debounced button on D2 (`examples/avr-nano-tamer`); no tier crate                                                                                                                                                                                                                                                                |

---

## Wiring — C3 bench rig (ESP32-C3 SuperMini)

The closed C3 bench rig wires every ESP32-C3 example to one pin map, so no board needs a mixed assembly.
The map is decided in [ADR-012](adr/012-esp32-c3-pin-map.md); the sections below repeat it per part for wiring.

| Part                                      | GPIO       | Pull / drive                         | Notes                                                    |
|:------------------------------------------|:-----------|:-------------------------------------|:---------------------------------------------------------|
| Button (B3F to GND)                       | 3          | internal pull-up                     | active-low; wake-capable                                 |
| 3-pin pogo signal (analog sensor, PIR, RGB data) | 4   | none (ADC1 input)                    | wake-capable                                             |
| I2C SDA / SCL                             | 6 / 7      | 4.7 kΩ pull-ups to 3V3 on the rig    | 100 kHz; the OLED module has no pull-ups                 |
| Buzzer (passive, NPN driver)              | 10         | NPN base through a base resistor     | no wake duty                                             |
| LED (active-low)                          | 8          | anode to 3V3 through 330 Ω           | strapping pin, held high at boot; on-board LED mirrors it |
| Rotary A / B / click                      | 0 / 1 / 5  | internal pull-up                     | see the rotary section; 0, 1 and 5 are wake-capable      |

GPIO 2 and 9 are strapping pins, and GPIO 18 and 19 (USB) and GPIO 20 and 21 (UART) are not used on this rig.
Only GPIO 0 to 5 can wake the C3 from deep sleep, which is why the inputs sit on 3, 4 and 5, and the outputs on 8 and 10.

### Connector pin order

The 3-pin pogo header uses the order GND, 3V3, signal.

| Pin | Signal | ESP32-C3 GPIO | Notes                               |
|:----|:-------|:--------------|:------------------------------------|
| 1   | GND    | GND           | ground                              |
| 2   | 3V3    | 3V3           | 3.3 V supply                        |
| 3   | SIG    | GPIO 4        | analog sensor, PIR or RGB data     |

The 4-pin I2C header uses the Qwiic / STEMMA QT order GND, 3V3, SDA, SCL, as described in [the shared I2C pogo header](#wiring--shared-i2c-pogo-header-esp32-c3-supermini).

- **Button:** one leg of the B3F to GPIO 3 and the other leg to GND; the internal pull-up holds the pin high until the button is pressed.
- **Buzzer:** the passive buzzer's positive terminal goes to 3V3 and its negative terminal to the NPN collector; the emitter goes to GND, GPIO 10 drives the base through a base resistor, and a flyback diode sits across the buzzer.
- **LED:** the anode goes to 3V3 through a 330 Ω resistor and the cathode goes to GPIO 8, so the LED lights when GPIO 8 is low.
  The on-board SuperMini LED mirrors the same signal.

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

## Wiring — rotary encoder (ESP32 WROOM-32 / Feather V2)

EC11 full-step encoder with integral push button, wired to external pins on this board.
Examples: `hal_esp_rotary_isr` / `idf_esp_rotary`.

| Signal      | GPIO    | Pull             | Notes                                              |
|:------------|:--------|:-----------------|:---------------------------------------------------|
| A / CLK     | GPIO 32 | internal pull-up | quadrature channel A; ISR-decoded                  |
| B / DT      | GPIO 33 | internal pull-up | quadrature channel B; ISR-decoded                  |
| Button / SW | GPIO 27 | internal pull-up | active-low (pressed = LOW); polled for debounce    |
| +           | 3V3     | —                | power                                              |
| −           | GND     | —                | ground                                             |

GPIO 34 to 39 are input-only and have no internal pull-ups, which is why the pins above avoid them.
Confirm GPIO 32, 33 and 27 on the Adafruit Feather V2 pinout card before wiring; this guide does not carry the card.
The pins are 3.3 V logic, so never connect them to 5 V.

---

## Wiring — rotary encoder (ESP32-C3 SuperMini)

EC11 full-step encoder with integral push button, wired to external pins on this board.
Examples: `hal_c3_rotary_isr` / `idf_c3_rotary`.

| Signal      | GPIO   | Pull             | Notes                                              |
|:------------|:-------|:-----------------|:---------------------------------------------------|
| A / CLK     | GPIO 0 | internal pull-up | quadrature channel A; ISR-decoded; wake-capable    |
| B / DT      | GPIO 1 | internal pull-up | quadrature channel B; ISR-decoded; wake-capable    |
| Button / SW | GPIO 5 | internal pull-up | active-low (pressed = LOW); polled; wake-capable   |
| +           | 3V3    | —                | power                                              |
| −           | GND    | —                | ground                                             |

The encoder sits on GPIO 0, 1 and 5 because only GPIO 0 to 5 can wake the C3 from deep sleep, so the click can wake the device.
GPIO 10 is the buzzer on the bench rig, which is why the click is not on 10.
GPIO 2, 8 and 9 are strapping pins, so the encoder avoids them.
The full map is in the [C3 bench rig](#wiring--c3-bench-rig-esp32-c3-supermini) section and [ADR-012](adr/012-esp32-c3-pin-map.md).
The pins are 3.3 V logic, so never connect them to 5 V.

---

## Wiring — rotary encoder (ESP32-C6-DevKitC-1)

EC11 full-step encoder with integral push button, wired to external pins on this board.
Examples: `hal_c6_rotary_isr` / `idf_c6_rotary`.

| Signal      | GPIO    | Pull             | Notes                                              |
|:------------|:--------|:-----------------|:---------------------------------------------------|
| A / CLK     | GPIO 18 | internal pull-up | quadrature channel A; ISR-decoded                  |
| B / DT      | GPIO 19 | internal pull-up | quadrature channel B; ISR-decoded                  |
| Button / SW | GPIO 20 | internal pull-up | active-low (pressed = LOW); polled for debounce    |
| +           | 3V3     | —                | power                                              |
| −           | GND     | —                | ground                                             |

The three pins sit adjacent on J3 and are clear of the strapping, USB and UART pins.
The pins are 3.3 V logic, so never connect them to 5 V.

---

## Wiring — Arduino Nano button (examples/avr-nano-tamer)

Momentary button on D2, debounced through `tamer::ButtonInput` on an ATmega328P (5 V logic board).
The example toggles D13 on each click and prints events over serial at 57600 baud.

| Signal   | Pin  | Pull             | Notes                                              |
|:---------|:-----|:-----------------|:---------------------------------------------------|
| Button   | D2   | internal pull-up | active-low (pressed = LOW); enabled in the code    |
| −        | GND  | —                | ground; the other button leg                       |
| LED      | D13  | —                | on-board LED, toggled on each click; no wiring     |

The internal pull-up is the only pull, so the button needs no external resistor.
The debounce window is 20 ticks of 1 ms.
Flash and monitor through the example's ravedude runner, which uses the same 57600 baud.

---

## Wiring — passive-piezo arpeggio (ESP32-C3 SuperMini, ESP32-C6-DevKitC-1)

Passive piezo driven by PWM frequency modulation to play melodies via `tamer::tone::ToneSequencer`.
Examples: `hal_c3_passive_buzzer_arpeggio` / `idf_c3_passive_buzzer_arpeggio` / `hal_c6_passive_buzzer_arpeggio`.
The C3 drives the buzzer from GPIO 10; the C6 drives it from GPIO 6, which is likewise clear of strapping, USB, UART-console and flash pins on that chip.
The C6 example has not been run on a board yet; it is compile-verified only.
A source reading of esp-hal 1.2.2 suggests its notes may sound one octave high on the C6 (see the example's Caveats).

| Signal     | GPIO                         | Notes                                     |
|:-----------|:-----------------------------|:------------------------------------------|
| + (signal) | GPIO 10 (C3) / GPIO 6 (C6)   | LEDC PWM output; frequency tuned per note |
| − (ground) | GND                          | ground                                    |

A passive piezo is a two-terminal element: the signal terminal is driven from the GPIO above and the other returns to GND — there is no separate supply pin.
The C3 bench rig drives the passive buzzer through an NPN transistor, as the [C3 bench rig](#wiring--c3-bench-rig-esp32-c3-supermini) section describes.
The passive piezo is driven by varying the PWM carrier frequency to produce the acoustic pitch.
All melody sequencing is pure logic in `tamer::tone`; only the PWM write happens in the example.

---

## Wiring — active-buzzer Morse keying (ESP32-C3 SuperMini)

Active electromagnetic buzzer (fixed internal oscillator) keyed on/off to transmit Morse code via `tamer::morse::MorseKeyer`.
Examples: `hal_c3_active_buzzer_morse` / `idf_c3_active_buzzer_morse`.

| Signal     | GPIO   | Notes                                  |
|:-----------|:-------|:---------------------------------------|
| + (signal) | GPIO 10 | on/off keying output; HIGH = buzzer on |
| − (ground) | GND    | ground                                 |

The TMB12A03 has a built-in oscillator, so keying GPIO 10 high sounds it and keying it low silences it.
All Morse timing and encoding is pure logic in `tamer::morse`; only the GPIO write happens in the example.

This is the canonical reference for driving an active buzzer from this repo's examples; the example doc-comments point here rather than repeat it.

Note: the TMB12A03 draws up to about 30 mA at 3 V, at the upper end of a C3 GPIO's safe source current.
The examples configure GPIO 10 for the maximum 40 mA drive strength, so a small, high-impedance unit runs directly off the pin as wired above.
For a much larger or lower-impedance buzzer beyond that margin, drive it through a small NPN transistor (GPIO 10 through a base resistor, buzzer on the collector, emitter to GND) with a flyback diode across the buzzer, rather than direct drive.

**Caveat:** The `Standard` speed preset (60 ms dot, ~20 WPM) is hardware-dependent.
Some TMB12A03 datasheets list acoustic response times up to 50 ms at the lowest operating voltage.
A 60 ms dot leaves little margin, especially with 5 ms polling.
Keep `Relaxed` (100 ms dot, ~12 WPM) as the default and verify `Standard` on your exact unit before relying on it in production.

---

## Wiring — shared I2C pogo header (ESP32-C3 SuperMini)

One 4-pin header on the C3 I2C bus (GPIO 6 and 7, [ADR-012](adr/012-esp32-c3-pin-map.md)) lets a BME680, an I2C OLED display and an MPU6050 be plugged in alone or together, e.g. through pogo pins for debugging.
Pin order follows the Qwiic / STEMMA QT convention.

| Header pin | Signal | ESP32-C3 GPIO | Notes                                         |
|:-----------|:-------|:--------------|:----------------------------------------------|
| 1          | GND    | GND           | ground                                        |
| 2          | 3V3    | 3V3           | 3.3 V supply                                  |
| 3          | SDA    | GPIO 6        | I2C0 data; 3.3 V logic, never connect to 5 V  |
| 4          | SCL    | GPIO 7        | I2C0 clock at 100 kHz; 3.3 V logic            |

Devices on the header and their default addresses:

| Device                | Address        | Alternate address (strap)     | Examples                                                |
|:----------------------|:---------------|:------------------------------|:--------------------------------------------------------|
| BME680                | 0x76 (SDO low) | 0x77 (SDO high)               | `hal_c3_bme680`, `hal_c3_bme680_async`, `idf_c3_bme680` |
| SSD1306 / SH1106 OLED | 0x3C           | 0x3D (SA0 / address resistor) | `hal_c3_oled`, `idf_c3_oled` (SSD1306 128x64)           |
| MPU6050 (GY-521)      | 0x68 (AD0 low) | 0x69 (AD0 high)               | `hal_c3_mpu6050_tilt`, `idf_c3_mpu6050_tilt`            |

The default addresses are distinct, so all three can share the bus without a conflict.
Run `hal_c3_i2c_scan` to confirm what answers.

Electrical notes:

- Each breakout normally carries its own SDA/SCL pull-ups, and they add in parallel.
  Two or three boards are fine at 100 kHz; with more, remove pull-ups from all but one board.
- The C3 bench rig carries 4.7 kΩ pull-ups on SDA and SCL to 3V3, because the OLED module has none.
  Breakouts plugged into the rig add their own pull-ups in parallel, so count them before adding more.
- Plug and unplug with the power off, or reset the board afterwards: hot-plugging can leave a device holding SDA low or latch up a brown-out.
- SPI displays cannot use this header; they need MOSI, SCK, CS and DC pins instead of SDA/SCL.

---

## Wiring — MPU6050 tilt (ESP32-C3 SuperMini)

MPU6050 6-axis IMU on I2C0, read by `tamer::mpu6050` and reduced to a tilt angle by `tamer::TiltEstimator`.
Examples: `hal_c3_mpu6050_tilt` / `idf_c3_mpu6050_tilt`.

| Signal | Pin on breakout | ESP32-C3 GPIO | Notes                                                  |
|:-------|:----------------|:--------------|:-------------------------------------------------------|
| VCC    | VCC             | 3V3           | 3.3 V supply                                           |
| GND    | GND             | GND           | ground                                                 |
| SDA    | SDA             | GPIO 6        | I2C0 data; 3.3 V logic, never connect to 5 V           |
| SCL    | SCL             | GPIO 7        | I2C0 clock at 100 kHz; 3.3 V logic                     |
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
That run used the old SDA GPIO 4 / SCL GPIO 5 wiring; the bus moved to GPIO 6 / 7 with [ADR-012](adr/012-esp32-c3-pin-map.md), so the example needs a rerun on the new wiring.
`idf_c3_mpu6050_tilt` is compile-verified only until it has been run on the same wiring.

Isolated `read failed (1 in a row): … AcknowledgeCheckFailed` lines between good readings, together with the breakout's power LED dimming as the module moves, mean a loose VCC or GND jumper, not a bus or driver fault.
The sensor browns out for a moment and cannot ACK; reseat the supply jumpers (or solder the header) and the NACKs stop.
