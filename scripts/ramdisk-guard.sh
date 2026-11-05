#!/usr/bin/env bash
set -euo pipefail
# ramdisk-guard.sh — stop a matrix loop before a build can fill the shared RAM disk.
# Usage: scripts/ramdisk-guard.sh <ramdisk_path> <ramdisk_mounted> <label>
#   ramdisk_mounted: "true" when the RAM disk is attached (the justfile's
#   `ramdisk_mounted` value); anything else makes this a no-op.
#
# Never prunes: `just ramdisk prune` deletes other repos' idle trees on the shared
# disk, so freeing space stays a deliberate manual step.

if [ $# -ne 3 ]; then
    printf 'Usage: %s <ramdisk_path> <ramdisk_mounted> <label>\n' "$0" >&2
    exit 2
fi

ramdisk="$1"
mounted="$2"
label="$3"
min_free_kb=4194304 # 4 GiB

[ "$mounted" = true ] || exit 0

free_kb=$(df -k "$ramdisk" | awk 'NR == 2 { print $4 }')
if [ "$free_kb" -lt "$min_free_kb" ]; then
    printf 'error: less than 4 GiB free on %s before %s; run: just ramdisk prune\n' "$ramdisk" "$label" >&2
    exit 1
fi
