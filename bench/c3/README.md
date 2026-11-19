# Bench — ESP32-C3 (bench rig and SuperMini)

Every C3 example uses the pin map of [ADR-012](../../docs/adr/012-esp32-c3-pin-map.md).
The pogo set and the button pin follow [ADR-015](../../docs/adr/015-pogo-ports.md).

| Part | GPIO | Notes |
|:--|:--|:--|
| B3F button to GND | 3 | button pin, wired directly; internal pull-up; deep-sleep wake |
| 3V3 pogo (GND, 3V3, SIG) | 4 | ADC1: 49E Hall or potentiometer wiper |
| 5V pogo (GND, 5V, SIG) | 1 | 3V3 logic; not fitted yet |
| I2C pogo (GND, 3V3, SDA, SCL) | 6 / 7 | 4.7 kΩ pull-ups; OLED, MPU6050, BME680, SHT3x |
| HX711 on the I2C pogo (GND, 3V3, DT, SCK) | 6 / 7 | DT on GPIO 6, SCK on GPIO 7, not I2C; 3V3 supply; experimental |
| Passive piezo via NPN | 10 | LEDC tone |
| LED, active-low | 8 | not fitted yet |

- The button, the 3V3 pogo and the I2C pogo are wired at once; the rig reaches one at a time.
- I2C addresses: OLED 0x3C, MPU6050 0x68, BME680 0x76, SHT3x 0x44.
- 5 V only on the 5V pogo's power pin; never 5 V on a signal.
- A deep-sleep wake check needs 10 kΩ from GPIO 3 to 3V3, not fitted yet: the C3 internal pull-up through sleep is unproven (rustyfarian-ci `conventions/pogo.md`).
- The EC11 sits on the second C3 rig, [`c3-rotary`](../c3-rotary/README.md), because its B input would take GPIO 1, the 5V pogo.
- With both C3 rigs attached, pass `--port` to `just bench` (see [bench/README.md](../README.md)).

Checks: `b3f`, `buzzer`, `hall-linear`, `poti`, `oled`, `mpu6050`, `rig`, `sht3x`; `bme680`, `bme680-async` abd `hx711` are experimental.
