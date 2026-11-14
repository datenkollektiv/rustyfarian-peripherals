#!/usr/bin/env bash
set -euo pipefail
# build-example.sh — build a named example (no flash).
# Usage: scripts/build-example.sh <example> [hal_dir [idf_dir [board]]]
#   example: hal_{chip}_{name} (bare-metal) | idf_{chip}_{name} (ESP-IDF)
#   chip ∈ {c3, c6, esp32, s3} (long forms esp32c3, esp32c6, esp32s3 also accepted)
#   board: optional profile (c3-supermini | c6-devkitc | esp32-wroom | feather-v2);
#          default per chip, see resolve_example in lib.sh (ADR-014)
#
# Chip, crate, target triple, and required-features are derived from the name.
# Bare-metal builds use the stable toolchain with the rustup target (RISC-V) or
# `cargo +esp -Zbuild-std=core,alloc` (Xtensa, no prebuilt core); ESP-IDF builds
# always use `cargo +esp`. The linker scripts come from the committed
# .cargo/config.toml; the ESP-IDF and Xtensa paths need `just setup esp`.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"

if [ $# -lt 1 ]; then
    printf 'Usage: %s <example> [hal_dir [idf_dir [board]]]\n  example: hal_{chip}_{name} | idf_{chip}_{name}\n' "$0" >&2
    exit 2
fi

example="$1"
hal_dir="${2:-target/hal}"
idf_dir="${3:-target/idf}"
board="${4:-}"

resolve_example "$example" "$hal_dir" "$idf_dir" "$board"

cargo_bin=(cargo)
if [ "$EX_XTENSA" = 1 ]; then
    # shellcheck source=./xtensa-toolchain.sh
    . "$SCRIPT_DIR/xtensa-toolchain.sh"
    setup_xtensa_toolchain
    cargo_bin=(cargo +esp)
fi

if [ "$EX_TIER" = hal ]; then
    # RISC-V bare-metal builds on stable with the prebuilt rustup target (no
    # nightly, no espup) — a floating nightly older than a stabilisation date
    # cannot compile the pinned esp-hal stack (see docs/project-lore.md). Xtensa
    # has no prebuilt core, so it needs `+esp` and `-Zbuild-std`.
    # `${arr[@]+"${arr[@]}"}` expands an empty array to nothing under `set -u`
    # on bash 3.2 (/bin/bash on macOS), where a bare "${arr[@]}" is an error.
    std_args=()
    [ "$EX_XTENSA" = 1 ] && std_args=(-Zbuild-std=core,alloc)
    printf 'Building %s for bare-metal %s (features=%s)...\n' "$example" "$EX_TARGET" "$EX_FEATURES"
    "${cargo_bin[@]}" build --release ${std_args[@]+"${std_args[@]}"} \
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
        ${feat_args[@]+"${feat_args[@]}"} --example "$example" -p "$EX_PKG"
fi
