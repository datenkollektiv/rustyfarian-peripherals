# Bench — ESP32-C6-DevKitC-1

| Part | GPIO | Notes |
|:--|:--|:--|
| B3F button to GND | 4 | internal pull-up |
| EC11 A / B / button | 18 / 19 / 20 | adjacent on J3; common to GND |
| OLED SDA / SCL | 21 / 22 | board profile `c6-devkitc`; 0x3C |

Checks: `b3f`, `rotary`, `oled-ssd1306`, `oled-sh1106`.
