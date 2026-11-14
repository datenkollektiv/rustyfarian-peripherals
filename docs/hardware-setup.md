# Hardware Setup Guide

Board wiring lives with the bench, one page per board:

| Board | Wiring |
|:--|:--|
| CrowPanel 1.28" HMI (ESP32-S3) | [`bench/s3/README.md`](../bench/s3/README.md) |
| ESP32-C3 bench rig and SuperMini | [`bench/c3/README.md`](../bench/c3/README.md) |
| ESP32-C6-DevKitC-1 | [`bench/c6/README.md`](../bench/c6/README.md) |
| ESP32 WROOM-32 | [`bench/esp32/README.md`](../bench/esp32/README.md) |
| Adafruit ESP32 Feather V2 | [`bench/esp32-feather/README.md`](../bench/esp32-feather/README.md) |

The ESP32-C3 pin map is in [ADR-012](adr/012-esp32-c3-pin-map.md).
This page keeps only what the bench pages do not cover.
Each example's doc comment lists its own pins.

## Wiring — shared I2C pogo header (ESP32-C3 SuperMini)

The 4-pin header uses the Qwiic / STEMMA QT order GND, 3V3, SDA (GPIO 6), SCL (GPIO 7) at 100 kHz and 3.3 V logic.

| Device | Address | Alternate (strap) |
|:--|:--|:--|
| BME680 | 0x76 (SDO low) | 0x77 |
| SSD1306 / SH1106 OLED | 0x3C | 0x3D |
| MPU6050 (GY-521) | 0x68 (AD0 low or open) | 0x69 |

- Breakout pull-ups add in parallel; two or three boards are fine at 100 kHz.
- The C3 rig adds 4.7 kΩ pull-ups, because the OLED module has none.
- The GY-521 header order is VCC, GND, SCL, SDA, so a pin-for-pin lead to the pogo crosses SDA and SCL.
- Calibrate a GY-521 chip up (header pins down); pins up calibrates −1 g and caps tilt at about ±26°.
- Isolated `AcknowledgeCheckFailed` reads while the power LED dims mean a loose VCC or GND jumper.

## Wiring — passive piezo (C6)

`hal_c6_passive_buzzer_arpeggio` drives the piezo from GPIO 6; it is compile-verified only and may sound an octave high (see its Caveats).
The C3 piezo is on the bench rig, GPIO 10 through an NPN.

## Wiring — active-buzzer Morse keying (ESP32-C3 SuperMini)

`hal_c3_active_buzzer_morse` / `idf_c3_active_buzzer_morse` key a TMB12A03 active buzzer on GPIO 10 (HIGH = on), other terminal to GND.

- The TMB12A03 draws up to about 30 mA at 3 V; the examples set GPIO 10 to 40 mA drive.
- Drive a larger buzzer through an NPN with a flyback diode.
- Keep the `Relaxed` preset (100 ms dot) as the default; a 60 ms `Standard` dot is near the buzzer's acoustic response time.

## Wiring — Arduino Nano button (examples/avr-nano-tamer)

A button from D2 to GND (internal pull-up, active-low); the on-board LED on D13 toggles on each click; serial at 57600 baud through the ravedude runner.
