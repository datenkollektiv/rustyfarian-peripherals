#!/usr/bin/env bash
set -euo pipefail
# build-example.sh — build a named example (no flash).
# Usage: scripts/build-example.sh <example> [hal_dir [idf_dir]]
#   example: hal_{chip}_{name} (bare-metal) | idf_{chip}_{name} (ESP-IDF)
#   chip ∈ {c3, c6, esp32, esp32s3}
#
# Chip, crate, target triple, and required-features are derived from the name.
# Bare-metal builds use the host nightly toolchain (RISC-V) or `cargo +esp`
# (Xtensa); ESP-IDF builds use `MCU=<mcu> cargo` (RISC-V) or `cargo +esp`
# (Xtensa). All builds need `just setup-cargo-config` (the linker scripts); the
# ESP-IDF and Xtensa paths additionally need `just setup-toolchain`.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"

if [ $# -lt 1 ]; then
    printf 'Usage: %s <example> [hal_dir [idf_dir]]\n  example: hal_{chip}_{name} | idf_{chip}_{name}\n' "$0" >&2
    exit 2
fi

# Device builds need the per-target config (the bare-metal linker script
# `-Tlinkall.x`, the ESP-IDF linker/target, etc.). Without it, the build reaches
# the link step and fails with cryptic "undefined symbol: _stack_start" errors.
if [ ! -f "$REPO_ROOT/.cargo/config.toml" ]; then
    printf 'error: device builds need .cargo/config.toml (linker scripts + target).\n       run: just setup-cargo-config\n' >&2
    exit 1
fi

example="$1"
hal_dir="${2:-target/hal}"
idf_dir="${3:-target/idf}"

resolve_example "$example" "$hal_dir" "$idf_dir"

cargo_bin=(cargo)
if [ "$EX_XTENSA" = 1 ]; then
    # shellcheck source=./xtensa-toolchain.sh
    . "$SCRIPT_DIR/xtensa-toolchain.sh"
    setup_xtensa_toolchain
    cargo_bin=(cargo +esp)
fi

if [ "$EX_TIER" = hal ]; then
    # RISC-V bare-metal builds on plain nightly (no espup); Xtensa needs +esp.
    [ "$EX_XTENSA" = 1 ] || cargo_bin=(cargo +nightly)
    printf 'Building %s for bare-metal %s (features=%s)...\n' "$example" "$EX_TARGET" "$EX_FEATURES"
    "${cargo_bin[@]}" build --release -Zbuild-std=core,alloc \
        --target "$EX_TARGET" --target-dir "$EX_TARGET_DIR" \
        --no-default-features --features "$EX_FEATURES" \
        --example "$example" -p "$EX_PKG"
else
    # ESP-IDF std targets (RISC-V `*-esp-espidf` and Xtensa alike) ship only with
    # the `esp` toolchain, so always build with `cargo +esp` — not the host
    # default (which lacks the espidf std target: "can't find crate for `core`").
    # Xtensa additionally needs the LLVM env from setup_xtensa_toolchain above.
    cargo_bin=(cargo +esp)
    feat_args=()
    [ -n "$EX_FEATURES" ] && feat_args=(--features "$EX_FEATURES")
    printf 'Building %s for %s (MCU=%s)...\n' "$example" "$EX_TARGET" "$EX_MCU"
    MCU="$EX_MCU" "${cargo_bin[@]}" build --release \
        --target "$EX_TARGET" --target-dir "$EX_TARGET_DIR" \
        "${feat_args[@]}" --example "$example" -p "$EX_PKG"
fi
