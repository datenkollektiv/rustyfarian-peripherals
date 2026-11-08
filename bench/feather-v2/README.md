# Bench — Adafruit Feather V2 (ESP32)

Same chip as the `esp32` board, built with the `feather-v2` board profile ([ADR-014](../../docs/adr/014-board-profiles.md)).

## Wiring

| Check | Signal                 | GPIO                | Notes                                            |
|:------|:-----------------------|:--------------------|:-------------------------------------------------|
| oled  | OLED SDA / SCL         | 22 / 20 (STEMMA QT) | socket power switched on via GPIO 2; 0x3C        |

## Checks

| Check | Examples                       | Verdict |
|:------|:-------------------------------|:--------|
| oled  | `hal_esp_oled`, `idf_esp_oled` | not run |

## Commands

Until the pin moves past rustyfarian-bench v0.3.0, the scenarios need the local checkout; run this once per shell:

```sh
export RUSTYFARIAN_BENCH=../rustyfarian-bench
```

Waits for an OLED on the STEMMA QT socket, checks the picture, a live swap to the other controller and the detach, on hal, then offers idf:

```sh
just bench feather-v2 oled
```
