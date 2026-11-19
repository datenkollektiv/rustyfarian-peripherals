# Bench — Adafruit ESP32 Feather V2

Same chip as `esp32`, built with the `feather-v2` board profile ([ADR-014](../../docs/adr/014-board-profiles.md)).
The pogo set and the button pin follow [ADR-015](../../docs/adr/015-pogo-ports.md).

| Part | GPIO | Notes |
|:--|:--|:--|
| I2C pogo: STEMMA QT socket (JST SH 1.0 mm) | SDA 22 / SCL 20 | socket power on GPIO 2; OLED 0x3C |
| B3F button to GND | 4 (A5) | button pin, direct or on a button pogo; not fitted yet |
| SW38 button (on-board) | 38 | input-only, board pull-up; extra button |
| 3V3 pogo (GND, 3V3, SIG) | 34 (A2) | ADC1, input-only, no internal pull-up; not fitted yet |
| 5V pogo (GND, 5V, SIG) | 25 (A1) | 3V3 logic; 5 V from the USB pin; not fitted yet |

- The B3F on A5 uses the internal pull-up and is the deep-sleep wake input.
- On-board: the NeoPixel on GPIO 0 (power on GPIO 2) and the red LED on GPIO 13; no check uses them.

Checks: `oled-ssd1306`, `oled-sh1106`.
