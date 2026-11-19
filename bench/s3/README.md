# Bench — CrowPanel 1.28" HMI (ESP32-S3)

No wiring: every part is on the board.
The CrowPanel carries no pogo set ([ADR-015](../../docs/adr/015-pogo-ports.md)): only GPIO 4 and 12 are free, on its FPC.

| Part | GPIO | Notes |
|:--|:--|:--|
| Knob A / B | 45 / 42 | |
| Knob button | 41 | not an RTC pin, so no deep-sleep wake |
| Touch panel INT (on-board) | 5 | deep-sleep wake source; touch power on GPIO 1 held through sleep |

- Also on the board, unused by the checks: the GC9A01 display (SPI), the CST816 touch controller (I2C 6 / 7) and a 5-LED WS2812 ring on GPIO 48.

Checks: `b3f`, `rotary`, `rotary-isr`.
