# Release Plan

Release process for the `rustyfarian-peripherals` workspace.

> **Status:** Active process — last exercised for `v0.1.0` (published to crates.io 2026-09-26, all three crates).
> The staged publication flow below is proven; update this note after each release.

## Versioning

- **Scheme:** SemVer (0.x.y — pre-stable; minor bumps may break).
- **Model:** Single workspace version in `[workspace.package]` — all three crates move together.
- **Snapshot convention:** None; crates live at the released version between releases.
- **Who decides version:** Maintainer; minor bump for new or breaking public API, patch for fixes only.

## Branch and Tag Convention

- **Release branch:** `main`
- **Tag format:** `vX.Y.Z` (e.g. `v0.1.0`)
- **Tagging:** Manual annotated tag on the release commit.

## Pre-flight Checklist

- [ ] Non-modifying gate passes: `just verify` (fmt-check, check, clippy `-D warnings`, test)
- [ ] `tamer` feature matrix passes: `just test-all-features` and `just clippy-all-features`
- [ ] Device tiers compile: `just check-hal` and `just check-idf`
- [ ] `cargo deny` clean: `just deny`
- [ ] `cargo audit` clean: `just audit`
- [ ] `tamer` packages cleanly: `just release-dry-run`
- [ ] Changelog has an `[Unreleased]` section with content for this release
- [ ] `[workspace.dependencies] tamer` carries `version = "X.Y.Z"` matching `[workspace.package]`
- [ ] No uncommitted changes: `git status --short` is empty
- [ ] Working tree is on `main` and in sync with `origin/main`
- [ ] Authenticated with crates.io (see Authentication below)
- [ ] Final hardware pass done (see below); feature docs carry no hardware gates of their own

The hardware tiers cannot dry-run publish until `tamer` of the same version is live on crates.io — see Publish Order.

### Final hardware pass

Hardware checks are collected here, not gated per feature, and run once before the release.
Tick each item with the board, port, and date; record new gotchas in `docs/project-lore.md`.

- [ ] Every example touched by the ADR-010 alignment runs on both tiers: `tilt_motion`, `hall_linear`, `poti`, `poti_led`, `passive_buzzer_arpeggio` (C3 twins, plus `hal_c6_passive_buzzer_arpeggio`).
- [ ] `idf_c3_passive_buzzer_arpeggio` at 10-bit LEDC (was 8-bit) still plays the full melody; the ~77 Hz floor is computed, not measured.
- [ ] Rotary `edge_count()` / `skipped_count()` move under turning on both tiers (the counting now lives in `QuadratureDecoder`).
- [ ] MPU6050: `idf_c3_mpu6050_tilt` on the C3 rig; a ±90° reading; a deliberate NACK (wrong address) prints the AD0 hint.
- [ ] BME680: `hal_c3_bme680` and `idf_c3_bme680` on the ADR-004 bus, incl. `heat_stab_r` at the fixed heater profile.
- [ ] BME680: `hal_c3_bme680_async` under embassy (esp-hal `I2c` in `Async` mode): same readings as the blocking twin, other tasks keep running during the conversion wait.
- [ ] BME680: `new_data` is clear after drain + trigger; then drop `PENDING(hardware)` from the drain mock script.
- [ ] BME680: plain `ctrl_gas_0` write (`0x00` / `0x08`) and the `meas_status_0` bits `0x20` / `0x40` behave as the poll assumes.
- [ ] BME680: capture a real register dump for the test fixture.
- [ ] Hall linear: `hal_c3_hall_linear` / `idf_c3_hall_linear` with a 49E sensor.
- [ ] Chip matrix hardware pass on ESP32, C3, C6 and S3 (`docs/features/chip-matrix-v1.md`), incl. the ESP-IDF rotary `Encoder` on C3 and C6.
- [ ] Optional: `hal_c6_passive_buzzer_arpeggio` pitch (suspected one octave high from the C6 LEDC clock).

## Version Bump

Files to update when bumping to `X.Y.Z`:

- `Cargo.toml` — `[workspace.package] version = "X.Y.Z"`
- `Cargo.toml` — `[workspace.dependencies] tamer = { path = ..., version = "X.Y.Z", ... }`
- `Cargo.lock` — refreshed by any `just check` after the bump

Post-release version bump: none.

## Publish

**Target:** crates.io — `tamer`, `rustyfarian-esp-hal-peripherals`, `rustyfarian-esp-idf-peripherals`.
**Credentials:** crates.io API token (see Authentication).
**Downstream consumption:** versioned `[dependencies]` from crates.io.

### Authentication

`cargo publish` reads the token from `~/.cargo/credentials.toml`; authenticate once per machine:

```sh
cargo login <your-crates-io-token>
```

Alternatively set `CARGO_REGISTRY_TOKEN` in the environment for the duration of the publish run.

### Crate Ownership

Sole owner of all three crates: `fwaibel@datenkollektiv.de`.
Move to a GitHub team owner (e.g. `github:datenkollektiv:wheel`) on the first merged external PR or at `v1.0.0`.

### Publish Order

Both hardware tiers depend on `tamer`, so publishing is staged.

**Stage 1 — pure core:**

```sh
just release-publish-tamer
```

Verify `https://crates.io/crates/tamer` shows `X.Y.Z` before continuing.

**Stage 2 — dry-run the hardware tiers** (now that `tamer X.Y.Z` is live):

```sh
just release-dry-run-hal
just release-dry-run-idf
```

**Stage 3 — publish the hardware tiers (any order):**

```sh
just release-publish-hal
just release-publish-idf
```

- `release-*-hal` uses `riscv32imc-unknown-none-elf` on stable with the crate's default `esp32c3,unstable` features.
- `release-*-hal` prints esp-hal's "use --release" banner during the verify build — harmless, the upload is source.
- `release-dry-run` accepts extra flags; pass `--allow-dirty` when checking before the release commit.
- `release-*-idf` uses `cargo +esp` with `riscv32imc-esp-espidf` and `MCU=esp32c3` (requires `espup`).
- docs.rs builds `rustyfarian-esp-hal-peripherals` from its `[package.metadata.docs.rs]` target.
- docs.rs builds of `rustyfarian-esp-idf-peripherals` are expected to fail; its README is the primary documentation.

## Changelog

**Location:** `CHANGELOG.md` (workspace-level, single source for all crates)
**Format:** [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
**Process:**

1. Replace `## [Unreleased]` with `## [X.Y.Z] - YYYY-MM-DD`.
2. Add a fresh, empty `## [Unreleased]` section above it.
3. Update the **Status** note at the top of this file.

## Tagging

```sh
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin main --follow-tags
```

## GitHub Release

- Create a release from tag `vX.Y.Z` titled `v X.Y.Z`.
- Body: the `## [X.Y.Z]` changelog section verbatim.

```sh
gh release create vX.Y.Z --title "v X.Y.Z" --notes-file <extracted-section.md>
```

## Rollback Procedure

Published crate versions cannot be deleted — `cargo yank` is the only recourse:

```sh
cargo yank --version X.Y.Z <crate>
cargo yank --version X.Y.Z --undo <crate>
```

- A yanked version still resolves for existing `Cargo.lock` files but cannot be newly depended on.
- Fix forward with a lockstep patch bump (e.g. `0.1.0` → `0.1.1`) across all three crates.
- If `tamer` was yanked, re-run Stages 1–3 in full.
- If only a tier was yanked, re-run Stage 1 (lockstep) and Stages 2–3 for that tier.
- If publishing failed before anything went live: `git push --delete origin vX.Y.Z`, `git tag -d vX.Y.Z`, `git revert HEAD`.

## Release Record Location

Each release produces files in `release/`:

1. `YYYY-MM-DD-<version>-preflight.md` — pre-flight assessment
2. `YYYY-MM-DD-<version>-plan.md` — ordered execution plan
3. `YYYY-MM-DD-<version>-record.md` — what was done and what remains
