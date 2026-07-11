# ADR-003: MPU6050 device module and micromath dependency

## Status
Accepted

## Context

`tamer` was founded as a pure, host-testable, dependency-free core. The first
output feature — angle-from-accelerometer tilt calculation — challenges that
assumption: `atan2` trigonometry requires either a heavyweight libm port or a
dedicated `no_std` CORDIC library.

The inbound MPU6050 IMU module is a complete sans-IO device driver: register
constants, 14-byte burst parsing, and per-axis offset calibration. All of this
is integer-only, requires no float library, and is fully host-testable.

However, a common use case — measuring sensor orientation from two accelerometer
axes — demands `atan2` to convert XY/YZ acceleration to an angle. The need to
support this use case without forcing the dependency on all consumers required
a deliberate decision: should `tamer`'s default build stay dependency-free, or
should all consumers pull in `micromath`?

## Decision

Add `micromath = { version = "2", default-features = false }` to the workspace
dependency list, and gate it behind a new, opt-in `tilt` feature.

- **Module split:** The MPU6050-specific register logic lives in
  `tamer::mpu6050` (unconditional build, no dependencies); the EMA smoother
  (`EmaFilter`) lives in `tamer::smoothing` (unconditional, uses only `core`
  floats); and the `atan2`-based angle functions live in a new `tamer::tilt`
  module (feature-gated, depends on `micromath`).

- **Feature gating:** `tilt = ["dep:micromath"]` in `Cargo.toml`. The module,
  its types, and its functions are conditional (`#[cfg(feature = "tilt")]`).
  Consumers who do not enable `tilt` pull no CORDIC dependency — the default
  `tamer` build remains dependency-free.

- **Why not a monolithic gate?** Keeping `mpu6050` and `smoothing` ungated
  allows consumers to use register constants and offset calibration without
  buying into floating-point trigonometry. Conversely, a consumer may want EMA
  smoothing on ADC readings without touching an IMU. The split respects the
  separation of concerns: device parsing, generic smoothing, and the specific
  math that only some applications need.

## Consequences

- `tamer` gains its first optional dependency (`micromath`), but only under an
  opt-in feature; the default build is unaffected.
- `tamer` ships its first public floating-point API (`f32` in `EmaFilter` and
  `tilt_degrees`/`tilt_degrees_i32`). This is accepted as a **documented,
  contained exception** to the existing integer-only discipline — the `tilt`
  feature gate is the containment boundary. Future maintainers should treat
  this as a precedent-setting decision: floating-point logic belongs behind a
  dedicated feature, not silently in the default build.
- Documentation explicitly notes `micromath`'s CORDIC approximation accuracy
  (±0.1%) and the host-test idiom (epsilon-based assertions, never exact
  `f32` equality).
- Consumers on platforms without an FPU or with FPU cost concerns simply do not
  enable `tilt`, keeping their binary lean.
- `docs.rs` renders all features by default (`#[package.metadata.docs.rs]
  all-features = true` in `tamer/Cargo.toml`), so the published docs show
  `tilt_degrees` and its siblings with the Cargo feature banner.

## Alternatives Considered

|  Alternative | Pros | Cons  | Why Rejected  |
|-------------:|-----:|:------|:--------------|
| Gate the entire `mpu6050` module behind a `mpu6050` feature, with `micromath` unconditional | Simpler `Cargo.toml`; no new feature | Couples register parsing and smoothing (both integer) to a float library; consumers who calibrate offsets pull in CORDIC whether they use it or not. Wastes space on platforms that measure orientation via other means (e.g. a tilt-triggered threshold without reporting the angle). |
| Full `libm` crate for CORDIC instead of `micromath` | Authoritative math; bit-exact vs. `std`'s `atan2` | Larger code size (~8 KB vs. micromath's ~2 KB); heavier than the use case needs (tilt-angle reporting is ±0.1% tolerance, not scientific computing); pulls in more transitive dependencies. |
| Hand-rolled fixed-point / Q-format `atan2` to keep tilt integer-only | No external dependency; deterministic results | Over-engineering for one consumer; adds maintenance burden for the minimal precision benefit when CORDIC approximation is adequate. Tilt measurement is inherently `f32` domain (radians, degrees); converting back to integer adds noise. |
| Always enable the `tilt` feature (make `micromath` unconditional in the default build) | Single, simpler API surface | Breaks the dependency-free promise for consumers who never use angle calculation; wastes binary size; contradicts the philosophy of opt-in hardware seams. |

## Verification

- `cargo tree` with and without `--features tilt` confirms `micromath` appears
  only under the feature.
- `micromath` is audited: license MIT OR Apache-2.0 (matches workspace
  allowlist in `deny.toml`); no RUSTSEC advisories; MSRV 1.88 compatible.
- Zero transitive dependencies from `micromath`; it is a self-contained CORDIC
  math library.
- `#[forbid(unsafe_code)]` in `micromath` (verified via crate docs) means no
  FFI or platform-specific unsafety.
- The pure module ships with host unit tests and runnable doctests covering the
  `mpu6050` + `calibration` + `tilt` workflow (147 unit + 18 doctests on the
  default build; 184 + 25 with `--all-features`). Hardware-tier example twins
  (`rustyfarian-esp-hal-peripherals`, `rustyfarian-esp-idf-peripherals`) are
  deferred to an on-demand follow-up per the feature doc's open questions.
- `micromath`'s advisory-clean status is independent of a separate, pre-existing
  `just deny` advisories failure in the esp-idf tier's build-dependency chain
  (`crossbeam-epoch`, an `anyhow`-family `downcast_mut` unsoundness); that finding
  was deliberately not suppressed here — see `docs/project-lore.md`.
