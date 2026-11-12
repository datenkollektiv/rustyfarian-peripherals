# Bench — ESP32-C3 (bench rig and SuperMini)

Every C3 example uses the pin map of [ADR-012](../../docs/adr/012-esp32-c3-pin-map.md).

| Part | GPIO | Notes |
|:--|:--|:--|
| B3F button to GND | 3 | internal pull-up |
| 3-pin pogo (GND, 3V3, SIG) | 4 | ADC1: 49E Hall or potentiometer wiper |
| 4-pin I2C pogo (GND, 3V3, SDA, SCL) | 6 / 7 | 4.7 kΩ pull-ups; OLED 0x3C, MPU6050 0x68, BME680 0x76 |
| Passive piezo via NPN | 10 | LEDC tone |
| EC11 A / B / button | 0 / 1 / 5 | not on the rig |
| LED, active-low | 8 | not fitted yet |

- The button, the 3-pin pogo and the I2C pogo are wired at once; the rig reaches one at a time.
- 3V3 only on every part; never 5 V on a signal.

Checks: `b3f`, `buzzer`, `hall-linear`, `poti`, `oled`, `mpu6050`, `rotary`, `rig`; `bme680` and `bme680-async` are experimental.
