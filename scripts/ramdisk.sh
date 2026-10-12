#!/usr/bin/env bash
set -euo pipefail
# ramdisk.sh — manage the shared build RAM disk at /Volumes/RustBuilds.
# Usage: scripts/ramdisk.sh attach|detach|prune [days]
#
# The RAM disk is shared across all rustyfarian repos that build for ESP targets
# (ws2812, network, peripherals, …); each repo namespaces its artifacts under
# targets/{hal,idf}/<repo-name>. Attaching is optional — without it, builds fall
# back to target/hal and target/idf on disk.
#
# `prune [days]` removes every targets/{hal,idf}/<repo> tree that holds no file
# newer than <days> days (default 7, minimum 1), so idle trees from other repos
# stop filling the shared disk. To wipe the whole disk, detach and re-attach.

if [ "$(uname)" != "Darwin" ]; then
    printf 'error: ramdisk.sh requires macOS (hdiutil/diskutil not available)\n' >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"

RAMDISK_NAME="RustBuilds"
# Interpreted as GiB (1024^3 bytes). Set RUSTBUILDS_RAMDISK_SIZE_GB to share one
# size across all rustyfarian repos that use /Volumes/RustBuilds; the legacy
# RAMDISK_SIZE_GB is still honoured as a fallback.
RAMDISK_SIZE_GB="${RUSTBUILDS_RAMDISK_SIZE_GB:-${RAMDISK_SIZE_GB:-6}}"
if ! [[ "$RAMDISK_SIZE_GB" =~ ^[1-9][0-9]*$ ]]; then
    printf 'error: RAMDISK_SIZE_GB must be a positive integer (got: "%s")\n' "$RAMDISK_SIZE_GB" >&2
    exit 1
fi
RAMDISK_PATH="/Volumes/$RAMDISK_NAME"
BYTES_PER_GIB=$((1024 * 1024 * 1024))
BYTES_PER_SECTOR=512

case "${1:-}" in
    attach)
        if is_ramdisk_mounted "$RAMDISK_PATH"; then
            echo "RAM disk already attached at /Volumes/$RAMDISK_NAME"
        else
            # hdiutil ram:// expects size in 512-byte sectors.
            SECTORS=$(( RAMDISK_SIZE_GB * BYTES_PER_GIB / BYTES_PER_SECTOR ))
            DEV=$(hdiutil attach -nomount "ram://$SECTORS" | xargs)
            # HFS+ is used deliberately: `diskutil erasevolume HFS+` is the
            # canonical one-step formatter for hdiutil-created RAM devices and
            # works on all macOS versions this project targets. APFS on a RAM
            # disk needs separate container steps and offers no benefit for an
            # ephemeral build cache.
            diskutil erasevolume HFS+ "$RAMDISK_NAME" "$DEV"
            echo "RAM disk attached at /Volumes/$RAMDISK_NAME (${RAMDISK_SIZE_GB} GB)"
        fi
        mkdir -p "/Volumes/$RAMDISK_NAME/targets/hal"
        mkdir -p "/Volumes/$RAMDISK_NAME/targets/idf"
        ;;
    detach)
        if is_ramdisk_mounted "$RAMDISK_PATH"; then
            hdiutil detach "/Volumes/$RAMDISK_NAME"
            echo "RAM disk detached."
        else
            echo "RAM disk not attached."
        fi
        ;;
    prune)
        DAYS="${2:-7}"
        if ! [[ "$DAYS" =~ ^[1-9][0-9]*$ ]]; then
            printf 'error: prune expects a number of days >= 1 (got: "%s")\n' "$DAYS" >&2
            printf 'to wipe the whole RAM disk use: just ramdisk detach && just ramdisk attach\n' >&2
            exit 1
        fi
        if ! is_ramdisk_mounted "$RAMDISK_PATH"; then
            echo "RAM disk not attached."
            exit 0
        fi
        PRUNED=0
        for tree in "$RAMDISK_PATH"/targets/hal/* "$RAMDISK_PATH"/targets/idf/*; do
            [ -d "$tree" ] && [ ! -L "$tree" ] || continue
            # A failed scan must not read as "stale": skip the tree instead.
            if ! recent=$(find "$tree" -type f -mtime "-${DAYS}" -print -quit); then
                printf 'skipped %s (scan failed)\n' "${tree#"$RAMDISK_PATH"/}" >&2
                continue
            fi
            [ -z "$recent" ] || continue
            size=$(du -sh "$tree" 2>/dev/null | awk '{ print $1 }') || size='?'
            rm -rf "$tree"
            echo "pruned ${tree#"$RAMDISK_PATH"/} ($size, no file newer than ${DAYS} days)"
            PRUNED=$((PRUNED + 1))
        done
        if [ "$PRUNED" -eq 0 ]; then
            echo "nothing to prune: every target tree has files newer than ${DAYS} days"
        fi
        df -h "$RAMDISK_PATH" | tail -1 | awk '{ printf "RAM disk %s used of %s, %s free\n", $3, $2, $4 }'
        ;;
    *)
        printf 'Usage: scripts/ramdisk.sh attach|detach|prune [days]\n' >&2
        exit 1
        ;;
esac
