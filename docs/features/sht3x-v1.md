---
gate: none
desk-work: available
---

# Feature: SHT3x temperature / humidity sensor v1

Temperature and relative humidity from a Sensirion SHT30 / SHT31 / SHT35 over I2C ([ADR-009](../adr/009-i2c-sensor-drivers-in-tamer.md)), single-shot mode only.
The consumer is a beehive monitor that needs brood temperature within 0.5 °C, which the BME680 (±1 °C, self-heating) cannot give.
Periodic / ART modes, alert thresholds and the SHT4x are out of scope.

## Decisions
| Decision | Reason | Rejected Alternative |
|:--|:--|:--|
| `tamer::sht3x`, feature `sht3x = []`, no dependency; driver needs `sht3x` + `hal` | ADR-009 placement, as `bme680` | `sht3x = ["hal"]` |
| Fixed-point output: `i32` centi-°C and `u16` centi-%RH, round to nearest | 0.01 °C resolution beats the ±0.2 °C sensor; no float in the core | `f32` as `bme680` |
| Rounding uses the `AnalogRange::normalize` idiom (`+ divisor/2`, widened) | One rounding idiom across `tamer` | Truncation as in the sketch |
| `Measurement` and `Status` keep private fields behind accessors | Matches `bme680::Reading`; public fields freeze the API | Public fields as in the sketch |
| `Repeatability::{High, Medium, Low}` carries command word and max wait 15.5 / 6.5 / 4.5 ms (Table 5, 2.15–2.4 V) | Datasheet facts as data; covers both supply ranges (Table 4 is 15 / 6 / 4 ms); High is the consumer default | Separate const tables; typical durations |
| Commands without clock stretching only | A not-ready sensor NACKs instead of holding SCL; one wait, one read | Clock-stretching variants |
| `Sht3x::new(i2c, addr)` is `const` and touches no bus; `init(&mut delay)` brings up | Mirrors `Mpu6050`; the consumer powers the sensor from a switched rail | `new` does the reset as `Bme680` |
| `init`: wait 2 ms, soft reset, wait 2 ms, status write, wait 1 ms, read (CRC-checked); reset-detected bit not asserted | Covers t_PU and t_SR (1.5 ms each); the status read is the presence check, and a stale bit is not a fault | Skip the status read; fail on bit 4 clear |
| Sans-IO `SingleShot` action machine drives `measure` and `measure_async` | ADR-010 trigger: two execution models share one step order | Two hand-written sequences |
| `Error::{Bus(E), Crc(CrcError), Sequence}` with `is_nack()` | Same shape as `mpu6050::Error`; CRC names the failing word | Fold CRC into `Bus` |
| No heater or clear-status methods in v1; the command constants ship | No consumer; constants let a caller do it without a release | `heater(bool)` now |
| Root re-exports `Sht3x` and `Repeatability` only | As `bme680`: `Measurement`, `Status`, errors stay under `tamer::sht3x::` | Flatten everything |
| Examples `hal_c3_sht3x` / `idf_c3_sht3x` on GPIO 6 / 7 at 0x44 plus `bench/c3/sht3x.toml` | ADR-012 bus; line-for-line twins like the BME680 pair | No examples |

## API surface
- Constants: `I2C_ADDR` 0x44, `I2C_ADDR_ALT` 0x45, `CMD_SOFT_RESET` 0x30A2, `CMD_HEATER_ON` 0x306D, `CMD_HEATER_OFF` 0x3066, `CMD_READ_STATUS` 0xF32D, `CMD_CLEAR_STATUS` 0x3041.
- `Repeatability::command() -> [u8; 2]` (0x2400 / 0x240B / 0x2416) and `max_duration_us()`.
- `crc8([u8; 2]) -> u8` (poly 0x31, init 0xFF, no reflection, no final XOR; CRC(0xBEEF) = 0x92).
- `raw_to_centi_celsius(u16) -> i32` (−45 + 175·S/65535) and `raw_to_centi_percent_rh(u16) -> u16` (100·S/65535).
- `parse_measurement(&[u8; 6]) -> Result<Measurement, CrcError>`, `parse_status(&[u8; 3]) -> Result<Status, CrcError>`; `CrcError::{Temperature, Humidity, Status}`.
- `Measurement::{temperature_centi_c, humidity_centi_rh, raw_temperature, raw_humidity}`; `Status::{bits, reset_detected, heater_on, alert_pending, command_failed, checksum_failed}`.
- `SingleShot::new(Repeatability)` + `poll(ShotInput) -> ShotAction::{WriteCommand([u8; 2]), Wait { us }, ReadFrame, Done(Measurement), Fail(CrcError), OutOfPhase}` + `max_polls()`, with `ShotInput::{Start, Written, Waited, Frame([u8; 6])}`.
- `Sht3x<I2C>`: `new`, `init<D>(&mut D) -> Result<Status, Error<E>>`, `soft_reset<D>(&mut D)`, `status<D>(&mut D)`, `measure<D>(&mut D, Repeatability) -> Result<Measurement, Error<E>>`, `release(self) -> I2C`; `measure_async` under `hal-async`.

## Constraints
- `no_std`, alloc-free, panic-free, no `unsafe`, MSRV 1.88; the `sht3x` feature adds no dependency.
- Every bus fault, NACK and CRC failure is an `Err`, never a panic; the consumer reports the field as absent.
- `measure` sends the command, waits exactly `max_duration_us`, then reads 6 bytes once; a NACK on that read is reported, not retried.
- Datasheet: 1 ms minimum between commands; `init` and `measure` never issue two commands closer than that.

## Required tests (host)
- Pure: CRC datasheet example, range endpoints, 0x6666 → 25.00 °C, 0x8000 → 50.00 %RH, round-to-nearest at a half step, brood-range resolution, CRC failure per word, status bit decode and default 0x8010.
- Machine: happy path action order, out-of-phase input, poll bound.
- Driver (`MockI2c`): `init` script (reset write, status write then read), NACK on reset keeps `E` and `is_nack()`, CRC failure on status and on each measurement word, 0x45 operation, delay totals per repeatability (test-local counting delay), `release` returns the bus.
- Parity: `measure` and `measure_async` consume the same script with equal delay totals (`block_on` from `tamer::mock`).

## Open Questions
- [ ] Hardware (C3 rig, SHT31 on the I2C pogo at 0x44): banner with status 0x8010 after init, plausible readings, RH rises on breath, NACK hint at the wrong address.

## State
- [x] Design approved
- [x] Core implementation (pure `sht3x`, `SingleShot`, driver, async runner)
- [x] Tests passing
- [x] Examples and bench scenario written, `just build-example` links both
- [x] Documentation updated (rustdoc, CHANGELOG, AGENTS, READMEs, hardware-setup, bench README, release-plan gate row)

## Session Log
- 2026-11-18 — Feature doc created via /feature dialog from the review-queue request
- 2026-11-18 — Core implemented (pure `sht3x`, `SingleShot`, blocking driver, `measure_async`) with host tests; API surface matched to it (`status` and `soft_reset` take a delay, status read is write then read)
- 2026-11-18 — Examples (`hal_c3_sht3x`, `idf_c3_sht3x`), bench scenario `c3/sht3x` and documentation added; both examples link; hardware verification still open
