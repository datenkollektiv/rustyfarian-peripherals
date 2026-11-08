# Bench — ESP32 WROOM-32 / Feather V2

## Wiring

| Check  | Signal                   | GPIO         | Notes                                                |
|:-------|:-------------------------|:-------------|:-----------------------------------------------------|
| b3f    | B3F button to GND        | 4            | internal pull-up, active-low                         |
| rotary | EC11 A / B / button      | 32 / 33 / 27 | GPIO 34-39 have no pull-ups; check the Feather V2 pinout card |
| oled   | OLED SDA / SCL           | 21 / 22      | board profile esp32-wroom (ADR-014); on a Feather V2 use `bench/feather-v2` |

Common of the encoder goes to GND on both rows.

## Checks

| Check  | Examples                                | Verdict |
|:-------|:----------------------------------------|:--------|
| b3f    | `hal_esp_b3f`, `idf_esp_b3f`            | not run |
| rotary | `hal_esp_rotary_isr`, `idf_esp_rotary`  | not run |
| oled   | `hal_esp_oled`, `idf_esp_oled`          | not run |

## Commands

Each step waits for Enter before it starts timing; read the instruction, press Enter, then act.

Builds, flashes and guides the button check (3 presses, a hold) on hal, then offers idf:

```sh
just bench esp32 b3f
```

Runs the rotary check (direction, round trip, spin with `skipped` staying 0, click, double click, long press) on hal, then offers idf:

```sh
just bench esp32 rotary
```

Runs every ESP32 check in turn:

```sh
just bench esp32
```

Same as above for one check, but skips the build when nothing changed:

```sh
just bench esp32 rotary --no-build
```

Shows the chip and MAC on each connected port:

```sh
just bench-detect
```

Waits for an OLED on SDA 21 / SCL 22, checks the picture, a live swap to the other controller and the detach, on hal, then offers idf:

```sh
just bench esp32 oled
```

## If it hangs

- No port at all: the board is in deep sleep or in download mode; replug it, holding BOOT only if it is truly stuck.
- Ctrl-C ends a run; it is recorded as `aborted`.
