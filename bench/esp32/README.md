# Bench — ESP32 WROOM-32

Silkscreen `Dn` is GPIO n.
The pogo set and the button pin follow [ADR-015](../../docs/adr/015-pogo-ports.md).

| Part | GPIO | Notes |
|:--|:--|:--|
| B3F button to GND | 4 | button pin, wired directly; internal pull-up; deep-sleep wake |
| EC11 A / B / button | 32 / 33 / 27 | common to GND |
| I2C pogo (GND, 3V3, SDA, SCL) | 21 / 22 | board profile `esp32-wroom`; not fitted yet |
| 3V3 pogo (GND, 3V3, SIG) | 34 | ADC1, input-only, no internal pull-up; not fitted yet |
| 5V pogo (GND, 5V, SIG) | 25 | 3V3 logic; 5 V from the VIN pin; not fitted yet |

- The OLED (0x3C, 3V3) is wired directly to GPIO 21 / 22 until the I2C pogo and its 4.7 kΩ pull-ups are fitted.
- GPIO 4 is also touch channel T0.
- No external pull-up on GPIO 4: the ESP32 internal pull-up held through deep sleep on GPIO 25 (rustyfarian-power `esp32-feather/anylow`).
- The rerun on GPIO 4, the button pin, is still open there.

Checks: `b3f`, `rotary`, `oled-ssd1306`, `oled-sh1106`.
