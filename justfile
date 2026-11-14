# Rustyfarian Peripherals — development tasks
#
# `just` is the canonical interface for this workspace; CI calls these same
# recipes so local and CI behaviour cannot drift (see .github/workflows/).
#
# Host gates cover the pure `tamer` core — no ESP toolchain required. The
# `rustyfarian-esp-{hal,idf}-peripherals` tiers now pull in their HALs, so they
# build only for device targets: use `check-hal` / `check-idf` and the
# `build-example` / `flash` / `run` family (which need `just setup esp`;
# the device target config is the committed .cargo/config.toml).

host_target := `scripts/host-target.sh`

# Host gates target the pure core explicitly, overriding the device default
# .cargo/config.toml would otherwise impose. The esp-hal/esp-idf tiers
# are excluded here — they do not compile for the host.
host_flags := "-p tamer --target " + host_target
tamer_hal_flags := host_flags + " --features hal"
tamer_all_features_flags := host_flags + " --all-features"
doc_flags  := "-p tamer --target " + host_target + " --no-deps"
# signwriter (host-buildable, graphics-bearing) gates run beside tamer's but are kept out of
# `host_flags`: its dev-dependency on tamer's `hal` feature would otherwise unify into tamer's own
# tests and hide a missing `hal` gate.
signwriter_flags := "-p signwriter --target " + host_target
signwriter_doc_flags := signwriter_flags + " --no-deps"

# Default device targets for the per-tier check recipes (ESP32-C3, the example
# chip). build-example / flash derive the triple per chip from the example name.
hal_target := "riscv32imc-unknown-none-elf"
idf_target := "riscv32imc-esp-espidf"

# Pinned AVR nightly for the avr-none recipes; matches the AVR example's
# rust-toolchain.toml; `just setup avr` installs it (CI uses the rustyfarian-ci setup-avr action).
avr_nightly := "nightly-2025-04-27"

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
avr_dir := if ramdisk_mounted == "true" { ramdisk + "/targets/avr/" + file_name(justfile_directory()) } else { "target/avr" }
# Absolute form for recipes that cd into examples/avr-nano-tamer (a relative target dir would
# resolve under the example, not the repo root).
avr_abs_dir := if ramdisk_mounted == "true" { ramdisk + "/targets/avr/" + file_name(justfile_directory()) } else { justfile_directory() + "/target/avr" }

# Guided bench runner: a local rustyfarian-bench checkout if RUSTYFARIAN_BENCH points at one,
# otherwise the pinned release from GitHub.
bench_dir := env("RUSTYFARIAN_BENCH", "")
bench := if bench_dir != "" { 'uv run --project "' + bench_dir + '" rustyfarian-bench' } else { "uvx --from git+https://github.com/datenkollektiv/rustyfarian-bench@v0.5.0 rustyfarian-bench" }

# act: a local rustyfarian-ci checkout if RUSTYFARIAN_CI points at one, otherwise the pushed @v1.
ci_dir := env("RUSTYFARIAN_CI", "")
ci_repo_flag := if ci_dir != "" { "--local-repository " + quote("datenkollektiv/rustyfarian-ci@v1=" + absolute_path(ci_dir)) } else { "" }

# list available recipes (default)
_default:
    @just --list

# --- Build & Check --------------------------------------------------------

# check the host-buildable crates (no ESP toolchain required)
[group('build')]
check:
    cargo check {{ host_flags }}
    cargo check {{ signwriter_flags }}

# build the host-buildable crates (no ESP toolchain required)
[group('build')]
build:
    cargo build {{ host_flags }}

# check the esp-hal crate for ESP32-C3 (stable) and ESP32-S3 (Xtensa; requires the ESP toolchain)
[group('build')]
check-hal:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "==> check-hal: ESP32-C3 ({{ hal_target }}, stable)"
    rustup target add {{ hal_target }}
    cargo check --target {{ hal_target }} \
        --target-dir "{{ hal_dir }}" --no-default-features --features esp32c3,unstable \
        -p rustyfarian-esp-hal-peripherals
    echo "==> check-hal: ESP32-S3 (xtensa-esp32s3-none-elf, esp toolchain)"
    # shellcheck source=./scripts/xtensa-toolchain.sh
    . "{{ justfile_directory() }}/scripts/xtensa-toolchain.sh"
    setup_xtensa_toolchain
    cargo +esp check -Zbuild-std=core,alloc --target xtensa-esp32s3-none-elf \
        --target-dir "{{ hal_dir }}" --no-default-features --features esp32s3,unstable \
        -p rustyfarian-esp-hal-peripherals

# lint the esp-hal crate (lib + examples) for ESP32-C3 and ESP32-S3, denying warnings
[group('build')]
clippy-hal:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "==> clippy-hal: ESP32-C3 ({{ hal_target }}, stable)"
    rustup target add {{ hal_target }}
    cargo clippy --target {{ hal_target }} \
        --target-dir "{{ hal_dir }}" --no-default-features --features esp32c3,unstable \
        -p rustyfarian-esp-hal-peripherals -- -D warnings
    echo "==> clippy-hal: ESP32-S3 (xtensa-esp32s3-none-elf, esp toolchain)"
    # shellcheck source=./scripts/xtensa-toolchain.sh
    . "{{ justfile_directory() }}/scripts/xtensa-toolchain.sh"
    setup_xtensa_toolchain
    cargo +esp clippy -Zbuild-std=core,alloc --target xtensa-esp32s3-none-elf \
        --target-dir "{{ hal_dir }}" --no-default-features --features esp32s3,unstable,rt,embassy \
        -p rustyfarian-esp-hal-peripherals --lib --examples -- -D warnings

# build the esp-hal crate's rustdoc for ESP32-C3 (the docs.rs target), denying warnings
[group('build')]
doc-hal:
    rustup target add {{ hal_target }}
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --target {{ hal_target }} \
        --target-dir "{{ hal_dir }}" --no-default-features --features esp32c3,unstable \
        -p rustyfarian-esp-hal-peripherals

# check the ESP-IDF (std) crate for the ESP32-C3 device target (requires the ESP toolchain)
[group('build')]
check-idf:
    MCU=esp32c3 cargo +esp check -p rustyfarian-esp-idf-peripherals --target {{ idf_target }} \
        --target-dir {{ idf_dir }}

# --- Chip matrix ----------------------------------------------------------
#
# Every tier checks and lints on all four ESP32 chips (docs/features/chip-matrix-v1.md).
# Kept out of `verify`: the four ESP-IDF builds are slow. Chips run one at a time, and
# each chip is also a recipe of its own (the CI jobs call the per-chip form).

# check and clippy both tier libraries on all four ESP32 chips (slow; requires espup)
[group('matrix')]
check-matrix: check-matrix-hal check-matrix-idf

# check and clippy the esp-hal library on all four ESP32 chips, one at a time (requires espup)
[group('matrix')]
check-matrix-hal:
    #!/usr/bin/env bash
    set -euo pipefail
    for chip in esp32 esp32c3 esp32c6 esp32s3; do
        scripts/ramdisk-guard.sh "{{ ramdisk }}" "{{ ramdisk_mounted }}" "$chip"
        scripts/check-matrix.sh hal "$chip" "{{ hal_dir }}" "{{ idf_dir }}"
    done

# check and clippy the ESP-IDF library on all four ESP32 chips, one at a time (requires espup)
[group('matrix')]
check-matrix-idf:
    #!/usr/bin/env bash
    set -euo pipefail
    for chip in esp32 esp32c3 esp32c6 esp32s3; do
        scripts/ramdisk-guard.sh "{{ ramdisk }}" "{{ ramdisk_mounted }}" "$chip"
        scripts/check-matrix.sh idf "$chip" "{{ hal_dir }}" "{{ idf_dir }}"
    done

# build and link the b3f smoke example for every chip on both tiers (the matrix's link proof)
[group('matrix')]
build-matrix: build-matrix-hal build-matrix-idf

# build and link hal_{esp32,c3,c6,s3}_b3f and the hal_{esp32,c3,c6}_oled examples (esp32 for both boards), one chip at a time
[group('matrix')]
build-matrix-hal:
    #!/usr/bin/env bash
    set -euo pipefail
    for chip in esp32 c3 c6 s3; do
        scripts/ramdisk-guard.sh "{{ ramdisk }}" "{{ ramdisk_mounted }}" "hal_${chip}_b3f"
        scripts/build-example.sh "hal_${chip}_b3f" "{{ hal_dir }}" "{{ idf_dir }}"
    done
    for entry in esp32: c3: c6: esp32:feather-v2; do
        chip="${entry%%:*}"
        board="${entry#*:}"
        scripts/ramdisk-guard.sh "{{ ramdisk }}" "{{ ramdisk_mounted }}" "hal_${chip}_oled ${board}"
        scripts/build-example.sh "hal_${chip}_oled" "{{ hal_dir }}" "{{ idf_dir }}" "${board}"
    done

# build and link idf_{esp32,c3,c6,s3}_b3f and the idf_{esp32,c3,c6}_oled examples (esp32 for both boards), one chip at a time (requires espup)
[group('matrix')]
build-matrix-idf:
    #!/usr/bin/env bash
    set -euo pipefail
    for chip in esp32 c3 c6 s3; do
        scripts/ramdisk-guard.sh "{{ ramdisk }}" "{{ ramdisk_mounted }}" "idf_${chip}_b3f"
        scripts/build-example.sh "idf_${chip}_b3f" "{{ hal_dir }}" "{{ idf_dir }}"
    done
    for entry in esp32: c3: c6: esp32:feather-v2; do
        chip="${entry%%:*}"
        board="${entry#*:}"
        scripts/ramdisk-guard.sh "{{ ramdisk }}" "{{ ramdisk_mounted }}" "idf_${chip}_oled ${board}"
        scripts/build-example.sh "idf_${chip}_oled" "{{ hal_dir }}" "{{ idf_dir }}" "${board}"
    done

# check and clippy the esp-hal library for one chip: esp32 | esp32c3 | esp32c6 | esp32s3
[group('matrix')]
check-matrix-hal-chip chip:
    scripts/check-matrix.sh hal "{{ chip }}" "{{ hal_dir }}" "{{ idf_dir }}"

# check and clippy the ESP-IDF library for one chip: esp32 | esp32c3 | esp32c6 | esp32s3
[group('matrix')]
check-matrix-idf-chip chip:
    scripts/check-matrix.sh idf "{{ chip }}" "{{ hal_dir }}" "{{ idf_dir }}"

# --- AVR (tamer on avr-none) ----------------------------------------------
#
# tamer links on the 8-bit AVR (ATmega328P, Arduino Nano). There is no AVR tier crate
# (see docs/features/chip-matrix-v1.md); the workspace-excluded example package is what
# reaches the linker. Needs `just setup avr` and avr-gcc: avr-gcc is required even for a
# core-only build, because the AVR 32-bit multiply intrinsics come from libgcc.

# check tamer (default + hal) on the real avr-none target (requires: just setup avr, avr-gcc)
[group('avr')]
check-avr-target:
    RUSTFLAGS="-C target-cpu=atmega328p" cargo +{{ avr_nightly }} check -p tamer --features hal --target avr-none -Z build-std=core --target-dir "{{ avr_dir }}"

# build every binary in the AVR tamer example, linking the avr-none ELF (requires: just setup avr, avr-gcc)
[group('avr')]
build-avr-example:
    cd examples/avr-nano-tamer && cargo +{{ avr_nightly }} build --release -Z build-std=core --bins --locked --target-dir "{{ avr_abs_dir }}"

# build the AVR example against upstream avr-hal main: the weekly early-warning check (throwaway copy)
[group('avr')]
build-avr-example-upstream:
    scripts/build-avr-example-upstream.sh "{{ justfile_directory() }}" "{{ avr_nightly }}" "{{ avr_abs_dir }}"

# --- Code Quality ---------------------------------------------------------

# print tamer's dependency tree for the given feature set (verifies feature gating)
[group('quality')]
tree *features:
    cargo tree -p tamer --target {{ host_target }} {{ features }}

# run clippy on the host-buildable crates, denying warnings
[group('quality')]
clippy:
    cargo clippy {{ host_flags }} -- -D warnings
    cargo clippy {{ signwriter_flags }} --all-targets -- -D warnings

# run clippy with every tamer feature enabled (hal + tilt), denying warnings
[group('quality')]
clippy-all-features:
    cargo clippy {{ tamer_all_features_flags }} -- -D warnings

# run host-side unit tests (no ESP toolchain required)
[group('quality')]
test:
    cargo test {{ host_flags }}
    cargo test {{ signwriter_flags }}

# run host-side unit tests with tamer's embedded-hal adapters enabled
[group('quality')]
test-hal:
    cargo test {{ tamer_hal_flags }}

# `--all-features` never builds the `bme680`-without-`hal` combination.
[group('quality')]
[doc('run host-side unit tests for the BME680 module alone and with its I2C driver')]
test-bme680:
    cargo test {{ host_flags }} --features bme680
    cargo test {{ host_flags }} --features bme680,hal

# run host-side unit tests with every tamer feature enabled (hal + tilt + bme680)
[group('quality')]
test-all-features:
    cargo test {{ tamer_all_features_flags }}

# run host-side tests with stdout/stderr visible
[group('quality')]
test-verbose:
    cargo test {{ host_flags }} -- --nocapture

# run a single named test
[group('quality')]
test-one name:
    cargo test {{ host_flags }} {{ name }}

# format all code
[group('quality')]
fmt:
    cargo fmt

# check formatting without modifying files
[group('quality')]
fmt-check:
    cargo fmt -- --check

# --- Documentation --------------------------------------------------------

# build rustdoc for the pure core
[group('docs')]
doc:
    cargo doc {{ doc_flags }}
    cargo doc {{ signwriter_doc_flags }}

# build rustdoc with every tamer feature enabled (matches docs.rs' all-features build)
[group('docs')]
doc-all-features:
    cargo doc {{ doc_flags }} --all-features

# build and open docs in the browser
[group('docs')]
doc-open:
    cargo doc {{ doc_flags }} --open

# --- Maintenance ----------------------------------------------------------

# check dependency licenses, advisories, and bans
[group('maintenance')]
deny:
    cargo deny check

# audit dependencies for known security advisories (RUSTSEC)
[group('maintenance')]
audit:
    [ -f Cargo.lock ] || cargo generate-lockfile
    cargo audit

# update dependencies
[group('maintenance')]
update:
    cargo update

# clean build artifacts (host/ide + the split device target dirs) and scratch
[group('maintenance')]
clean:
    cargo clean
    cargo clean --target-dir {{ hal_dir }}
    cargo clean --target-dir {{ idf_dir }}
    cargo clean --target-dir "{{ avr_abs_dir }}"
    rm -rf tmp

# clean only the bare-metal (esp-hal) tier's target dir; keeps the ESP-IDF cache
[group('maintenance')]
clean-hal:
    cargo clean --target-dir {{ hal_dir }}

# clean only the ESP-IDF tier's target dir (forces the esp-idf-sys rebuild); keeps the hal artifacts
[group('maintenance')]
clean-idf:
    cargo clean --target-dir {{ idf_dir }}

# report development tooling status and the resolved build target dirs
[group('maintenance')]
doctor:
    @scripts/doctor.sh "{{ ramdisk }}" "{{ hal_dir }}" "{{ idf_dir }}" "{{ avr_dir }}"

# manage the shared build RAM disk: just ramdisk attach | detach | prune [days]
[group('maintenance')]
ramdisk action *args:
    @scripts/ramdisk.sh "{{ action }}" {{ args }}

# --- Release --------------------------------------------------------------
#
# Staged per `release-plan.md`: publish `tamer` first; the tiers can only be
# packaged once `tamer` of the same version is live on crates.io.

# check tamer and signwriter against their latest crates.io release for semver breaks (network)
[group('release')]
semver:
    cargo semver-checks check-release -p tamer -p signwriter --target {{ host_target }}

# dry-run packaging of tamer (runs verify first); pass --allow-dirty before committing
[group('release')]
release-dry-run *flags: verify
    cargo publish --dry-run -p tamer --target {{ host_target }} {{ flags }}

# dry-run packaging of signwriter (host-buildable; runs no verify, pass --allow-dirty before committing)
[group('release')]
release-dry-run-signwriter *flags:
    cargo publish --dry-run -p signwriter --target {{ host_target }} {{ flags }}

# dry-run packaging of the esp-hal tier (needs tamer of this version on crates.io)
[group('release')]
release-dry-run-hal:
    rustup target add {{ hal_target }}
    cargo publish --dry-run -p rustyfarian-esp-hal-peripherals --target {{ hal_target }} \
        --target-dir {{ hal_dir }}

# dry-run packaging of the esp-idf tier (needs tamer on crates.io and the ESP toolchain)
[group('release')]
release-dry-run-idf:
    MCU=esp32c3 cargo +esp publish --dry-run -p rustyfarian-esp-idf-peripherals --target {{ idf_target }} \
        --target-dir {{ idf_dir }}

# publish tamer to crates.io
[group('release')]
[confirm]
release-publish:
    cargo publish -p tamer --target {{ host_target }}

# publish signwriter to crates.io
[group('release')]
[confirm]
release-publish-signwriter:
    cargo publish -p signwriter --target {{ host_target }}

# publish the esp-hal tier to crates.io
[group('release')]
[confirm]
release-publish-hal:
    rustup target add {{ hal_target }}
    cargo publish -p rustyfarian-esp-hal-peripherals --target {{ hal_target }} \
        --target-dir {{ hal_dir }}

# publish the esp-idf tier to crates.io (requires the ESP toolchain)
[group('release')]
[confirm]
release-publish-idf:
    MCU=esp32c3 cargo +esp publish -p rustyfarian-esp-idf-peripherals --target {{ idf_target }} \
        --target-dir {{ idf_dir }}

# --- Composite ------------------------------------------------------------

# format, then run verify (modifies files — local use only)
[group('gates')]
pre-commit: fmt verify

# non-modifying, offline host verification: format check, check, lint, test
[group('gates')]
verify: fmt-check check clippy test

# CI-equivalent host gate: verify + deny (device jobs: `just ci-riscv`, `ci-xtensa`, `ci-idf <chip>`, `ci-avr`, `ci-avr-upstream`)
[group('gates')]
ci: verify deny

# --- CI jobs --------------------------------------------------------------
#
# Each GitHub Actions job runs exactly one of these (rustyfarian-ci CONVENTIONS.md). Host jobs run
# through rustyfarian-ci's host-checks.yml; device jobs run here too, so `just ci-<job>` reproduces a job.

# CI job: check formatting
[group('ci')]
ci-fmt: fmt-check

# CI job: lint the host-buildable crates
[group('ci')]
ci-clippy: clippy

# CI job: type-check and test the host-buildable crates
[group('ci')]
ci-test: check test

# CI job: licences, advisories, bans and sources
[group('ci')]
ci-deny: deny

# CI job: RUSTSEC advisories
[group('ci')]
ci-audit: audit

# CI job: esp-hal tier on ESP32-C3 and ESP32-C6 (stable): check + clippy, then link the b3f examples
[group('ci')]
ci-riscv: (check-matrix-hal-chip "esp32c3") (check-matrix-hal-chip "esp32c6") (build-example "hal_c3_b3f") (build-example "hal_c6_b3f")

# CI job: esp-hal tier on ESP32 and ESP32-S3 (esp toolchain): check + clippy, then link the b3f examples
[group('ci')]
ci-xtensa: (check-matrix-hal-chip "esp32") (check-matrix-hal-chip "esp32s3") (build-example "hal_esp32_b3f") (build-example "hal_s3_b3f")

# CI job: ESP-IDF tier for one chip (esp32 | c3 | c6 | s3): check + clippy, then link idf_<chip>_b3f
[group('ci')]
ci-idf chip:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ chip }}" in
        esp32) mcu=esp32 ;;
        c3|c6|s3) mcu="esp32{{ chip }}" ;;
        *) printf 'error: chip must be esp32 | c3 | c6 | s3 (got "%s")\n' "{{ chip }}" >&2; exit 2 ;;
    esac
    scripts/check-matrix.sh idf "$mcu" "{{ hal_dir }}" "{{ idf_dir }}"
    scripts/build-example.sh "idf_{{ chip }}_b3f" "{{ hal_dir }}" "{{ idf_dir }}"

# CI job: tamer on avr-none and the AVR example link (needs `just setup avr` and avr-gcc)
[group('ci')]
ci-avr: check-avr-target build-avr-example

# CI job: AVR example against upstream avr-hal main (weekly canary)
[group('ci')]
ci-avr-upstream: build-avr-example-upstream

# --- act (local GitHub Actions) -------------------------------------------
#
# Runs jobs in Docker through the same workflows CI runs. Needs Docker and act. The event is
# `workflow_dispatch` because every workflow has it; `push` would skip avr-upstream.

# run the host jobs (no args) or one job: `just act riscv`; one matrix cell: `just act idf:chip:c3`; `just act list` lists the jobs
[group('act')]
act job="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "{{ job }}" = "list" ]; then
        exec act --list {{ ci_repo_flag }}
    fi
    for spec in {{ if job == "" { "fmt clippy test deny audit" } else { job } }}; do
        j="${spec%%:*}"
        if [ "$spec" = "$j" ]; then set --; else set -- --matrix "${spec#*:}"; fi
        act workflow_dispatch -j "$j" --pull=false --concurrent-jobs 1 \
            -P ubuntu-latest=catthehacker/ubuntu:act-latest {{ ci_repo_flag }} "$@"
    done

# --- Device (examples) ----------------------------------------------------
#
# Example names follow `{hal|idf}_{chip}_{name}` (chip ∈ c3|c6|esp32|esp32s3);
# the scripts derive the crate, target triple, and required-features from the
# name and build into the tier's split target dir. ESP-IDF and Xtensa builds
# need `just setup esp`.

# list available hardware examples
[group('device')]
examples:
    #!/usr/bin/env bash
    echo "Available examples (use with: just build-example <name> / just run <name>):"
    for f in crates/*/examples/*.rs; do
        [ -e "$f" ] || continue
        printf '  %-20s (%s)\n' "$(basename "$f" .rs)" "$(echo "$f" | cut -d/ -f2)"
    done

# build a named example; crate/chip/target auto-detected from the name; optional board profile as second argument (e.g. `just build-example hal_esp32_oled feather-v2`)
[group('device')]
build-example example board="":
    scripts/build-example.sh "{{ example }}" "{{ hal_dir }}" "{{ idf_dir }}" "{{ board }}"

# build and flash a named example to a connected board (optional explicit port and board profile, ADR-014)
[group('device')]
flash example port="" board="":
    scripts/flash.sh "{{ example }}" "{{ hal_dir }}" "{{ idf_dir }}" "{{ port }}" "{{ board }}"

# build, flash, then open the serial monitor (optional board profile, ADR-014)
[group('device')]
run example board="":
    just flash "{{ example }}" "" "{{ board }}"
    espflash monitor

# open the serial monitor for an already-flashed device
[group('device')]
monitor:
    espflash monitor

# reset the board and record its serial output to a file (non-interactive)
[group('device')]
capture seconds logfile port="":
    scripts/capture.sh "{{ seconds }}" "{{ logfile }}" "{{ port }}"

# guided hardware check: `just bench s3 b3f`, `just bench hal_s3_b3f`, `just bench s3` (see bench/README.md)
[group('device')]
bench target *args:
    {{ bench }} --repo . run "{{ target }}" {{ args }}

# list the bench scenarios
[group('device')]
bench-list:
    {{ bench }} --repo . list

# probe the connected boards (chip and MAC per port)
[group('device')]
bench-detect:
    {{ bench }} detect

# --- Setup ----------------------------------------------------------------

# install what a fresh clone is missing, skipping what is present: all (no args), or one of esp | avr | tools
[group('setup')]
setup component="":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ component }}" in
        ""|esp|avr|tools) ;;
        *) printf 'error: component must be esp | avr | tools (got "%s")\n' "{{ component }}" >&2; exit 2 ;;
    esac
    want() { [ -z "{{ component }}" ] || [ "{{ component }}" = "$1" ]; }
    if want esp; then
        if rustup toolchain list | grep '^esp' >/dev/null; then
            echo "==> setup esp: esp toolchain present (espup update refreshes it)"
        else
            command -v espup >/dev/null || cargo install --locked espup
            espup install
        fi
    fi
    if want avr; then
        echo "==> setup avr: {{ avr_nightly }} with rust-src (avr-gcc separately: brew install avr-gcc / apt install gcc-avr)"
        rustup toolchain install {{ avr_nightly }} --profile minimal --component rust-src
    fi
    if want tools; then
        for tool in cargo-deny cargo-audit cargo-semver-checks espflash ldproxy; do
            if command -v "$tool" >/dev/null; then
                echo "==> setup tools: $tool present"
            else
                cargo install --locked "$tool"
            fi
        done
        command -v uv >/dev/null || echo "==> setup tools: uv missing (needed by just bench): https://docs.astral.sh/uv/"
    fi
