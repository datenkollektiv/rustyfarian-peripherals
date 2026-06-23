#!/usr/bin/env bash
set -euo pipefail
# flash.sh — build and flash a named example to a connected board.
# Usage: scripts/flash.sh <example> [hal_dir [idf_dir]]
#   example: hal_{chip}_{name} | idf_{chip}_{name}, chip ∈ {c3, c6, esp32, esp32s3}
#
# Delegates the build to build-example.sh, then flashes the resulting image with
# espflash. Requires a connected board and the ESP toolchain (see build-example.sh).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"

if [ $# -lt 1 ]; then
    printf 'Usage: %s <example> [hal_dir [idf_dir]]\n  example: hal_{chip}_{name} | idf_{chip}_{name}\n' "$0" >&2
    exit 2
fi

example="$1"
hal_dir="${2:-target/hal}"
idf_dir="${3:-target/idf}"

resolve_example "$example" "$hal_dir" "$idf_dir"
"$SCRIPT_DIR/build-example.sh" "$example" "$hal_dir" "$idf_dir"

artifact="$EX_TARGET_DIR/$EX_TARGET/release/examples/$example"
printf 'Flashing %s...\n' "$artifact"
espflash flash --ignore-app-descriptor "$artifact"
