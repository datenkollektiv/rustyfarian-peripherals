# Bench — ESP32-C6-DevKitC-1

## Wiring

| Check  | Signal                   | GPIO         | Notes                                                |
|:-------|:-------------------------|:-------------|:-----------------------------------------------------|
| b3f    | B3F button to GND        | 4            | internal pull-up, active-low                         |
| rotary | EC11 A / B / button      | 18 / 19 / 20 | adjacent on J3; internal pull-ups, common to GND|
| oled   | OLED SDA / SCL           | 21 / 22      | board profile c6-devkitc (ADR-014); 0x3C, SSD1306 or SH1106 |

## Checks

| Check  | Examples                                | Verdict |
|:-------|:----------------------------------------|:--------|
| b3f    | `hal_c6_b3f`, `idf_c6_b3f`              | not run |
| rotary | `hal_c6_rotary_isr`, `idf_c6_rotary`    | not run |
| oled-ssd1306 | `hal_c6_oled`, `idf_c6_oled`            | not run |
| oled-sh1106  | `hal_c6_oled`, `idf_c6_oled`            | not run |

## Commands

No step waits for Enter: read the instruction and act; each step passes as soon as its line arrives (60 s).

Builds, flashes and guides the button check (3 presses) on hal, then offers idf:

```sh
just bench c6 b3f
```

Runs the rotary check (direction, round trip, spin with `skipped` staying 0, click, double click, long press) on hal, then offers idf:

```sh
just bench c6 rotary
```

Runs every C6 check in turn:

```sh
just bench c6
```

Same as above for one check, but skips the build when nothing changed:

```sh
just bench c6 rotary --no-build
```

Shows the chip and MAC on each connected port:

```sh
just bench-detect
```

Checks an SSD1306 OLED on SDA 21 / SCL 22 (wired before the run or connected when asked): detected controller, picture, detach after you pull its VCC or SDA wire, on hal, then offers idf:

```sh
just bench c6 oled-ssd1306
```

The same check with an SH1106 OLED:

```sh
just bench c6 oled-sh1106
```

## If it hangs

- No port at all: the board is in deep sleep or in download mode; replug it, holding BOOT only if it is truly stuck.
- Ctrl-C ends a run; it is recorded as `aborted`.
