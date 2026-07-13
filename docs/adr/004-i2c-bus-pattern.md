# ADR-004: I2C bus pattern and bring-up diagnostics

## Status
Accepted

## Context

The rustyfarian-peripherals workspace ships its first I2C support with a pair of
hardware bring-up examples: `hal_c3_i2c_scan` and `idf_c3_i2c_scan` on ESP32-C3.
These are bus scanners — diagnostic tools that probe every 7-bit I2C address
(`0x08..0x77`) and report ACKing devices — and **neither is a peripheral driver**.
They follow the esp-hal 1.x and esp-idf-hal 0.46 native I2C APIs directly with no
`tamer` involvement.

Two intertwined decisions needed resolution:

1. **Bus pattern across both tiers.** The two examples establish a shared I2C
   foundation: GPIO 4 (SDA) / GPIO 5 (SCL) at ~100 kHz, reusable by future
   hardware examples (MPU6050, planned OLED display). Both esp-hal and esp-idf-hal
   can distinguish NACK (device absent) from genuine bus faults, though via
   different mechanisms: esp-hal's `I2c::write(addr, &[])` yields
   `Error::AcknowledgeCheckFailed` directly; esp-idf-hal's `I2cDriver::write`
   returns a generic `EspError`, requiring inspection of `err.code() == ESP_FAIL`
   to detect NACK (matching esp-idf-hal's own `to_i2c_err` classifier).

2. **The bring-up-diagnostic exemption.** The scanning tool has no pure `tamer`
   core. This repository's flagship rule — "every peripheral needs a host-testable
   pure core" — targets *peripherals/drivers*, not *diagnostic tools*. This
   decision explicitly permits hardware-only bring-up utilities as out-of-band
   examples, distinct from a peripheral's design.

## Decision

### I2C Bus Pattern

- **SDA = GPIO 4, SCL = GPIO 5** (`100 kHz` default). These are non-strapping,
  non-USB/UART pins on common ESP32-C3 development boards, suitable for both
  `rustyfarian-esp-hal-peripherals` and `rustyfarian-esp-idf-peripherals`
  examples. Reuse them for all upcoming I2C peripherals (MPU6050, display
  drivers, etc.) so consumers wire once and mount the bus everywhere.

- **Probe methodology:** A zero-length write (START + 7-bit address + write bit +
  STOP, no data byte) is the minimal I2C address probe. Both tiers discriminate
  device absence from genuine bus faults: esp-hal surfaces NACK as
  `Error::AcknowledgeCheckFailed` directly; esp-idf-hal's `I2cDriver::write`
  returns an `EspError` that can be inspected via `err.code() == ESP_FAIL` for
  NACK (matching esp-idf-hal's own `to_i2c_err` classification of `ESP_FAIL` →
  `NoAcknowledge`). Any other error code (e.g. `ESP_ERR_TIMEOUT`) signals a
  genuine bus fault (missing pull-ups, shorted line, or wrong pins) and must be
  logged as a diagnostic alert, not silently swallowed.

### Bring-up-Diagnostic Exemption

- **Hardware-only examples are admissible** when they serve diagnostic or
  validation purposes (bus discovery, raw register peeking, calibration tools)
  and are explicitly labeled as such. They live under `examples/`, not in the
  `tamer` core or a hardware tier's public API.

- **A diagnostic is not a peripheral.** A peripheral — rotary encoder, button,
  buzzer, display — ships its pure, host-testable decode/render logic in `tamer`
  and uses a hardware example to demonstrate the wiring and the hal/idf adapter.
  A diagnostic is a standalone tool: it may exercise hardware directly without
  an underlying pure core (the scanner probes addresses; calibration tools may
  read raw ADC samples). The difference is architectural, not stylistic.

## Consequences

**Positive:**

- The I2C bus pattern is now frozen at GPIO 4/5 / 100 kHz. Future peripheral
  hardware examples follow the same wiring, so a consumer mounting MPU6050,
  OLED, or other I2C devices can reuse the same bus connector.
- Bring-up diagnostics have an explicit, bounded niche: validation tools that
  live in `examples/`, not in the public API. This clarifies the rule ("pure
  core per peripheral") — it applies to drivers, not test harnesses.
- The scanner pair (`hal_c3_i2c_scan` / `idf_c3_i2c_scan`) proved both esp-hal
  and esp-idf paths for I2C, de-risking the real MPU6050 hardware example.

**Negative / Limitations:**

- GPIO 4/5 are now a canonical (near-immutable) choice; if a future board needs
  a different pin pair, that becomes a documented variant example, not the
  default. This is intentional: standardization reduces decision fatigue.

## Alternatives Considered

|                                                          Alternative |                                                                                      Pros | Cons                                                                                                                                                                                                                           | Why Rejected |
|---------------------------------------------------------------------:|------------------------------------------------------------------------------------------:|:-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|:-------------|
|             Embed a minimal `tamer::i2c` trait façade in the scanner | Would exercise the eventual I2C bus trait; separates probe logic from esp-hal/idf details | Premature abstraction — one diagnostic does not prove a trait boundary; the trait would remain untested until a peripheral driver lands. The diagnostic's value is validation, not trait design.                               |
|            Distinguish NACK in the esp-idf tier via a wrapper module |                                                     Would encapsulate error code dispatch | Unnecessary — `err.code() == ESP_FAIL` check is more direct and mirrors esp-idf-hal's own classifier (`to_i2c_err`). Both tiers' APIs are transparent enough; adding a wrapper in `examples/` adds cognitive load for no gain. |
|       Allocate a separate I2C bus for diagnostics (e.g., GPIO 19/20) |                             Isolation; consumers wouldn't reuse this bus for real devices | Defeats the point of a diagnostic ("confirm this bus works before trusting your device to it"); wastes pins on commonly I2C-sparse boards; harder to share schematics.                                                         |
| Use a Rust-idiomatic I2C crate wrapper (e.g., `i2cdev-embedded-hal`) |                                                                 Portable API across tiers | Would couple the examples to an extra dependency and a pre-built adapter; both tiers have direct, uncomplicated I2C APIs at their level of abstraction. Wrapping adds cognitive load for a diagnostic.                         |

## Verification

- Both `hal_c3_i2c_scan` and `idf_c3_i2c_scan` compile and run on an ESP32-C3
  with GPIO 4 (SDA) / GPIO 5 (SCL) wired to an I2C device.
- The scanner correctly reports ACKing addresses: esp-hal surfaces NACK as
  `Error::AcknowledgeCheckFailed`; esp-idf-hal surfaces it as `EspError` with
  `code() == ESP_FAIL`, discriminated inline to avoid reporting it as an error.
- Non-NACK errors (e.g. `ESP_ERR_TIMEOUT` from a stuck bus, wrong pins, or
  missing pull-ups) are logged as bus warnings to alert the user to a genuine
  wiring fault, never silently swallowed.
- The bus pin choice (GPIO 4/5) is documented in the example header and the
  feature doc for MPU6050 as the canonical choice for future I2C examples.
- ADR-004 is referenced in `docs/ROADMAP.md` near the MPU6050 hardware example
  to signal the established pattern.
