# Release Plan

Release process for the `rustyfarian-peripherals` workspace.

> **Status:** Active process — last exercised for `v0.2.0` (published to crates.io 2026-11-12, all four crates).
> The staged publication flow below is proven; update this note after each release.

## Versioning

- **Scheme:** SemVer (0.x.y — pre-stable; minor bumps may break).
- **Model:** Single workspace version in `[workspace.package]` — `tamer`, `signwriter` and the two hardware tiers move together.
- **Publish order:** `signwriter` is published right after `tamer`, because it depends on it.
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
- [ ] `[workspace.dependencies] signwriter` carries `version = "X.Y.Z"` matching `[workspace.package]`
- [ ] No uncommitted changes: `git status --short` is empty
- [ ] Working tree is on `main` and in sync with `origin/main`
- [ ] Authenticated with crates.io (see Authentication below)
- [ ] Final hardware pass done (see below); feature docs carry no hardware gates of their own

The hardware tiers and `signwriter` cannot dry-run publish until `tamer` of the same version is live on crates.io — see Publish Order.

### Final hardware pass

One row per gate `<board>/<check>`; the scenario `bench/<board>/<check>.toml` says what the check does ([bench/README.md](bench/README.md)).
A gate passes when `--unattended` and `--attended` both pass on one commit; a change to `tamer` or a tier reopens the gates that run through it.

| Gate | Result | Date | Commit | Board |
|:--|:--|:--|:--|:--|
| `s3/b3f` | pass | 2026-11-11 | c8907e9 | CrowPanel S3 |
| `s3/rotary` | pass | 2026-11-11 | c8907e9 | CrowPanel S3 |
| `s3/rotary-isr` | pass | 2026-11-11 | c8907e9 | CrowPanel S3 |
| `esp32/b3f` | pass | 2026-11-12 | 6214033 | ESP32 WROOM-32 |
| `esp32/rotary` | pass | 2026-11-12 | 6214033 | ESP32 WROOM-32 |
| `esp32/oled-ssd1306` | pass | 2026-11-12 | 6214033 | ESP32 WROOM-32 |
| `esp32/oled-sh1106` | open: no SH1106 module | | | |
| `c3/b3f` | pass | 2026-11-12 | 38dfe55 | C3 bench rig |
| `c3/buzzer` | pass | 2026-11-12 | 38dfe55 | C3 bench rig |
| `c3/hall-linear` | pass | 2026-11-08 | 9430069 | C3 SuperMini |
| `c3/poti` | pass | 2026-11-08 | 9430069 | C3 SuperMini |
| `c3/oled` | rerun: `signwriter` P1 fix | 2026-11-08 | 2d04fbd | C3 bench rig |
| `c3/mpu6050` | open: idf attended | 2026-11-12 | d905622 | C3 SuperMini |
| `c3/rotary` | open | | | |
| `c3/rig` | open | | | |
| `c6/b3f` | open | | | |
| `c6/rotary` | open | | | |
| `c6/oled-ssd1306` | open | | | |
| `c6/oled-sh1106` | open: no SH1106 module | | | |
| `esp32-feather/oled-*` | deferred: no STEMMA QT cable | | | |
| `c3/bme680`, `c3/bme680-async` | deferred: `bme680` experimental | | | |
| `s3/oled` | n/a: I2C is the touch bus | | | |

Manual items without a scenario:

- MPU6050 wrong address prints the AD0 hint (rebuild with `I2C_ADDR_ALT`).
- `poti_led` on the C3 rig.
- BME680 (deferred): `new_data` clear after drain, `ctrl_gas_0` / `meas_status_0` bits, a real register dump for the fixture.
- Optional: `hal_c6_passive_buzzer_arpeggio` pitch (suspected an octave high).

## Version Bump

Files to update when bumping to `X.Y.Z`:

- `Cargo.toml` — `[workspace.package] version = "X.Y.Z"`
- `Cargo.toml` — `[workspace.dependencies] tamer = { path = ..., version = "X.Y.Z", ... }`
- `Cargo.toml` — `[workspace.dependencies] signwriter = { path = ..., version = "X.Y.Z" }`
- `Cargo.lock` — refreshed by any `just check` after the bump

Post-release version bump: none.

## Publish

**Target:** crates.io — `tamer`, `signwriter`, `rustyfarian-esp-hal-peripherals`, `rustyfarian-esp-idf-peripherals`.
**Credentials:** crates.io API token (see Authentication).
**Downstream consumption:** versioned `[dependencies]` from crates.io.

### Authentication

`cargo publish` reads the token from `~/.cargo/credentials.toml`; authenticate once per machine:

```sh
cargo login <your-crates-io-token>
```

Alternatively set `CARGO_REGISTRY_TOKEN` in the environment for the duration of the publish run.

### Crate Ownership

Sole owner of every published crate: `fwaibel@datenkollektiv.de`.
Move to a GitHub team owner (e.g. `github:datenkollektiv:wheel`) on the first merged external PR or at `v1.0.0`.

### Publish Order

Both hardware tiers depend on `tamer`, so publishing is staged.

**Stage 1 — pure core:**

```sh
just release-publish
```

Verify `https://crates.io/crates/tamer` shows `X.Y.Z` before continuing.

**Stage 1b — `signwriter` (same wave, after `tamer` is live):**

```sh
cargo publish -p signwriter --dry-run
cargo publish -p signwriter
```

Use `just release-dry-run-signwriter`, then `just release-publish-signwriter`.
`signwriter` goes before Stage 2 because the tiers list it as a dev-dependency, so it must be live before their dry-runs.
Verify `https://crates.io/crates/signwriter` shows `X.Y.Z` before continuing.

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
- Fix forward with a lockstep patch bump (e.g. `0.1.0` → `0.1.1`) across `tamer`, `signwriter` and the two tiers.
- If `tamer` was yanked, re-run Stages 1–3 in full.
- If only a tier was yanked, re-run Stage 1 (lockstep) and Stages 2–3 for that tier.
- If publishing failed before anything went live: `git push --delete origin vX.Y.Z`, `git tag -d vX.Y.Z`, `git revert HEAD`.

## Release Record Location

Each release produces files in `release/`:

1. `YYYY-MM-DD-<version>-preflight.md` — pre-flight assessment
2. `YYYY-MM-DD-<version>-plan.md` — ordered execution plan
3. `YYYY-MM-DD-<version>-record.md` — what was done and what remains
