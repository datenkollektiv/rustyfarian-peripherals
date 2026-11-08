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
Each item has a gate id, `<board>/<check>`, and the bench command that produces its evidence: `just bench <board> <check>`.
Items without a bench scenario keep a manual note and are ticked by hand.
Every tick records the commit hash and the board (the ELF SHA-256 is optional) and the date; record new gotchas in `docs/project-lore.md`.
A change to `tamer` or to a touched tier invalidates the prior ticks of every flow that runs through it.
Bench results are local and git-ignored (`bench/*/results.jsonl`), so the tick itself must carry the commit and the date.
Run each board twice: `just bench <board> --unattended` (walk away), then `just bench <board> --attended`; a gate is ticked only when both modes pass on the same commit.

Board scenarios: `c3` (`b3f`, `rotary`, `bme680`, `bme680-async`, `mpu6050`, `buzzer`, `hall-linear`, `poti`), `c6` and `esp32` (`b3f`, `rotary`), `s3` (`b3f`, `rotary`, `rotary-isr`).
The chip-matrix cells are in `docs/features/chip-matrix-v1.md` § Hardware pass.

- [ ] Gate `c3/buzzer` (`just bench c3 buzzer`): the C3 passive-buzzer arpeggio on both tiers; `idf_c3_passive_buzzer_arpeggio` at 10-bit LEDC (was 8-bit) still plays the full melody; the ~77 Hz floor is computed, not measured.
- [x] Gate `c3/poti` (`just bench c3 poti`): the `poti` example on the C3 rig; `poti_led` is a manual note until a scenario covers it — pass (2026-11-08, 5a11564, ESP32-C3 SuperMini): full run on both tiers, calibrated 0..4095, 0 % / 100 % / middle each held 2 s.
- [ ] Gate `c3/mpu6050` (`just bench c3 mpu6050`): `idf_c3_mpu6050_tilt` on the C3 rig with a ±90° reading; the deliberate NACK (wrong address) that prints the AD0 hint is a manual note until the scenario prompts for it.
- [x] Gate `c3/hall-linear` (`just bench c3 hall-linear`): `hal_c3_hall_linear` / `idf_c3_hall_linear` with a 49E sensor — pass (2026-11-08, 5a11564, ESP32-C3 SuperMini): both modes on both tiers, midpoint hal 2336 / idf 2345, each pole held 2 s.
- [ ] Gate `c3/bme680` (`just bench c3 bme680`): `hal_c3_bme680` and `idf_c3_bme680` on the ADR-004 bus, incl. `heat_stab_r` at the fixed heater profile.
- [ ] Gate `c3/bme680-async` (`just bench c3 bme680-async`): `hal_c3_bme680_async` under embassy (esp-hal `I2c` in `Async` mode): same readings as the blocking twin, other tasks keep running during the conversion wait.
- [ ] BME680 manual: `new_data` is clear after drain + trigger; then drop `PENDING(hardware)` from the drain mock script.
- [ ] BME680 manual: plain `ctrl_gas_0` write (`0x00` / `0x08`) and the `meas_status_0` bits `0x20` / `0x40` behave as the poll assumes.
- [ ] BME680 manual: capture a real register dump for the test fixture.
- [ ] Gate `c3/rotary` (`just bench c3 rotary`): `hal_c3_rotary_isr` and `idf_c3_rotary` move `edge_count()` / `skipped_count()` under turning (the counting lives in `QuadratureDecoder`).
- [ ] Gate `c6/rotary` (`just bench c6 rotary`): `hal_c6_rotary_isr` and `idf_c6_rotary`, the same counts under turning.
- [ ] Gate `esp32/rotary` (`just bench esp32 rotary`): `hal_esp_rotary_isr` and `idf_esp_rotary`, the same counts under turning.
- [ ] Gate `s3/rotary` (`just bench s3 rotary`): `hal_s3_rotary` and `idf_s3_rotary` on the CrowPanel S3.
- [ ] Gate `s3/rotary-isr` (`just bench s3 rotary-isr`): `hal_s3_rotary_isr` under load; there is no ESP-IDF twin.
- [ ] Gate `s3/b3f` (`just bench s3 b3f`): hal passed on 2026-11-06 on the CrowPanel S3; the tick stays open until the commit is recorded, and the idf twin `idf_s3_b3f` is not run.
- [ ] Gate `c3/b3f`, `c6/b3f` and `esp32/b3f` (`just bench <board> b3f`): the `b3f` smoke on each board and tier, not run.
- [ ] Chip matrix rollup: every cell of `docs/features/chip-matrix-v1.md` § Hardware pass reads `pass (...)` or `n/a (...)`; the gate items above tick the cells.
- [ ] Optional manual: `hal_c6_passive_buzzer_arpeggio` pitch (suspected one octave high from the C6 LEDC clock); no bench scenario covers it.

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
