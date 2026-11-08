# Bench — ESP32-C3 SuperMini

## Wiring

| Check        | Signal                      | GPIO         | Notes                                                    |
|:-------------|:----------------------------|:-------------|:---------------------------------------------------------|
| b3f          | B3F button to GND           | 4            | internal pull-up, active-low                             |
| rotary       | EC11 A / B / button         | 0 / 1 / 10   | internal pull-ups; 2/8/9 are strapping pins              |
| bme680       | BME680 SDA / SCL            | 4 / 5        | I2C pogo header, address 0x76 (SDO to GND), 3V3 only     |
| mpu6050      | MPU6050 SDA / SCL           | 4 / 5        | same header, address 0x68 (AD0 to GND)                   |
| buzzer       | passive piezo + to GND      | 6            | direct drive for a small piezo, NPN for a loud one       |
| hall-linear  | 49E (HW-477) OUT            | 4            | ADC1; VCC to 3V3 (OUT rests ~1.65 V), never 5 V |
| poti         | potentiometer wiper         | 4            | ADC1; outer legs to 3V3 and GND                          |

GPIO 4 is shared: wire only one of b3f, hall-linear, poti or the I2C header at a time.
See `docs/hardware-setup.md` for the I2C pogo header pin order (GND, 3V3, SDA, SCL) and the buzzer wiring.

## Checks

| Check        | Examples                                                           | Verdict |
|:-------------|:-------------------------------------------------------------------|:--------|
| b3f          | `hal_c3_b3f`, `idf_c3_b3f`                                         | not run |
| rotary       | `hal_c3_rotary_isr`, `idf_c3_rotary`                               | not run |
| bme680       | `hal_c3_bme680`, `idf_c3_bme680`                                   | not run |
| bme680-async | `hal_c3_bme680_async`                                              | not run |
| mpu6050      | `hal_c3_mpu6050_tilt`, `idf_c3_mpu6050_tilt`                       | not run |
| buzzer       | `hal_c3_passive_buzzer_arpeggio`, `idf_c3_passive_buzzer_arpeggio` | not run |
| hall-linear  | `hal_c3_hall_linear`, `idf_c3_hall_linear`                         | not run |
| poti         | `hal_c3_poti`, `idf_c3_poti`                                       | not run |

## Commands

Each step waits for Enter before it starts timing; read the instruction, press Enter, then act.

Until the pin moves past rustyfarian-bench v0.3.0, the scenarios need the local checkout (hall-linear pole capture, poti reset on Enter); run this once per shell:

```sh
export RUSTYFARIAN_BENCH=../rustyfarian-bench
```

Builds, flashes and guides the button check (3 presses, a hold) on hal, then offers idf:

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

## Not covered here

- MPU6050 wrong-address hint: set `I2C_ADDR` to 0x69 with AD0 low (or the reverse), rebuild and check the AD0 hint prints instead of a panic.

Shows the chip and MAC on each connected port:

```sh
just bench-detect
```

## If it hangs

- No port at all: the board is in deep sleep or in download mode; replug it, holding BOOT only if it is truly stuck.
- Ctrl-C ends a run; it is recorded as `aborted`.
