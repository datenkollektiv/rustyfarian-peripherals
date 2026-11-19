# Bench — ESP32-C6-DevKitC-1

The pogo set and the button pin follow [ADR-015](../../docs/adr/015-pogo-ports.md).

| Part | GPIO | Notes |
|:--|:--|:--|
| B3F button to GND | 4 | button pin, wired directly; internal pull-up; deep-sleep wake |
| EC11 A / B / button | 18 / 19 / 20 | adjacent on J3; common to GND |
| I2C pogo (GND, 3V3, SDA, SCL) | 21 / 22 (J3-7 / J3-6) | board profile `c6-devkitc`; not fitted yet |
| HX711 on the I2C pogo pins (GND, 3V3, DT, SCK) | 21 / 22 | DT on GPIO 21, SCK on GPIO 22, not I2C; 3V3 supply; wired directly; experimental |
| 3V3 pogo (GND, 3V3, SIG) | 2 (J1-12) | ADC1; 3V3 from J1-1, the only 3V3 pin; not fitted yet |
| 5V pogo (GND, 5V, SIG) | 3 (J1-13) | 3V3 logic; J1 15 / 14 / 13 = GND / 5V / GPIO 3; not fitted yet |

- The OLED (0x3C) and the SHT3x (0x44, ADDR to GND) are wired directly to GPIO 21 / 22 until the I2C pogo and its 4.7 kΩ pull-ups are fitted; a module without its own pull-ups needs them now.
- The HX711 module takes the same four wires (GND, 3V3, DT, SCK) in turn; it needs no pull-ups, and the pogo's, once fitted, do no harm.
- A deep-sleep wake check needs 10 kΩ from GPIO 4 to 3V3, not fitted yet: the C6 internal pull-up through sleep is unproven (rustyfarian-ci `conventions/pogo.md`).
- The on-board WS2812 on GPIO 8 is not used by any check.

Checks: `b3f`, `rotary`, `oled-ssd1306`, `oled-sh1106`, `sht3x`; `hx711` is experimental.
