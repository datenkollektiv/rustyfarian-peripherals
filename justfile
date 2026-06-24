# Rustyfarian Peripherals — development tasks
#
# `just` is the canonical interface for this workspace; CI calls these same
# recipes so local and CI behaviour cannot drift (see .github/workflows/).
#
# Host gates cover the pure `tamer` core — no ESP toolchain required. The
# `rustyfarian-esp-{hal,idf}-peripherals` tiers now pull in their HALs, so they
# build only for device targets: use `check-hal` / `check-idf` and the
# `build-example` / `flash` / `run` family (which need `just setup-toolchain` +
# `just setup-cargo-config`).

host_target := `scripts/host-target.sh`

# Host gates target the pure core explicitly, overriding any device default a
# copied .cargo/config.toml would otherwise impose. The esp-hal/esp-idf tiers
# are excluded here — they do not compile for the host.
host_flags := "-p tamer --target " + host_target
tamer_hal_flags := host_flags + " --features hal"
doc_flags  := "-p tamer --target " + host_target + " --no-deps"

# Default device targets for the per-tier check recipes (ESP32-C3, the example
# chip). build-example / flash derive the triple per chip from the example name.
hal_target := "riscv32imc-unknown-none-elf"
idf_target := "riscv32imc-esp-espidf"

# Bare-metal and ESP-IDF builds use SEPARATE target dirs so their artifacts
# (no_std `build-std=core` vs `std`) never collide. They share the optional
# RAM disk at {{ ramdisk }} (across all rustyfarian repos, namespaced per repo);
# without it, builds fall back to target/hal and target/idf. (`cargo` itself,
# e.g. rust-analyzer, uses target/ide per .cargo/config.toml.)
#
# Detection uses `diskutil` (via scripts/ramdisk-mounted.sh), not a directory
# check, so a stale /Volumes/RustBuilds left after a failed detach is correctly
# treated as unmounted. Manage the disk with `just ramdisk attach|detach`.
ramdisk := "/Volumes/RustBuilds"
ramdisk_mounted := shell(justfile_directory() + '/scripts/ramdisk-mounted.sh "' + ramdisk + '"')
hal_dir := if ramdisk_mounted == "true" { ramdisk + "/targets/hal/" + file_name(justfile_directory()) } else { "target/hal" }
idf_dir := if ramdisk_mounted == "true" { ramdisk + "/targets/idf/" + file_name(justfile_directory()) } else { "target/idf" }

# list available recipes (default)
_default:
    @just --list

# --- Build & Check --------------------------------------------------------

# check the host-buildable crates (no ESP toolchain required)
check:
    cargo check {{ host_flags }}

# build the host-buildable crates (no ESP toolchain required)
build:
    cargo build {{ host_flags }}

# check the esp-hal crate for ESP32-C3 (RISC-V); needs nightly for -Zbuild-std, no espup
check-hal:
    rustup target add {{ hal_target }}
    cargo +nightly check -Zbuild-std=core,alloc --target {{ hal_target }} \
        --target-dir {{ hal_dir }} --no-default-features --features esp32c3,unstable \
        -p rustyfarian-esp-hal-peripherals

# check the ESP-IDF (std) crate for the ESP32-C3 device target (requires the ESP toolchain)
check-idf:
    MCU=esp32c3 cargo +esp check -p rustyfarian-esp-idf-peripherals --target {{ idf_target }} \
        --target-dir {{ idf_dir }}

# --- Code Quality ---------------------------------------------------------

# run clippy on the host-buildable crates, denying warnings
clippy:
    cargo clippy {{ host_flags }} -- -D warnings

# run host-side unit tests (no ESP toolchain required)
test:
    cargo test {{ host_flags }}

# run host-side unit tests with tamer's embedded-hal adapters enabled
test-hal:
    cargo test {{ tamer_hal_flags }}

# run host-side tests with stdout/stderr visible
test-verbose:
    cargo test {{ host_flags }} -- --nocapture

# run a single named test
test-one name:
    cargo test {{ host_flags }} {{ name }}

# format all code
fmt:
    cargo fmt

# check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# --- Documentation --------------------------------------------------------

# build rustdoc for the pure core
doc:
    cargo doc {{ doc_flags }}

# build and open docs in the browser
doc-open:
    cargo doc {{ doc_flags }} --open

# --- Maintenance ----------------------------------------------------------

# check dependency licenses, advisories, and bans
deny:
    cargo deny check

# audit dependencies for known security advisories (RUSTSEC)
audit:
    [ -f Cargo.lock ] || cargo generate-lockfile
    cargo audit

# update dependencies
update:
    cargo update

# clean build artifacts (host/ide + the split device target dirs) and scratch
clean:
    cargo clean
    cargo clean --target-dir {{ hal_dir }}
    cargo clean --target-dir {{ idf_dir }}
    rm -rf tmp

# report development tooling status and the resolved build target dirs
doctor:
    @scripts/doctor.sh "{{ ramdisk }}" "{{ hal_dir }}" "{{ idf_dir }}"

# manage the shared build RAM disk: just ramdisk attach | detach
ramdisk action:
    @scripts/ramdisk.sh "{{ action }}"

# --- Composite ------------------------------------------------------------

# full pre-commit verification: format, check, lint, test (modifies files — local use only)
pre-commit: fmt check clippy test

# non-modifying full verification: fails on any anomaly
verify:
    @cargo fmt -- --check || (printf '\nFormatting issues found — run `just pre-commit` to auto-fix.\n' >&2 && exit 1)
    cargo check {{ host_flags }}
    cargo clippy {{ host_flags }} -- -D warnings
    cargo test {{ host_flags }}

# CI-equivalent verification (non-modifying): format check, deny, check, lint, test
ci: fmt-check deny check clippy test

# --- Device (examples) ----------------------------------------------------
#
# Example names follow `{hal|idf}_{chip}_{name}` (chip ∈ c3|c6|esp32|esp32s3);
# the scripts derive the crate, target triple, and required-features from the
# name and build into the tier's split target dir. These require
# `just setup-cargo-config`; ESP-IDF and Xtensa builds also need
# `just setup-toolchain`.

# list available hardware examples
examples:
    #!/usr/bin/env bash
    echo "Available examples (use with: just build-example <name> / just run <name>):"
    for f in crates/*/examples/*.rs; do
        [ -e "$f" ] || continue
        printf '  %-20s (%s)\n' "$(basename "$f" .rs)" "$(echo "$f" | cut -d/ -f2)"
    done

# build a named example; crate/chip/target auto-detected from the name
build-example example:
    scripts/build-example.sh "{{ example }}" "{{ hal_dir }}" "{{ idf_dir }}"

# build and flash a named example to a connected board
flash example:
    scripts/flash.sh "{{ example }}" "{{ hal_dir }}" "{{ idf_dir }}"

# build, flash, then open the serial monitor
run example:
    just flash "{{ example }}"
    espflash monitor

# open the serial monitor for an already-flashed device
monitor:
    espflash monitor

# --- Setup ----------------------------------------------------------------

# copy the cargo config template for first-time device-build setup
setup-cargo-config:
    cp .cargo/config.toml.dist .cargo/config.toml

# install the ESP-IDF / Xtensa toolchain via espup (only needed for device builds)
setup-toolchain:
    espup install
