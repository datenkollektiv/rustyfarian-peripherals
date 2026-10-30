# ADR-009: I2C sensor drivers live in `tamer` behind per-device features

## Status
Accepted

## Context

Downstream projects need I2C sensor drivers: BME680 first, then SHT3x.
ADR-004 fixed the I2C bus pins and the diagnostics exemption, but not where a sensor driver lives.
`tamer::mpu6050` set the decode-only precedent: `tamer` parses a burst buffer and the caller owns the transport.
A sensor driver adds a sequence on top of the decoding: reset, chip-ID check, calibration reads, trigger, bounded polling.
That sequence is generic over `embedded_hal::i2c::I2c` and `embedded_hal::delay::DelayNs`.
Both pinned stacks implement those traits for their own types: esp-hal 1.2.2 (`I2c`, `Delay`) and esp-idf-hal 0.47.0 (`I2cDriver`, `Delay`).
So the same generic code runs on either tier, and a mock bus can test it on the host.

## Decision

- Each device gets a device-named `tamer` module (`tamer::bme680`) behind a feature of the same name.
- The device feature alone enables the pure part: register constants, calibration parsing, compensation, and register-byte builders.
- The driver struct is compiled when both the device feature and `hal` are enabled, so the math stays usable without `embedded-hal`.
- The driver owns its `I2C` by value and returns it through `release()`; a failed constructor returns the bus alongside the error.
- A shared bus goes through `embedded-hal-bus` devices in the consumer, never through a `tamer` dependency.
- `tamer::mock` ships a handwritten `MockI2c` (scripted, with error injection) and a `NoopDelay`, both under `hal`.
- Driver errors keep the HAL error as `Error::Bus(E)`; portable handling such as NACK detection inspects `kind()`.
- The ESP tiers add no driver code, only hardware examples and, if wanted, feature passthrough.
- Devices that need nothing beyond a buffer decoder stay transport-free, as `mpu6050` does.
- A device joins this pattern only after confirming it speaks I2C; HX711, a clocked GPIO protocol, needs its own decision.

## Consequences

- One host-tested driver serves both tiers, which keeps the sans-io spine and tier parity.
- `hal` grows from `InputPin` adapters to `InputPin`, `I2c`, and `DelayNs`; crate docs must say so.
- Consumers choose a sensor in one place, `tamer`'s features.
- `tamer` takes on driver sequencing code, so every driver needs mock-bus tests for its error paths.

## Alternatives Considered
| Alternative                 | Pros                     | Cons                                        | Why Rejected                    |
|:----------------------------|:-------------------------|:--------------------------------------------|:--------------------------------|
| Driver in both tiers        | Tier owns all bus code   | Duplicated, host-testable logic in wrappers | Violates the sans-io boundary   |
| Transport left to caller    | Smallest `tamer` surface | Every consumer rewrites the sequence        | Untested logic moves downstream |
| One catch-all `hal` feature | Simplest gating          | Every `hal` user compiles every driver      | Code size; no opt-in per device |
| `bme680 = ["hal"]`          | One switch per device    | Math unusable without `embedded-hal`        | Couples pure core to the bus    |
| `embedded-hal-mock`         | Ready-made mock          | Extra dependency; not shippable             | Downstream cannot reuse it      |
