# Key Insights

This file is the persistent knowledge base for the rustyfarian-peripherals project.
All agents and Claude Code sessions should read this file at the start of relevant work and update it when new insights are discovered.

Organise entries by topic, not chronologically.
Keep each insight concise: one paragraph or a short bullet list per topic.
Remove or update entries that are superseded.

---

## Workspace shape

- **Three crates, one pure core.** `tamer` holds all host-testable input logic; `rustyfarian-esp-hal-peripherals` (bare-metal) and `rustyfarian-esp-idf-peripherals` (std) are thin wrappers that delegate to it. Anything host-testable belongs in `tamer`, never in a hardware wrapper.
- **Skeleton, by design.** No input primitives are implemented yet — they grow downstream-driven. Every crate builds on the host today, so all three are covered by the host CI gates (no ESP toolchain needed). This changes once a hardware crate pulls in `esp-hal` / `esp-idf-hal`: at that point its check moves to a device target (`just check-idf` and a future `check-hal`), and CI keeps covering only the host-testable surface — mirroring the sibling repos.

## CI and Build Validation

- **GitHub Actions workflows live in `.github/workflows/`** with four files: `rust.yml` (CI: deny + check + test), `fmt.yml` (format check), `clippy.yml` (clippy), `audit.yml` (cargo-audit, runs on schedule + push). Each workflow calls a `just` recipe via `extractions/setup-just@v2`, so the justfile is the single source of truth and CI cannot drift from local `just verify` / `just ci`.
- **Keep the four workflow files structurally consistent** (checkout → toolchain → setup-just → cache → recipe). If the boilerplate grows, factor it into a reusable workflow rather than letting them diverge.
- **`RUSTUP_TOOLCHAIN: stable` is set in every workflow.** It keeps CI on stable even after a `rust-toolchain.toml` (channel = `esp`) is added for device work — the `esp` channel is not installed on GitHub-hosted runners. (No `rust-toolchain.toml` exists yet; add one only when device builds dominate, like rustyfarian-power.)
- **No cross-compilation in CI (no `espup`).** Host gates run `--target <host-triple>` (detected via `scripts/host-target.sh`) against the host-buildable crates only. The ESP toolchain is a local-only requirement for the hardware crates.
- **`deny.toml` is required for `just deny` / `just ci` to pass.** Without it, `cargo-deny` defaults to an empty licence allow-list and rejects every dependency — even MIT/Apache-2.0. The allow-list here is shared with the sibling repos and already covers the esp dependency trees, so it does not need revisiting when the hardware deps land. `multiple-versions = "warn"` keeps esp release-wave duplicate-version warnings non-fatal.

## Conventions worth not relearning

- **Chip cfg comes from the target triple, per crate.** Each hardware crate's `build.rs` emits `cfg(esp32)` / `cfg(esp32s3)` itself (with `rustc-check-cfg` registration) because build-script cfg flags do not propagate to dependents.
- **esp-idf link glue is per-crate too.** When `esp-idf-hal` is added, the esp-idf crate's `build.rs` must call `embuild::espidf::sysenv::output()` or examples/tests fail to link with undefined ESP-IDF symbols — link args from a dependency's build script do not propagate. See `rustyfarian-esp-idf-power/build.rs`.
- **Pin esp stacks exactly.** When the hardware deps land, pin `esp-hal` / `esp-idf` with `=` in `[workspace.dependencies]`, coordinated with the sibling repos' waves — never caret.
