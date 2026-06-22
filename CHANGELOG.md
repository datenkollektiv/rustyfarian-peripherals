# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor
bumps may carry breaking changes).

---

## [Unreleased]

### Added
- Initial workspace skeleton for the rustyfarian input-peripherals stack:
  - `tamer` — the pure, `no_std`, host-buildable core that will hold debounce,
    rotary quadrature decoding, and button-event logic behind traits with
    `Noop*` mocks. Ships with an optional `hal` feature seam over
    `embedded-hal`. No primitives yet — these grow downstream-driven.
  - `rustyfarian-esp-hal-peripherals` — bare-metal (esp-hal, `no_std`) hardware
    tier; thin re-export of `tamer` with chip-feature and `build.rs` cfg seams.
  - `rustyfarian-esp-idf-peripherals` — ESP-IDF (std) hardware tier; thin
    re-export of `tamer` with a `build.rs` cfg seam.
- Tooling: `justfile` (canonical task interface), `deny.toml`,
  `.cargo/config.toml.dist` (multi-chip device targets), and
  `scripts/` (`host-target.sh`, `doctor.sh`).
- CI: `rust`, `fmt`, `clippy`, and `audit` GitHub Actions workflows (all
  just-recipe driven, host toolchain) plus Dependabot.
- Docs: `README`, `VISION`, `ROADMAP`, `AGENTS`, `CONTRIBUTING`,
  `CODE_OF_CONDUCT`, ADR/feature templates, and `docs/`
  (`project-lore`, `key-insights`, `hardware-setup`).
- Dual `LICENSE-MIT` / `LICENSE-APACHE` at the workspace root and per crate.
