# Key Insights

This file is the persistent knowledge base for the rustyfarian-peripherals project.
All agents and Claude Code sessions should read this file at the start of relevant work and update it when new insights are discovered.

Organise entries by topic, not chronologically.
Keep each insight concise: one paragraph or a short bullet list per topic.
Remove or update entries that are superseded.

---

## Workspace shape

- **Three crates, one pure core.**
  - `tamer` holds all host-testable input logic.
  - `rustyfarian-esp-hal-peripherals` (bare-metal) and `rustyfarian-esp-idf-peripherals` (std) are thin wrappers that delegate to it.
  - Anything host-testable belongs in `tamer`, never in a hardware wrapper.
- **Skeleton, by design.**
  - No input primitives are implemented yet — they grow downstream-driven.
  - Every crate builds on the host today, so all three are covered by the host CI gates (no ESP toolchain needed).
  - Once a hardware crate pulls in `esp-hal` / `esp-idf-hal`, its check moves to a device target.
  - The device gates run in CI through the cross-target workflows (`just ci-riscv`, `ci-xtensa`, `ci-idf <chip>`, `ci-avr`).

## CI and Build Validation

- **Host workflows are thin callers.**
  - `rust.yml`, `fmt.yml`, `clippy.yml` and `audit.yml` call `datenkollektiv/rustyfarian-ci`'s `host-checks.yml@v1`.
  - Each job runs one `just ci-<job>` recipe, so the justfile stays the single source of truth.
- **Device workflows share composite actions only.**
  - The `cross-target-*.yml` files stay per repo (path filters and chips differ) and use the shared `setup-*` actions at `@v1`.
  - `cross-target-idf.yml` builds the ESP-IDF tier for all four chips.
- **Host jobs run on stable.**
  - A caller's `env:` does not reach `host-checks.yml`, so the `toolchain` input (default `stable`) is exported as `RUSTUP_TOOLCHAIN`.
  - Device recipes that need the `esp` toolchain (e.g., `check-idf`, Xtensa builds) use `cargo +esp` explicitly.
  - There is no repo-wide `rust-toolchain.toml` because host gates must not require the ESP toolchain.
  - If device builds dominate the workflow (rare), add a `rust-toolchain.toml` then, but keep host recipes explicit with `--target` and the `esp`-toolchain recipes explicit with `+esp`.
  - RISC-V bare-metal builds (the C3 half of `check-hal`, `hal_c3_*` / `hal_c6_*` examples) run on stable.
  - The S3 half of `check-hal` needs the `esp` toolchain like any Xtensa build.
  - The RISC-V builds use stable with the prebuilt rustup target, not a floating nightly, which can lag the features the pinned `esp-hal` needs (2026-09-25: a January nightly rejected `esp-sync 0.3.0`'s `cfg_select!`).
- **Cross-compilation runs in CI.**
  - Device jobs install the `esp` toolchain through the shared `setup-xtensa` action (family pin `1.97.0.0`) and the ESP-IDF toolchain through `setup-esp-idf`.
  - Host gates run `--target <host-triple>` (detected via `scripts/host-target.sh`) against the host-buildable crates only.
- **`deny.toml` is required for `just deny` / `just ci` to pass.**
  - Without it, `cargo-deny` defaults to an empty licence allow-list and rejects every dependency — even MIT/Apache-2.0.
  - The allow-list here is shared with the sibling repos and covers the esp dependency trees.
  - `multiple-versions = "warn"` keeps esp release-wave duplicate-version warnings non-fatal.

## Conventions worth not relearning

- **Chip cfg comes from the target triple, per crate.**
  - Each hardware crate's `build.rs` emits `cfg(esp32)` / `cfg(esp32s3)` itself (with `rustc-check-cfg` registration).
  - It must, because build-script cfg flags do not propagate to dependents.
- **esp-idf link glue is per-crate too.**
  - The esp-idf crate's `build.rs` must call `embuild::espidf::sysenv::output()` or examples/tests fail to link with undefined ESP-IDF symbols.
  - Link args from a dependency's build script do not propagate.
  - See `rustyfarian-esp-idf-power/build.rs`.
- **Pin esp stacks exactly.** Pin `esp-hal` / `esp-idf` with `=` in `[workspace.dependencies]`, coordinated with the sibling repos' waves — never caret.
- **Tier conformance guards are compile-time only.**
  - Each tier's `lib.rs` has a private `const _: fn()` guard pinning the `edge_count()` / `skipped_count()` signatures on the sync rotary encoder (idf `Encoder`, hal `InterruptEncoder`).
  - The guard also pins that no local item shadows the glob-re-exported `tamer` `RotaryEncoder` / `NoopRotaryEncoder`.
  - Only the target recipes (`check-hal`, `check-idf`, `check-matrix`) compile them; `just test` and `just check` on the host do not.
