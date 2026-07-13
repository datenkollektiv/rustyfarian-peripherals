# Feature: MPU6050 Accelerometer / IMU primitive v1

A pure, sans-IO support module for the InvenSense **MPU6050** 6-axis IMU: register
map, raw-buffer parsing, accelerometer offset calibration, and (feature-gated)
tilt-angle trigonometry. The consumer performs the I2C burst read; `tamer` owns
all the host-testable decoding and math. This lands `tamer`'s first **device-support
module** and its first (opt-in) floating-point surface.

## Background

Rationale for *shape*, *placement*, and *naming* — kept out of the design contract
below.

- **Why `tamer` (the pure core).** The module is register constants + byte parsing
  + tilt math and offset calibration — it imports no I2C, no HAL, no chip crate. It
  is exactly the "decode/render logic with no business touching hardware" that
  `VISION.md` mandates living in the pure core, alongside `analog`, `hall`, and
  `smoothing`. The hardware tiers do the burst read and feed the 14-byte buffer in.

- **Why a device-named module, not a generic façade.** Every existing `tamer`
  module is named for a *mechanism* (`SlidingAverage`, `HallSensor`, `RangeMap`).
  This module is different in kind: it encodes one part's datasheet — fixed register
  addresses, a fixed init sequence, a fixed 14-byte accel/temp/gyro layout, an
  MPU6050-specific sensitivity constant. A generic `imu`/`accel` trait façade would
  imply a validated abstraction across ≥2 devices; there is exactly one. So v1 names
  the module for the device it is (`tamer::mpu6050`) and defers any façade until a
  second IMU actually forces the boundary. This mirrors how `hall` encodes
  linear-Hall domain knowledge without pretending to be generic over all analog
  sensors.

- **Why split the reusable mechanisms out of the device module.** The donated code
  bundles three things: (a) genuinely device-specific parsing/registers, (b) an
  exponential-moving-average filter, (c) two-axis tilt trigonometry. (b) and (c) are
  hardware-agnostic mechanisms — an EMA smoother is the float sibling of the existing
  `smoothing::SlidingAverage`, and tilt-from-two-axes works for any accelerometer.
  Burying them under `mpu6050` would hide reusable math behind a device name and a
  device-flavoured feature. So the EMA filter moves to `tamer::smoothing` and the
  tilt functions get their own small `tamer::tilt` module (whose name matches its
  Cargo feature 1:1). `mpu6050` keeps only what is truly MPU6050-specific.

- **Why the numeric-convention exception is called out.** Every `tamer` type to
  date is integer-only by deliberate design (`analog` uses widened `u32` intermediates
  precisely to avoid float rounding). This module introduces `f32` public API. That
  is accepted here as a *documented, contained exception*, not silent drift — see the
  Behavioral contract and the `f32`-exception row in the Decisions table.

## Decisions

|                                                                                                            Decision | Reason                                                                                                                                                                      | Rejected Alternative                                                                                                                                        |
|--------------------------------------------------------------------------------------------------------------------:|:----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|:------------------------------------------------------------------------------------------------------------------------------------------------------------|
|                                        Land as a device-named `tamer::mpu6050` module (registers + parsing + calib) | It is sans-IO device code — legitimately pure, just concrete rather than mechanism-shaped; name it for what it is                                                           | Generic `tamer::imu` / `accel` trait façade in v1 — one implementor, no validated abstraction; premature                                                    |
|                                                        Move the EMA smoother into `tamer::smoothing` as `EmaFilter` | It is a general exponentially-weighted moving average, the float sibling of `SlidingAverage`; belongs where the next consumer will find it                                  | Keep `EmaFilter` inside `mpu6050` — device-locks a reusable mechanism, invites reinvention elsewhere                                                        |
|                                 Move tilt math into a new `tamer::tilt` module (`tilt_degrees`, `tilt_degrees_i32`) | Two-axis-to-angle trig on caller-supplied units — no MPU6050 register knowledge; a dedicated module makes the `tilt` feature name self-documenting                          | Keep `mpu6050::tilt_degrees` — mislabels generic trig as device-specific, gives it a device-flavoured feature                                               |
|                                   Gate only the tilt functions behind a `tilt` feature (`tilt = ["dep:micromath"]`) | `atan2` needs `micromath` (no_std CORDIC); parsing / registers / calibration / EMA need no float lib, so the core stays dependency-free by default                          | Gate the whole `mpu6050` module behind `tilt` — forces dependency-free code behind an unrelated feature                                                     |
|                                                            Keep `EmaFilter` unconditional (default, un-gated) build | Exponential decay is plain `core` `f32` multiply-add — no `micromath`; a consumer wanting EMA-smoothed ADC data must not pull in a CORDIC `atan2`                           | Gate `EmaFilter` behind `tilt` — false coupling between smoothing and trigonometry                                                                          |
|                                                  Use `micromath` (not `libm`) for the no_std `atan2` implementation | Purpose-built for `no_std`/`f32`/small code size (CORDIC approximations); tilt-angle is not scientific computing                                                            | `libm` — full-precision C math port, larger code and heavier than this needs                                                                                |
|                                          Expose `INIT_SEQUENCE` as a `&'static [(u8, u8)]` slice, not a fixed array | Array length is part of the type; a slice lets the init sequence grow (e.g. an added config write) in a later minor version without a breaking change                       | `pub const INIT_SEQUENCE: [(u8, u8); 5]` — length change is a breaking API change                                                                           |
|              `RawReading` uses private fields + accessor methods, constructed only via `parse_raw` (no public ctor) | Matches the decoded-value-type precedent exactly (`AnalogSample`: private fields, `raw()`/`percent()` accessors); leaves room to add fields non-breakingly                  | `#[non_exhaustive]` with public fields, or freely struct-literal-constructible fields — the latter makes an additive field a breaking change                |
|                 `EmaFilter::new(alpha)` panics if `alpha` ∉ `0.0..=1.0` (NaN included), documented under `# Panics` | One exact, fail-fast contract matching the crate's construction-invariant idiom (`AnalogRange::new` / `RangeMap::new` `assert!`); `alpha` is a compile-time tuning constant | Silent clamp (hides caller error), or a `Result`-returning ctor (crate reserves `Result` for methods fed external data, not literal-invariant construction) |
|                                     Return accel offsets as named accessors / a named type, not a bare `(i16, i16)` | A tuple return can't gain named fields later without a signature break; mirrors `AnalogRange::min()/max()` precedent                                                        | `offsets() -> (i16, i16)` tuple — positional Y/Z is error-prone and semver-fragile                                                                          |
|      Document `f32` as a deliberate, contained exception: integer by default, float only under `tilt` + `EmaFilter` | `tamer`'s determinism/no-float story stays intact for the default build; FPU-less targets simply don't enable `tilt`                                                        | Silently accept mixed int/float core — hides a precedent-setting change from future maintainers                                                             |

## Behavioral contract

The mapping the implementation and its tests must satisfy.

- **Parsing is total and exact.** `parse_raw(&[u8; 14]) -> RawReading` maps the
  MPU6050 burst-read layout (accel X/Y/Z, temperature, gyro X/Y/Z, each big-endian
  `i16`) to typed fields. It takes a fixed-size `[u8; 14]`, so it cannot panic on a
  short buffer and needs no length check.
- **Registers are datasheet facts.** `I2C_ADDR` (`0x68`), `I2C_ADDR_ALT` (`0x69`),
  the `REG_*` set (`PWR_MGMT_1`, `ACCEL_CONFIG`, `GYRO_CONFIG`, `SMPLRT_DIV`,
  `CONFIG`, `WHO_AM_I`, `ACCEL_XOUT_H`), `WHO_AM_I_VALUE`, and `ACCEL_SENSITIVITY_2G`
  are `pub const` scalars. `INIT_SEQUENCE` is a `&'static [(register, value)]` slice
  a driver writes in order at bring-up.
- **Calibration averages, never extrapolates.** `AccelCalibration` accumulates
  no-tilt accel samples (`add_sample`, `count`) and yields per-axis zero offsets;
  `apply_offsets` subtracts them from a `RawReading` into widened intermediates so no
  step overflows. Offsets are exposed by named access, not a positional tuple.
- **Tilt is angle-from-two-axes, tolerance-checked.** `tilt_degrees(accel_y,
  accel_z) -> f32` (and the `_i32` variant) computes inclination via `atan2` over two
  accel axes in shared units (raw LSB or physical g — the function is scale-free). It
  is defined at the origin (`atan2(0, 0)` yields a finite documented value, no NaN
  panic). Because `micromath`'s CORDIC `atan2` is a bounded-error approximation (not
  bit-exact vs `libm`/`std`), host tests assert results within an epsilon, never by
  exact `f32` equality.
- **EMA is deterministic multiply-add.** `EmaFilter::new(alpha)` /
  `update(sample) -> f32` / `value()` / `reset()` implement
  `next = alpha * sample + (1 - alpha) * prev`. Pure `core` float ops → bit-exact
  host-vs-target, so its tests may assert exact values. `new` **panics** if `alpha`
  is not in `0.0..=1.0` (NaN included), documented under `# Panics` — the single,
  fail-fast contract, matching the crate's construction-invariant idiom
  (`AnalogRange::new`, `RangeMap::new`, `SlidingAverage`'s `N > 0` assert). Not a
  silent clamp and not a `Result`: `alpha` is a compile-time-known tuning constant,
  so an out-of-range value is a programmer error to surface loudly, not a runtime
  condition to thread through a `Result` (which the crate reserves for methods fed
  external data, e.g. `HallSensor::calibrate_from_samples`).

## Constraints

- Pure `no_std`, alloc-free, host-testable; **no I2C / HAL / chip-crate coupling** —
  consistent with `analog` / `hall` / `smoothing`. MSRV 1.88.
- The default build stays **dependency-free** beyond the existing optional
  `embedded-hal` (`hal` feature). `micromath` enters only via the opt-in `tilt`
  feature (`dep:micromath`, `default-features = false`, pinned in
  `[workspace.dependencies]`).
- `#[package.metadata.docs.rs] all-features = true` already builds every feature, so
  `tilt` renders on docs.rs with no extra config; gated items get rustdoc's automatic
  "available on feature" banner from their `#[cfg]`.
- Value types follow crate idiom: `#[derive(Debug, Clone, Copy, PartialEq, ...)]`,
  `const fn` constructors where feasible, `#[must_use]`, `# Panics`/`# Errors`
  sections, `Display` on any error enum.
- Pure value types with no hardware trait → **no `Noop*` mock required** (that rule
  applies to hardware-interaction traits; this module has none).
- Never imports `rustyfarian-ws2812` — the hard `VISION.md` boundary.

## Module & API surface (v1)

- `tamer::mpu6050` — `I2C_ADDR`, `I2C_ADDR_ALT`, `REG_*`, `WHO_AM_I_VALUE`,
  `ACCEL_SENSITIVITY_2G`, `INIT_SEQUENCE`, `RawReading`, `parse_raw`,
  `AccelCalibration`, `apply_offsets`.
- `tamer::smoothing` — gains `EmaFilter` next to `SlidingAverage`.
- `tamer::tilt` (feature `tilt`) — `tilt_degrees`, `tilt_degrees_i32`.
- **Prelude**: add `mpu6050::{RawReading, AccelCalibration}` and
  `smoothing::EmaFilter` unconditionally (hero types, matching the preluded
  `AnalogCalibration` / `HallSensor` / `SlidingAverage` precedent). **Exclude** the
  raw `REG_*` / `INIT_SEQUENCE` / `WHO_AM_I_VALUE` constants (no module preludes raw
  constants). Tilt functions stay out of the prelude (it currently exports only
  types/traits, no free functions).

## Required tests (host)

- **Parsing:** `parse_raw` decodes a known 14-byte big-endian buffer into the exact
  accel/temp/gyro field values, including negative (two's-complement) values.
- **Calibration:** offsets equal the mean of accumulated no-tilt samples; `count`
  tracks sample count; `apply_offsets` subtracts correctly with no overflow at i16
  extremes.
- **Tilt:** `tilt_degrees` matches reference angles within epsilon at cardinal
  orientations (level, ±90°); `atan2(0, 0)` returns the documented finite value
  without panic; `_i32` variant agrees with the `f32` variant within tolerance.
- **EMA:** first `update` after construction/`reset` returns a defined value;
  repeated identical samples converge toward that sample; `alpha` bounds behavior
  (0 = frozen, 1 = pass-through) holds; `EmaFilter::new` panics on out-of-range and
  NaN `alpha` (`#[should_panic]`); assertions are exact (deterministic).
- **Semver guards:** `RawReading` is constructed only via `parse_raw` — private
  fields, accessor methods, no public constructor (compile-fail on struct-literal
  construction from a downstream crate); `INIT_SEQUENCE` iterates as a slice.

## Deferred (explicitly decided — not open)

- **Generic `imu` / `accel` trait façade → later.** Revisit only when a second IMU
  is added and the abstraction boundary is real. v1 is deliberately device-concrete.
- **Gyro-derived orientation / sensor fusion → not planned.** v1 parses gyro axes
  but adds no fusion (complementary/Kalman) math; an angle comes from accel tilt only.
- **Full-scale-range (FSR) selection beyond ±2 g → later, additively.** v1 ships the
  ±2 g sensitivity constant; wider ranges are a non-breaking addition when needed.
- **Fixed-point tilt (no `micromath`) → not planned.** A Q-format `atan2` to keep
  tilt integer-only is over-engineering for one consumer; the `tilt` feature gate is
  the chosen containment.

## Open Questions

- [ ] **`AccelCalibration` genericity.** Its shape (running average over `i16` accel
  samples) is close to a generic `Calibration<T>`, and conceptually overlaps
  `analog::AnalogCalibration`. Keep it `mpu6050`-local for v1 (its Y/Z accel
  semantics and `RawReading` coupling tie it here), or extract a shared type now?
  Leaning: keep local; extract only if a second consumer needs the same shape.
- [ ] **Hardware example / tier twin.** Should v1 ship an esp-hal / esp-idf example
  (I2C burst read → `parse_raw` → `tilt_degrees`), or land the pure module first and
  add the twin on demand? (An I2C display pairing is the natural showcase but pulls in
  a display driver.)
- [ ] **Prelude for tilt functions.** Confirm the lean to exclude free functions from
  the prelude (call `tamer::tilt::tilt_degrees` explicitly), keeping the prelude
  types-only.

## State

- [x] Design approved (fit confirmed against `VISION.md`; device-named module;
  reusable mechanisms split into `smoothing`/`tilt`; `micromath` feature-gated;
  behavioral contract specified)
- [x] Core implementation (`tamer::mpu6050` + `smoothing::EmaFilter` + `tamer::tilt`)
- [x] Host tests passing (per **Required tests** above): 147 unit + 17 doctests default; 184 + 24 with `--all-features`, all green
- [ ] Documentation updated (module docs, prelude exports, doctests, CHANGELOG;
  numeric-convention exception noted)

## Session Log

- 2026-07-10 — Feature doc created via `/feature`. Assessed fit with `rust-engineer`:
  the sans-IO MPU6050 primitive fits the pure core (imports no I2C/HAL, no `ws2812`
  coupling), per `VISION.md`'s input+output peripheral scope. Chose a device-named
  `tamer::mpu6050` over a premature generic `imu` façade (one implementor); split the
  reusable `EmaFilter` into `tamer::smoothing` and the tilt trig into a new
  `tamer::tilt` module so device code doesn't hide general mechanisms. Gated only the
  `atan2`-based tilt functions behind a `tilt` feature (`dep:micromath`), keeping the
  default build dependency-free; `EmaFilter` stays unconditional (plain `core`
  floats). Recorded semver-hardening decisions (`INIT_SEQUENCE` as a slice,
  `RawReading` non-literal-constructible, named accel-offset access) and flagged
  `f32` as `tamer`'s first, deliberately contained, floating-point surface. Open:
  `AccelCalibration` genericity, `RawReading` field-exposure form, a hardware example
  twin, and whether tilt-free functions belong in the prelude.
- 2026-07-11 — PR-review firming-up (docs-only): locked `EmaFilter::new(alpha)` to
  **panic** on out-of-range/NaN (crate idiom, not clamp/`Result`) and `RawReading` to
  private-fields-plus-accessors via `parse_raw`; added a `docs/features/README.md` index.
- 2026-07-12 — Core implementation landed via agent team (dependency-manager → pin;
  rust-engineer → the three modules; code/doc/justfile reviewers). 147 unit + 17
  doctests default, 184 + 24 `--all-features`, all green. Ticked Core implementation +
  Host tests. Added ADR-003 for the `micromath` feature-gate decision and a
  `compile_fail` doctest locking the `RawReading` no-public-ctor guard; updated ROADMAP.
- 2026-07-12 — Post-review fix (blocking): `AccelCalibration::offsets()` computed
  `avg_z - 16384` in `i16` — a debug-build panic for an upside-down posture. Widened
  the offset pipeline to `i32` (`AccelOffsets`, `apply_offsets`) and added an
  extreme-negative-Z regression test. code-reviewer confirmed resolved; `just verify`
  green (149 + 18; 186 + 25 `--all-features`).
- 2026-07-13 — I2C bus bring-up: twin bus scanners (`hal_c3_i2c_scan` /
  `idf_c3_i2c_scan`) landed as diagnostics on ESP32-C3 to de-risk the bus before
  the MPU6050 hardware example. Both build cleanly; GPIO 4 (SDA) / GPIO 5 (SCL) /
  100 kHz are now the canonical bus pins, to be reused by the MPU6050 example and
  future I2C peripherals (display, etc.). Recorded ADR-004 capturing the I2C bus
  pattern and the bring-up-diagnostic exemption to the "every peripheral needs a
  pure core" rule (diagnostics are validation tools, not drivers). Review catch:
  the idf scanner originally collapsed all `I2cDriver::write` errors to "device
  absent"; fixed to discriminate NACK (`ESP_FAIL`) from genuine bus faults
  (`ESP_ERR_TIMEOUT`, etc.) via `err.code()` dispatch, mirroring esp-idf-hal's own
  `to_i2c_err` classifier; esp-hal twin already distinguished
  `Error::AcknowledgeCheckFailed` from other errors. Both scanners now surface
  stuck buses as warnings, not silence.
