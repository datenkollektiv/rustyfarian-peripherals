# Bench — Adafruit Feather V2 (ESP32)

Same chip as the `esp32` board, built with the `feather-v2` board profile ([ADR-014](../../docs/adr/014-board-profiles.md)).

## Wiring

| Check | Signal                 | GPIO                | Notes                                            |
|:------|:-----------------------|:--------------------|:-------------------------------------------------|
| oled  | OLED SDA / SCL         | 22 / 20 (STEMMA QT) | socket power switched on via GPIO 2; 0x3C        |

## Checks

| Check | Examples                       | Verdict |
|:------|:-------------------------------|:--------|
| oled-ssd1306 | `hal_esp_oled`, `idf_esp_oled` | deferred (needs a STEMMA QT cable) |
| oled-sh1106  | `hal_esp_oled`, `idf_esp_oled` | deferred (needs a STEMMA QT cable) |

## Commands

Checks an SSD1306 OLED on the STEMMA QT socket (wired before the run or connected when asked): detected controller, picture, detach after you pull its VCC or SDA wire, on hal, then offers idf:

```sh
just bench feather-v2 oled-ssd1306
```

The same check with an SH1106 OLED:

```sh
just bench feather-v2 oled-sh1106
```
