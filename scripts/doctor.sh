#!/usr/bin/env bash
set -euo pipefail
# doctor.sh — report development tooling status for rustyfarian-peripherals
# Usage: scripts/doctor.sh (no arguments)

# status <name> <state> <detail>
status() { printf '  %-16s %-9s %s\n' "$1" "$2" "$3"; }

printf 'rustyfarian-peripherals — tooling status\n\n'

# --- Rust toolchain -------------------------------------------------------
if command -v rustc >/dev/null 2>&1; then
    status "rustc" "ok" "$(rustc --version 2>/dev/null)"
else
    status "rustc" "MISSING" "install Rust via https://rustup.rs"
fi

if command -v cargo >/dev/null 2>&1; then
    status "cargo" "ok" "$(cargo --version 2>/dev/null)"
else
    status "cargo" "MISSING" "install Rust via https://rustup.rs"
fi

if command -v just >/dev/null 2>&1; then
    status "just" "ok" "$(just --version 2>/dev/null)"
else
    status "just" "MISSING" "install just (the task runner running this)"
fi

# --- Quality tooling (optional; CI installs these) ------------------------
if command -v cargo-deny >/dev/null 2>&1; then
    status "cargo-deny" "ok" "$(cargo deny --version 2>/dev/null)"
else
    status "cargo-deny" "optional" "cargo install cargo-deny --locked (for: just deny)"
fi

if command -v cargo-audit >/dev/null 2>&1; then
    status "cargo-audit" "ok" "$(cargo audit --version 2>/dev/null)"
else
    status "cargo-audit" "optional" "cargo install cargo-audit --locked (for: just audit)"
fi

# --- Device toolchain (only needed for the hardware crates) ---------------
if command -v rustup >/dev/null 2>&1 && rustup toolchain list 2>/dev/null | grep -q '^esp'; then
    status "esp toolchain" "ok" "rustup 'esp' channel present (Xtensa Rust fork)"
else
    status "esp toolchain" "optional" "run: espup install (only for device builds)"
fi

if command -v espflash >/dev/null 2>&1; then
    status "espflash" "ok" "$(espflash --version 2>/dev/null | head -n1)"
else
    status "espflash" "optional" "cargo install espflash (only for flashing devices)"
fi

printf '\nHost work (check / test / clippy / deny) needs only rustc, cargo, and just.\n'
printf 'The esp toolchain and espflash are required only for the hardware crates.\n'
