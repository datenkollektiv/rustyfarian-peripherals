# Maintenance Plan

Regular maintenance workbook for `rustyfarian-peripherals` — a Cargo workspace of
the pure `no_std` `tamer` core plus two thin ESP32 hardware tiers
(`rustyfarian-esp-hal-peripherals` bare-metal, `rustyfarian-esp-idf-peripherals` std)
targeting ESP32-C3 / C6 (RISC-V) and ESP32 / S3 (Xtensa).

Covers: build verification, dependency updates, security scanning, CI/CD status,
device-target compile validation, hardware validation, and toolchain freshness.

The first end-to-end cycle ran on 2026-09-25; the artefacts in [`audit/`](audit/)
(git-ignored) serve as concrete templates for the audit / plan / maintenance file
structure. The sibling `rustyfarian-ws2812` runs the same protocol and pins the
same esp stack — its `maintenance-plan.md` and `audit/` history are the reference
when a wave arrives there first.

## Build & Test

### Primary build gate
- `just verify` — non-modifying: fmt-check, check, clippy (`-D warnings`), host tests
  (unit + doc) for `tamer`. Exit 0 is the audit's primary PASS signal.
- `just ci` — the CI-equivalent chain (adds `just deny`).
- `just test-all-features` / `just clippy-all-features` — `hal` + `tilt` feature
  surfaces; run when a bump touches `embedded-hal` or `micromath`.

A `just verify` doctest failure with `no X in module` for a type that plainly exists
is usually a stale `tamer` rlib in `target/ide` (see `docs/project-lore.md`
§ Build & Validation); `touch` the source file and re-run before diagnosing further.

### Device-target compile checks
The hardware tiers do not build for the host. After any esp-* bump:
- `just check-hal` — esp-hal tier, ESP32-C3 (`riscv32imc-unknown-none-elf`, stable + rustup target).
- `just check-idf` — ESP-IDF tier, ESP32-C3 (`riscv32imc-esp-espidf`, `cargo +esp`).
- `just build-example <name>` — examples are where HAL API breakage surfaces; the
  library crates barely touch the HALs. Spot-check one example per peripheral
  family the bump could affect **and** one per chip family:
  - GPIO in/out: `hal_c3_blink`, `idf_c3_blink`, `hal_c6_b3f`
  - ADC + LEDC PWM: `hal_c3_poti_led`, `idf_c3_poti_led`
  - LEDC retune (long-lived timer borrow): `hal_c3_passive_buzzer_arpeggio`
  - I2C: `hal_c3_i2c_scan`, `idf_c3_i2c_scan`
  - Xtensa bare-metal: `hal_esp_b3f`, `hal_s3_b3f`
  - Xtensa ESP-IDF + `critical-section` link path: `idf_s3_rotary`

Device builds need `just setup-cargo-config` (linker scripts / targets) and, for
ESP-IDF and Xtensa, `just setup-toolchain` (espup `esp` channel). `just doctor`
reports both plus the resolved (RAM-disk) target dirs.

### Hardware tests
`just run <example>` builds, flashes, and opens the serial monitor. Pass criteria:
- The example's documented behaviour is observable (LED blinks / dims, encoder
  events log, I2C scan lists the expected address, tone plays).
- Serial monitor shows normal output only — no panic, watchdog reset, or backtrace.
- Stable for a 60-second run and reproducible across a re-flash.

Minimum after an esp stack bump, boards permitting: `hal_c3_blink` and
`idf_c3_blink` (both tiers boot on the new stack), plus `idf_s3_rotary` when an
S3 is present (the ISR + `critical-section` path that `cargo check` cannot prove).
Record "compile-verified only" honestly when no board is attached.

## Dependency Updates

### Workspace `Cargo.toml`
All shared dependency versions live in the root `[workspace.dependencies]`; each
entry carries a comment explaining the constraint. Internal crates are path-only
(no `version`) while pre-publication.

Cargo treats `"1.0"` as `^1.0` and `"=1.0.0"` as exact pinning — comments must
reflect the actual semantics.

### Pure-logic crates (host-buildable, liberal policy)
- `embedded-hal`, `nb`, `micromath` — caret ranges; patch/minor bumps are safe and
  covered by host tests. `micromath` is `tamer`'s only floating-point dependency
  (behind `tilt`; see ADR-003).
- `log`, `anyhow` — std-only, used by the ESP-IDF tier; caret ranges, refreshed by
  `just update`.

### ESP-IDF stack (exact pins)
- `esp-idf-hal`, `esp-idf-svc`, `esp-idf-sys`, `embuild` move together: each
  `esp-idf-hal` minor requires a matching `esp-idf-sys` minor and an `embuild`
  floor, and `esp-idf-svc` requires the same `esp-idf-hal` minor. Bump all four in
  one diff.
- `critical-section = "=1.2.0"` stays pinned exactly: the rotary ISR relies on
  esp-idf-hal supplying the FreeRTOS-backed, ISR-safe implementation via
  `esp-idf-svc/critical-section`. On every `esp-idf-hal` bump re-read
  `src/task.rs` (`critical_section::set_impl!`) and `src/interrupt.rs`
  (`IsrCriticalSection` / `vPortEnterCritical`) at the new tag and record the
  re-check date in `docs/project-lore.md`.
- `ESP_IDF_VERSION` (`.cargo/config.toml.dist`) is a separate decision from the
  crate pins; moving it costs an IDF download and an `sdkconfig` re-validation.

### esp-hal stack (exact pins)
- `esp-hal`, `esp-bootloader-esp-idf`, `esp-println` are released from the
  `esp-rs/esp-hal` monorepo as a coordinated wave; treat them together. `esp-hal`
  itself often lands a week after the companion crates.
- Pre-1.0 companions signal breaking changes with minor bumps; `esp-hal` minors
  (1.1 → 1.2) do too — read the `CHANGELOG.md` at the new tag for the ADC, LEDC,
  I2C, GPIO, `delay` and `time` entries, the peripherals this tier's examples use.
- `esp-hal` may raise its `rust-version`; the hardware tiers follow it, `tamer`
  does not (see MSRV policy below).

### MSRV policy
- The workspace floor (`[workspace.package] rust-version`) is `tamer`'s MSRV and
  stays as low as the pure core allows.
- The two hardware tiers declare their own `rust-version` when the esp stack
  demands a higher one (mirrors `rustyfarian-ws2812`). Cargo enforces
  `rust-version` on every package in the graph against the active toolchain,
  so stable (used by `check-hal` and the RISC-V examples) and the espup `esp`
  toolchain (Xtensa, ESP-IDF) must both report a numeric version at or above
  the tiers' MSRV. Never route RISC-V bare-metal builds through a floating
  nightly: a nightly older than a stabilisation date satisfies `rust-version`
  yet cannot compile the crate (lore § Build & Validation).

### Coordinated esp-hal upgrade — runbook
1. **Survey.** Query crates.io for the latest version of every pinned esp-* crate
   (`https://crates.io/api/v1/crates/<name>`); note `rust_version` and `created_at`.
2. **Read the changelogs** at the new tags for the peripherals in use (ADC, LEDC,
   I2C, GPIO, delay, time, `esp_app_desc!`, `esp-println` feature names).
3. **Bump** the pins in one diff; raise tier `rust-version` if required.
   `just update`; inspect `Cargo.lock` for the resolved versions.
4. **Compile check.** `just check-hal`, `just check-idf`, then the example spot-check
   list above.
5. **Re-audit.** `just audit`, `just deny`. Re-evaluate every `deny.toml` ignore
   (notably `paste` via `esp-hal`) against the new graph and re-cite the version
   and date in the comment.
6. **Docs.** `CHANGELOG.md ## [Unreleased] ### Changed` — one bullet per crate bump
   and one per applied API migration. Update version references in `AGENTS.md`,
   `README.md`, `docs/project-lore.md`, and source comments that cite a pinned
   version (`grep -rn '0\.46\|1\.1\.0'` and friends).
7. **Hardware retest** per the pass criteria above; record honestly.

### Security scanning
- `just deny` — `cargo deny check` (advisories, licenses, bans, sources).
- `just audit` — `cargo audit` against the RustSec database.
- `deny.toml` lists ignored advisories with rationale and a review-by date.
- Advisories arrive through the esp-idf build-dep chain (`embuild → globwalk →
  ignore → crossbeam-*`) and `anyhow` far more often than through code this repo
  owns; `just update` usually resolves them (lore § Build & Validation).

## CI/CD
GitHub Actions workflows in `.github/workflows/`:
- `audit.yml` — RustSec advisory check (push, PR, weekly schedule).
- `clippy.yml`, `fmt.yml` — lint and format gates.
- `rust.yml` — deny + check + test.

All four call `just` recipes. Keep the action versions in step across the four
files; an action's Node runtime is only knowable from `runs.using` in its
`action.yml` at the tag in use. `gh run list` needs `gh auth login`; without it,
CI status is NOT VERIFIED.

## Documentation Freshness
Review during quarterly cycles:
- `docs/project-lore.md` — remove entries resolved upstream; re-date re-confirmed ones.
- `docs/ROADMAP.md` — still reflects priorities; drop resolved items.
- `CHANGELOG.md` — `## [Unreleased]` matches branch state.
- `AGENTS.md` / `README.md` — MSRV, crate table, feature flags. The README states
  MSRV in *two* places: the `img.shields.io` Rust badge and the MSRV paragraph
  under "Common Tasks". A tier MSRV raise must update both, or the badge silently
  advertises the old floor.
- `docs/key-insights.md` — CI action versions and build conventions.

## Scheduled Maintenance Cadence

### Monthly
- [ ] `just verify` and `just ci` pass.
- [ ] `just audit` / `just deny` clean apart from documented `deny.toml` ignores.
- [ ] `just update` for in-range transitives; re-run the gates.
- [ ] CI run status (when `gh` is authenticated).

### Quarterly
- [ ] Everything in the monthly checklist.
- [ ] Audit every `[workspace.dependencies]` pin against crates.io.
- [ ] Detect an esp-hal or esp-idf wave; if present, follow the runbook.
- [ ] Check GitHub Actions tags for runtime deprecations.
- [ ] Device-target compile checks and the example spot-check list.
- [ ] Hardware retest of at least one example per tier on an attached board.
- [ ] Re-evaluate `deny.toml` ignores; review `docs/project-lore.md` for accuracy.
- [ ] Compare pins with `rustyfarian-ws2812` — the siblings should sit on the same wave.

## Maintenance Protocol
Each cycle produces three files in `audit/` (git-ignored — internal logs):
1. `YYYY-MM-DD-<cadence>-audit.md` — read-only assessment.
2. `YYYY-MM-DD-<cadence>-plan.md` — executable plan derived from the audit.
3. `YYYY-MM-DD-<cadence>-maintenance.md` — what was actually applied, with outcomes.

Behavioural changes from a cycle belong in `CHANGELOG.md ## [Unreleased]`;
deferred concerns in `docs/ROADMAP.md`; non-obvious technical insights in
`docs/project-lore.md`. Commits are the maintainer's — the cycle leaves a
verified working tree and a suggested commit split.
