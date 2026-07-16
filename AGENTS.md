# AGENTS.md

> Fast-path operating guide for AI coding agents on this project.
> Prefer repository truth over assumptions — check the files referenced below.

## Project Overview

`rustyfarian-peripherals` is a Cargo workspace of reusable **hardware peripheral**
crates for embedded Rust — input (pins, debounce, presence, rotary encoders,
analog controls, button events, Hall/tilt sensing) *and* output (tone/buzzer
sequencing). The design is **sans-io**: pure decoding/timing/rendering logic
lives in a `no_std` crate with no hardware dependency, and two thin hardware
tiers (esp-hal, ESP-IDF) provide the GPIO glue. Target hardware: ESP32 (RISC-V
C3/C6 and Xtensa ESP32/S3). MSRV is `1.88`. It is the peripheral layer of the
rustyfarian family; LED output lives in the separate `rustyfarian-ws2812` repo,
which this repo never depends on.

## Architecture

Three workspace members under `crates/`:

| Crate | Role | Target |
|:------|:-----|:-------|
| `tamer` | Pure logic behind traits with `Noop*` mocks: `debounce`, `presence`, `rotary`, `button`, `analog`, `range_map`, `smoothing`, `hall`, `tilt`, `mpu6050`, `tone` | `no_std` |
| `rustyfarian-esp-hal-peripherals` | Bare-metal esp-hal GPIO drivers + device examples; re-exports `tamer` | `no_std` |
| `rustyfarian-esp-idf-peripherals` | ESP-IDF (std) drivers — ships `rotary::Encoder` (interrupt-driven, persistent raw-FFI); re-exports `tamer` | `std` (ESP-IDF) |

The pure logic in `tamer` is the contract; the hardware crates are thin wrappers
that delegate all decoding/rendering to it. `embedded-hal`'s `InputPin` (and the
relevant output/bus traits) is the seam, behind `tamer`'s optional `hal` feature.
`tamer` has two features: `hal` (embedded-hal adapters) and `tilt` (`atan2` trig
via `micromath`); the default build is dependency-free.

## Development Workflow

`just` is the canonical interface — CI calls the same recipes, so local and CI
behaviour cannot drift. Host work needs only `rustc`, `cargo`, and `just`.

| Command | Purpose |
|:--------|:--------|
| `just check` / `just test` | Check / host-test the pure core (`-p tamer`) |
| `just clippy` | Clippy, warnings denied |
| `just verify` | Non-modifying gate: fmt-check + check + clippy + test |
| `just pre-commit` | Same, but auto-formats (modifies files) |
| `just ci` | CI-equivalent: fmt-check + deny + check + clippy + test |
| `just test-all-features` / `just clippy-all-features` | Exercise the `hal` + `tilt` features |
| `just check-hal` / `just check-idf` | Build a hardware tier for a device target |
| `just examples` / `just build-example <name>` / `just run <name>` | List / build / flash+monitor device examples |
| `just deny` / `just audit` / `just doctor` | License-advisory checks / RUSTSEC audit / tooling report |

The `esp` toolchain (`just setup-toolchain`, via espup) and device target config
(`just setup-cargo-config`) are needed only for the hardware crates and examples.

## Key Conventions

**Sans-io boundary.** Anything host-testable must stay out of the hardware crates
and live in `tamer`. A green host run must mean the decode/render logic is
verified. Never put pure logic inside an esp-hal/esp-idf wrapper.

**Trait-first + `Noop*` mocks.** Every hardware *interaction* is behind a trait;
ship its `Noop*` mock in the same change (the rule applies to interaction traits,
not pure value modules like `tone`). Consumers test against these mocks.

**Demand-driven.** Add primitives and tier adapters when a downstream project
needs them, not speculatively — glue stays inline in the first example until a
second consumer justifies a shared adapter.

**Chip cfg seam.** Both hardware crates' `build.rs` emit `cfg(esp32)` /
`cfg(esp32s3)` from the target triple (with `rustc-check-cfg` registration) so
driver code branches without depending on esp-hal's own cfgs, which do not
propagate to dependents.

**Exact-pinned esp stacks.** The `esp-hal` / `esp-idf` stacks are pinned with `=`
in `[workspace.dependencies]`, coordinated with the sibling repos' release waves —
never caret. See the rationale comments there.

**Documentation style.** One sentence per line. Use `sh` / `shell` / `text`
fences (not `bash`). Never put comments inside code snippets — explanatory text
goes above. ADRs follow the Michael Nygard format under
`docs/adr/NNN-short-description.md`; feature docs in `docs/features/name-vN.md`.

**Example naming.** `{hal|idf}_{chip}_{name}` (chip ∈ `c3|c6|esp32|esp32s3`),
e.g. `hal_c3_buzzer`, `idf_s3_rotary`.

## Coding Principles

- **State assumptions** before starting. If a task has multiple valid interpretations, present them rather than picking silently.
- **Simplicity first.** Minimum code that solves the problem. No features beyond what was asked. No abstractions for single-use code. No error handling for impossible scenarios.
- **Surgical changes.** Touch only what the task requires. Do not improve adjacent code, comments, or formatting. Every changed line should trace directly to the request.
- When your changes create orphans (unused imports, variables, functions), remove them. Do not remove pre-existing dead code unless asked.

## Important Files

- `justfile` — every standard task; read first when unsure how to build/test
- `Cargo.toml` (root) — workspace metadata and the exact-pinned hardware-dep stacks
- `.cargo/config.toml.dist` — device target config (opt-in via `just setup-cargo-config`)
- `crates/tamer/src/lib.rs` — the pure-core contract and the pattern new primitives follow
- `docs/key-insights.md` — CI/build conventions and resolved gotchas
- `docs/project-lore.md` — non-obvious hardware/build discoveries
- `docs/adr/` — architecture decisions (raw-FFI interrupts, I2C bus pattern, etc.)
- `VISION.md` / `docs/ROADMAP.md` — what this is for and what's planned
- `CHANGELOG.md` — release history (Keep a Changelog format)
