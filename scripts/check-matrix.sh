#!/usr/bin/env bash
set -euo pipefail
# check-matrix.sh — check and clippy one tier's library for one ESP32 chip.
# Usage: scripts/check-matrix.sh <hal|idf> <chip> [hal_dir [idf_dir]]
#   chip ∈ {esp32, esp32c3, esp32c6, esp32s3}
#
# The chip-matrix gate (docs/features/chip-matrix-v1.md): every tier must compile and
# lint on all four ESP32 chips. Target triple and MCU come from scripts/lib.sh
# (resolve_example), the same mapping build-example.sh uses, so the matrix cannot drift
# from the example builds. Only the library is checked here; examples have their own gates.
#
# hal  — bare-metal esp-hal tier: stable + rustup target for RISC-V (C3, C6);
#        `cargo +esp -Zbuild-std=core,alloc` for Xtensa (ESP32, S3).
# idf  — ESP-IDF tier: always `cargo +esp`, with MCU set; needs `just setup-cargo-config`
#        (ESP_IDF_VERSION and the ldproxy linker live in .cargo/config.toml).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"

if [ $# -lt 2 ]; then
    printf 'Usage: %s <hal|idf> <chip> [hal_dir [idf_dir]]\n  chip: esp32 | esp32c3 | esp32c6 | esp32s3\n' "$0" >&2
    exit 2
fi

tier="$1"
chip="$2"
hal_dir="${3:-$REPO_ROOT/target/hal}"
idf_dir="${4:-$REPO_ROOT/target/idf}"

case "$tier" in
    hal|idf) ;;
    *)
        printf 'error: tier must be hal or idf (got "%s").\n' "$tier" >&2
        exit 2
        ;;
esac
case "$chip" in
    esp32|esp32c3|esp32c6|esp32s3) ;;
    *)
        printf 'error: chip must be esp32 | esp32c3 | esp32c6 | esp32s3 (got "%s").\n' "$chip" >&2
        exit 2
        ;;
esac

if [ "$tier" = idf ] && [ ! -f "$REPO_ROOT/.cargo/config.toml" ]; then
    printf 'error: ESP-IDF builds need .cargo/config.toml (ESP_IDF_VERSION + ldproxy).\n       run: just setup-cargo-config\n' >&2
    exit 1
fi

resolve_example "${tier}_${chip}" "$hal_dir" "$idf_dir"

cargo_bin=(cargo)
if [ "$EX_XTENSA" = 1 ]; then
    # shellcheck source=./xtensa-toolchain.sh
    . "$SCRIPT_DIR/xtensa-toolchain.sh"
    setup_xtensa_toolchain
fi
# ESP-IDF std targets (RISC-V `*-esp-espidf` too) are only buildable with the `esp`
# toolchain, as build-example.sh and check-idf do; stable cannot find `core` for them.
if [ "$tier" = idf ] || [ "$EX_XTENSA" = 1 ]; then
    cargo_bin=(cargo +esp)
fi

if [ "$tier" = hal ]; then
    # `build-std` only for Xtensa, which has no prebuilt core; RISC-V uses stable with the
    # prebuilt rustup target. Passed on the command line: the root config's build-std array
    # would merge with, not be replaced by, a child setting (see docs/project-lore.md).
    std_args=()
    [ "$EX_XTENSA" = 1 ] && std_args=(-Zbuild-std=core,alloc)
    if [ "$EX_XTENSA" = 0 ]; then
        rustup target add "$EX_TARGET"
    fi
    # esp-hal lib feature set mirrors `just check-hal`: the chip feature plus `unstable`.
    feat="${EX_MCU},unstable"

    printf '==> check-matrix: hal %s (%s, features=%s) check\n' "$chip" "$EX_TARGET" "$feat"
    "${cargo_bin[@]}" check --lib ${std_args[@]+"${std_args[@]}"} \
        --target "$EX_TARGET" --target-dir "$EX_TARGET_DIR" \
        --no-default-features --features "$feat" -p "$EX_PKG"

    printf '==> check-matrix: hal %s (%s) clippy -D warnings\n' "$chip" "$EX_TARGET"
    "${cargo_bin[@]}" clippy --lib ${std_args[@]+"${std_args[@]}"} \
        --target "$EX_TARGET" --target-dir "$EX_TARGET_DIR" \
        --no-default-features --features "$feat" -p "$EX_PKG" -- -D warnings
else
    # ESP-IDF std targets ship only with the `esp` toolchain; the chip is selected via MCU.
    printf '==> check-matrix: idf %s (%s, MCU=%s) check\n' "$chip" "$EX_TARGET" "$EX_MCU"
    MCU="$EX_MCU" "${cargo_bin[@]}" check --lib \
        --target "$EX_TARGET" --target-dir "$EX_TARGET_DIR" -p "$EX_PKG"

    printf '==> check-matrix: idf %s (%s) clippy -D warnings\n' "$chip" "$EX_TARGET"
    MCU="$EX_MCU" "${cargo_bin[@]}" clippy --lib \
        --target "$EX_TARGET" --target-dir "$EX_TARGET_DIR" -p "$EX_PKG" -- -D warnings
fi
