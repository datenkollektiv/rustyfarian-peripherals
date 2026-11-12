# Bench — ESP32-C3 SuperMini

## Wiring

The C3 bench rig follows the ESP32-C3 pin map (ADR-012); every C3 example uses the same pins.

| Check        | Signal                      | GPIO         | Notes                                                        |
|:-------------|:----------------------------|:-------------|:-------------------------------------------------------------|
| b3f          | B3F button to GND           | 3            | internal pull-up, active-low                                 |
| rotary       | EC11 A / B / button         | 0 / 1 / 5    | internal pull-ups; not on the rig                            |
| bme680       | BME680 SDA / SCL            | 6 / 7        | 4-pin I2C pogo, address 0x76 (SDO to GND), 3V3 only          |
| mpu6050      | MPU6050 SDA / SCL           | 6 / 7        | same pogo, address 0x68 (AD0 to GND)                         |
| oled         | SSD1306 128x64 SDA / SCL    | 6 / 7        | same pogo, address 0x3C; SSD1306 or SH1106, attach or swap any time   |
| buzzer       | passive piezo via NPN       | 10           | LEDC tone                                                    |
| hall-linear  | 49E (HW-477) OUT            | 4            | 3-pin pogo, ADC1; VCC to 3V3 (OUT rests ~1.65 V), never 5 V  |
| poti         | potentiometer wiper         | 4            | 3-pin pogo, ADC1; outer legs to 3V3 and GND                  |
| rig          | the whole rig               | 3, 4, 6/7, 10 | attach/detach each part, press the button, confirm the buzzer |

The button (3), the 3-pin pogo (4) and the 4-pin I2C pogo (6/7) are wired at the same time on their own pins; the rig lets you reach only one of them at a time.
The rig's LED will sit on GPIO 8, active-low; it is not fitted yet, so the rig check has no LED step until it is.
See `docs/hardware-setup.md` § C3 bench rig for the connector pin orders and the I2C pull-ups.

## Checks

| Check        | Examples                                                           | Verdict |
|:-------------|:-------------------------------------------------------------------|:--------|
| b3f          | `hal_c3_b3f`, `idf_c3_b3f`                                         | pass (2026-11-12, 38dfe55) |
| rotary       | `hal_c3_rotary_isr`, `idf_c3_rotary`                               | not run |
| bme680       | `hal_c3_bme680`, `idf_c3_bme680`                                   | not run |
| bme680-async | `hal_c3_bme680_async`                                              | not run |
| mpu6050      | `hal_c3_mpu6050_tilt`, `idf_c3_mpu6050_tilt`                       | not run |
| buzzer       | `hal_c3_passive_buzzer_arpeggio`, `idf_c3_passive_buzzer_arpeggio` | pass (2026-11-12, 38dfe55) |
| hall-linear  | `hal_c3_hall_linear`, `idf_c3_hall_linear`                         | pass (2026-11-08, 9430069) |
| poti         | `hal_c3_poti`, `idf_c3_poti`                                       | pass (2026-11-08, 9430069) |
| oled         | `hal_c3_oled`, `idf_c3_oled`                                       | pass (2026-11-08, 2d04fbd) |
| rig          | `hal_c3_rig_check` (hal only, diagnostic)                          | not run |

## Commands

`b3f`, `rotary`, `oled` and `buzzer` steps start without Enter and pass as soon as their line arrives (60 s); the other checks still wait for Enter before a step starts timing.

Builds, flashes and guides the button check (3 presses) on hal, then offers idf:

```sh
just bench c3 b3f
```

Runs the rotary check (direction, round trip, spin with `skipped` staying 0, click, double click, long press) on hal, then offers idf:

```sh
just bench c3 rotary
```

Checks the blocking BME680 readings, a valid gas value and a breath test on hal, then offers idf:

```sh
just bench c3 bme680
```

Waits for the OLED, then checks it initialises, keeps refreshing and shows the test picture, and notices the detach, on hal, then offers idf:

```sh
just bench c3 oled
```

Checks the embassy-async BME680 readings against the blocking twin (hal only):

```sh
just bench c3 bme680-async
```

Checks the MPU6050 calibration, level reading and a tilt of about 90 degrees each way on hal, then offers idf:

```sh
just bench c3 mpu6050
```

Checks the arpeggio loop in the log and by ear, including the idf 10-bit LEDC melody and pitch:

```sh
just bench c3 buzzer
```

Checks the 49E midpoint has headroom both ways, then asks for one pole and then the other, each held for 2 s, on hal, then offers idf:

```sh
just bench c3 hall-linear
```

Checks the calibration sweep reaches both rails (raw ≤ 199 and ≥ 3900), then the low, high and middle readings, each held for 2 s, on hal, then offers idf:

```sh
just bench c3 poti
```

Checks every connector of the C3 bench rig against the pin map: attach and detach the I2C device and the pogo part, press the button, then press again when the tone sounds:

```sh
just bench c3 rig
```

Runs every C3 check in turn (the wiring changes between checks, so prefer one at a time):

```sh
just bench c3
```

Same as above for one check, but skips the build when nothing changed:

```sh
just bench c3 rotary --no-build
```

Runs only the steps the log can judge, so you can walk away (a gate needs this and the attended run):

```sh
just bench c3 hall-linear --unattended
```

Runs only the steps that need a person, skipping the build:

```sh
just bench c3 hall-linear --attended --no-build
```

## Spike: OLED identify

Not a gate: flashes `hal_c3_oled_identify`, which prints how the plugged OLED answers status and RAM read-back probes, so SSD1306 and SH1106 can be told apart.
Plug the SSD1306, wait for `IDENTIFY done`, unplug it, plug the SH1106, wait again, then the SSD1306 once more; Ctrl-C ends it:

```sh
just run hal_c3_oled_identify
```

## Not covered here

- MPU6050 wrong-address hint: set `I2C_ADDR` to 0x69 with AD0 low (or the reverse), rebuild and check the AD0 hint prints instead of a panic.

Shows the chip and MAC on each connected port:

```sh
just bench-detect
```

## If it hangs

- No port at all: the board is in deep sleep or in download mode; replug it, holding BOOT only if it is truly stuck.
- Ctrl-C ends a run; it is recorded as `aborted`.
