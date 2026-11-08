# ADR-012: ESP32-C3 pin map

## Status
Accepted (2026-11-08); the I2C pin choice in ADR-004 is amended by this record.

## Context
The closed ESP32-C3 bench rig is becoming the main C3 bench part, and the examples for both tiers must be wired to one map so that no board needs a mixed assembly.
Before fixing the map, an ecosystem survey of the rustyfarian family and its neighbours found two camps that disagree on the pins.

- This repository's [ADR-004](004-i2c-bus-pattern.md) and the beekeeper project put I2C on GPIO 4 (SDA) and GPIO 5 (SCL).
  Both also overloaded GPIO 4 as SDA while using it for a button, a potentiometer, a Hall sensor, a PIR sensor or a WS2812 data line.
- The family's deployed C3 props put the OLED on SDA 6 / SCL 7, the PIR on GPIO 4 and the cue LED on GPIO 8.
  The closed C3 bench rig also uses GPIO 6 and 7 for I2C.
- The `esp-rust-board` uses a bus on GPIO 10 and 8, but that board fixes the pins in hardware, so it is out of scope for this map.
- `rustyfarian-rgb-clock` puts its WS2812 data line on GPIO 4, described as the shared pogo-pin test rig.

Sharing GPIO 4 between I2C SDA and a sensor input is the conflict that matters, because one pin cannot serve as both the bus line and the sensor line.
The C3 also has two hardware limits that shape the map.
Only GPIO 0 to 5 can wake the chip from deep sleep, so the inputs that should wake the device belong there.
GPIO 2, 8 and 9 are strapping pins, GPIO 18 and 19 carry USB, and GPIO 20 and 21 carry the UART console.

## Decision
The C3 bench rig and every ESP32-C3 example use the following map.

| Part                                      | C3 GPIO | Notes                                                        |
|:------------------------------------------|:--------|:-------------------------------------------------------------|
| Button (B3F to GND, internal pull-up)     | 3       | wake-capable                                                 |
| 3-pin pogo (analog sensor, PIR, RGB data) | 4       | ADC1, wake-capable; unchanged                                |
| I2C SDA / SCL                             | 6 / 7   | 100 kHz; rig adds 4.7 kΩ pull-ups (the OLED module has none) |
| Buzzer (passive, NPN driver)              | 10      | no wake duty                                                 |
| LED, active-low (anode→330 Ω→3V3)         | 8       | strapping pin kept high at boot; on-board LED mirrors it     |
| Rotary A / B / click                      | 0/1/5   | click moves 10→5 (wake-capable)                              |

Never used: 2, 9 (strapping), 18/19 (USB), 20/21 (UART).

The reasons behind the placement are as follows.

- Inputs sit on GPIO 3, 4 and 5 because those are the wake-capable pins; the button, the 3-pin pogo and the rotary click can therefore wake the chip from deep sleep.
- Outputs sit on GPIO 8 and 10, which have no wake duty.
- I2C moves to GPIO 6 and 7, so that GPIO 4 is no longer shared between a bus and an input; the pins match the family's deployed C3 props and the closed rig.
- I2C runs at 100 kHz.
  The rig adds 4.7 kΩ pull-ups from SDA and SCL to 3V3, because the OLED module has none.
- The LED is active-low, with the anode on 3V3 through a 330 Ω resistor and the cathode on GPIO 8.
  GPIO 8 is a strapping pin that must stay high for download mode, and active-low wiring keeps it high while the LED is off, so the on-board LED mirrors the same signal.
- The 3-pin pogo header has GND, 3V3 and a signal pin, so it carries analog sensors, PIR sensors and WS2812 data without taking a bus.
- The rotary encoder moves its click input from GPIO 10 to GPIO 5, so that the outputs can use GPIO 10 and the inputs stay on the wake-capable pins.

## Consequences
- The examples move to the new map on both tiers.
  I2C examples (`*_c3_bme680`, `*_c3_i2c_scan`, `*_c3_mpu6050_tilt`) move from GPIO 4/5 to GPIO 6/7, the passive and active buzzer examples move from GPIO 6 to GPIO 10, the button example moves from GPIO 4 to GPIO 3, the rotary click moves from GPIO 10 to GPIO 5, and the LED examples drive GPIO 8 with active-low logic.
- The 3-pin pogo parts (poti, hall-linear, hall-switch, reed, IR proximity, tilt, touch) stay on GPIO 4.
  Their gate results on the C3 stay valid because the pin does not change.
- The C6 and ESP32 examples are unaffected; this map covers only the C3.
- The pin-map change is an example-level change.
  The `tamer` library API, the `no_std` and `std` boundary, and the tier crates' public APIs do not change.
- The beekeeper project still uses I2C on GPIO 4/5 and a battery ADC on GPIO 3.
  It needs a follow-up in its own repository; this ADR does not change it.
- [ADR-004](004-i2c-bus-pattern.md) keeps its probe methodology, its NACK classification for both tiers, and its bring-up-diagnostic exemption.
  Only its pin choice is amended.
- The rig must be rewired before the C3 bench checks can pass: the button moves from 4 to 3, the buzzer from 6 to 10, and the LED is added on 8.
- Any future C3 example takes its pins from this table; a pin outside it needs a new ADR that supersedes this one.

## Alternatives Considered

|                                  Alternative | Pros                                                     | Cons                                                                                   | Why Rejected                                                                                |
|----------------------------------------------:|:---------------------------------------------------------|:---------------------------------------------------------------------------------------|:--------------------------------------------------------------------------------------------|
|      Keep I2C on GPIO 4/5 (ADR-004 as written) | Matches the beekeeper project and the existing examples  | GPIO 4 is SDA and also the 3-pin pogo signal; GPIO 5 is the rotary click in the new map | The bus and the sensor input cannot share a pin on the rig, and the rig's sensor set needs GPIO 4. |
|               Keep the rotary click on GPIO 10 | No rotary example change                                 | GPIO 10 is then taken, so the buzzer has no free non-wake output pin                  | The buzzer needs GPIO 10; moving the click to wake-capable GPIO 5 frees it.                 |
|                      Put the LED on GPIO 9 or 2 | Leaves GPIO 8 free                                       | Both are strapping pins, so an output on them is not stable at boot                    | Rejected as strapping pins.                                                                  |
|            Use the esp-rust-board bus on GPIO 10 and 8 | Proven on that board                                    | The bus is fixed by that board's wiring                                                | Out of scope; this map covers the SuperMini and the rig.                                     |

## Verification
- `just build-example` succeeds for every touched C3 example on both tiers.
- `just check-idf` and `just check-hal` pass with the new pins.
- A grep of the C3 examples and docs finds no SDA 4 / SCL 5, buzzer 6, rotary click 10 or button 4.
- On the rewired rig, `just bench c3 b3f`, `just bench c3 buzzer` and the I2C checks (`hal_c3_i2c_scan`, `just bench c3 bme680` or `mpu6050`) pass, and the OLED answers at 0x3C on GPIO 6/7.
- `hal_c3_rig_check` (`just bench c3 rig`) confirms the map on the rig, one connector at a time.
