# Bench — ESP32 WROOM-32

Silkscreen `Dn` is GPIO n.

| Part | GPIO | Notes |
|:--|:--|:--|
| B3F button to GND | 4 | internal pull-up |
| EC11 A / B / button | 32 / 33 / 27 | common to GND |
| OLED SDA / SCL | 21 / 22 | board profile `esp32-wroom`; 0x3C, 3V3 |

Checks: `b3f`, `rotary`, `oled-ssd1306`, `oled-sh1106`.
