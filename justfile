# Rustyfarian Peripherals — development tasks
#
# `just` is the canonical interface for this workspace; CI calls these same
# recipes so local and CI behaviour cannot drift (see .github/workflows/).
#
# While the workspace is a skeleton, every crate is host-buildable, so the host
# gates below cover `tamer` (pure core) and the bare-metal `esp-hal` crate
# directly — no ESP toolchain required. Device recipes (`check-idf`, and the
# `flash` / `run` / `build-example` family) arrive with the first
# downstream-driven driver and example; until then they are intentionally
# absent rather than stubbed.
#
# Run `just setup-cargo-config` before any device build to opt into the
# multi-chip target config in .cargo/config.toml.dist.

host_target := `scripts/host-target.sh`

# Host gates target the host-buildable crates explicitly, overriding any device
# default a copied .cargo/config.toml would otherwise impose.
host_flags := "-p tamer -p rustyfarian-esp-hal-peripherals --target " + host_target
doc_flags  := "-p tamer --target " + host_target + " --no-deps"

# ESP-IDF (std) target for the Adafruit ESP32 Feather class boards; the chip is
# illustrative — adjust when the first idf driver lands.
esp32_target := "xtensa-esp32-espidf"

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

# check the ESP-IDF (std) crate for a device target (requires espup) — no driver yet, kept for when one lands
check-idf:
    cargo check -p rustyfarian-esp-idf-peripherals --target {{ esp32_target }}

# --- Code Quality ---------------------------------------------------------

# run clippy on the host-buildable crates, denying warnings
clippy:
    cargo clippy {{ host_flags }} -- -D warnings

# run host-side unit tests (no ESP toolchain required)
test:
    cargo test {{ host_flags }}

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

# clean build artifacts
clean:
    cargo clean

# report development tooling status
doctor:
    @scripts/doctor.sh

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

# --- Setup ----------------------------------------------------------------

# copy the cargo config template for first-time device-build setup
setup-cargo-config:
    cp .cargo/config.toml.dist .cargo/config.toml

# install the ESP-IDF / Xtensa toolchain via espup (only needed for device builds)
setup-toolchain:
    espup install
