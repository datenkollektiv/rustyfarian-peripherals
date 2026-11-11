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
| b3f    | `hal_esp_b3f`, `idf_esp_b3f`            | pass (2026-11-12, 6214033) |
| rotary | `hal_esp_rotary_isr`, `idf_esp_rotary`  | pass (2026-11-12, 6214033) |
| oled-ssd1306 | `hal_esp_oled`, `idf_esp_oled`          | pass (2026-11-12, 6214033) |
| oled-sh1106  | `hal_esp_oled`, `idf_esp_oled`          | not run |

## Commands

No step waits for Enter: read the instruction and act; each step passes as soon as its line arrives (60 s).

Builds, flashes and guides the button check (3 presses) on hal, then offers idf:

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

Checks an SSD1306 OLED on SDA 21 / SCL 22 (wired before the run or connected when asked): detected controller, picture, detach after you pull its VCC or SDA wire, on hal, then offers idf:

```sh
just bench esp32 oled-ssd1306
```

The same check with an SH1106 OLED:

```sh
just bench esp32 oled-sh1106
```

## If it hangs

- No port at all: the board is in deep sleep or in download mode; replug it, holding BOOT only if it is truly stuck.
- Ctrl-C ends a run; it is recorded as `aborted`.
