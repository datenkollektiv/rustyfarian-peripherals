# AGENTS.md

> Fast-path operating guide for AI coding agents on this project.
> Prefer repository truth over assumptions — check the files referenced below.

## Project Overview

`rustyfarian-peripherals` is a Cargo workspace of reusable **input peripheral**
crates for embedded Rust — pins, debounced switches, rotary encoders, and button
events. The design is **sans-io**: pure decoding/timing logic lives in a `no_std`
crate with no hardware dependency, and two thin hardware tiers (esp-hal,
ESP-IDF) provide the actual GPIO glue. Target hardware: ESP32 (RISC-V and
Xtensa). MSRV is `1.88`.

It is the input layer of the rustyfarian family; output (LEDs) lives in the
separate `rustyfarian-ws2812` repo, which this repo never depends on.

## Status: skeleton

The workspace structure, tooling, CI, and docs are in place. The input
primitives themselves are **not yet implemented** — they grow downstream-driven
(see `docs/ROADMAP.md` and `VISION.md`). Every crate builds on the host today. When
adding the first primitive, follow the pattern in the `tamer` crate docs: pure
logic + a `Noop*` mock + host tests + (optionally) a `hal`-feature adapter.

## Architecture

Three workspace members under `crates/`:

| Crate | Role | Target |
|:------|:-----|:-------|
| `tamer` | Pure debounce / rotary / button-event logic behind traits, with `Noop*` mocks | `no_std` |
| `rustyfarian-esp-hal-peripherals` | Bare-metal esp-hal GPIO drivers; re-exports `tamer` | `no_std` |
| `rustyfarian-esp-idf-peripherals` | ESP-IDF (std) GPIO drivers; re-exports `tamer` | `std` (ESP-IDF) |

The pure logic in `tamer` is the contract; the hardware crates are thin wrappers
that delegate all decoding to it. `embedded-hal`'s `InputPin` is the trait seam
between them (behind `tamer`'s optional `hal` feature).

## Development Workflow

`just` is the canonical interface — CI calls the same recipes, so local and CI
behaviour cannot drift. Host work needs only `rustc`, `cargo`, and `just`.

| Command | Purpose |
|:--------|:--------|
| `just check` | Check the host-buildable crates (`tamer` + esp-hal tier) |
| `just test` | Host-side unit tests |
| `just clippy` | Clippy, warnings denied |
| `just verify` | Non-modifying gate: fmt-check + check + clippy + test |
| `just pre-commit` | Same, but auto-formats (modifies files) |
| `just ci` | CI-equivalent: fmt-check + deny + check + clippy + test |
| `just deny` / `just audit` | License/advisory/ban checks; RUSTSEC audit |
| `just doctor` | Tooling status report |
| `just check-idf` | Check the ESP-IDF crate for a device target (requires espup) |

The `esp` toolchain (via `espup`) and the device target config
(`just setup-cargo-config`) are needed only for the hardware crates. The
`flash` / `run` / `build-example` recipes arrive with the first device example.

## Key Conventions

**Sans-io boundary.** Anything host-testable must stay out of the hardware
crates and live in `tamer`. A green host run must mean the decoding logic is
verified. Do not put pure logic inside an esp-hal/esp-idf wrapper.

**Trait-first + `Noop*` mocks.** Every hardware interaction is behind a trait;
ship its `Noop*` mock in the same change. Consumer crates test against these
mocks — never make them invent their own.

**Demand-driven.** Add primitives when a downstream project needs them, not
speculatively. Match the request; do not build a catalogue.

**Chip cfg seam.** Both hardware crates' `build.rs` emit `cfg(esp32)` /
`cfg(esp32s3)` from the target triple (with `rustc-check-cfg` registration).
When esp-idf-hal is wired, its `build.rs` must also call
`embuild::espidf::sysenv::output()` for the link step — see
`rustyfarian-esp-idf-power/build.rs`.

**Exact-pinned esp stacks.** When the hardware deps land, pin `esp-hal` /
`esp-idf` stacks with `=` in `[workspace.dependencies]`, coordinated with the
sibling repos' release waves — never caret. See the rationale comments in
rustyfarian-ws2812 and rustyfarian-network.

**Documentation style.** One sentence per line. Use `sh` / `shell` / `text`
fences (not `bash`). Never put comments inside code snippets — explanatory text
goes above the snippet. ADRs follow the Michael Nygard format under
`docs/adr/NNN-short-description.md`.

**Example naming (when examples land).** `{driver}_{chip}_{name}`, e.g.
`hal_c6_rotary`, `idf_esp32_button` — matching the sibling repos.

## Important Files

- `justfile` — every standard task; read first when unsure how to build/test
- `Cargo.toml` (root) — workspace metadata and the (commented) hardware-dep pins
- `.cargo/config.toml.dist` — device target config (opt-in via `just setup-cargo-config`)
- `crates/tamer/src/lib.rs` — the pure-core contract and the pattern new primitives follow
- `docs/key-insights.md` — CI/build conventions and resolved gotchas
- `docs/project-lore.md` — non-obvious hardware/build discoveries
- `VISION.md` / `docs/ROADMAP.md` — what this is for and what's planned
- `CHANGELOG.md` — release history (Keep a Changelog format)
