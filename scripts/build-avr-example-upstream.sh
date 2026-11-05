#!/usr/bin/env bash
set -euo pipefail
# build-avr-example-upstream.sh — build the AVR tamer example against upstream avr-hal main
# Usage: scripts/build-avr-example-upstream.sh <REPO_ROOT> <AVR_NIGHTLY> [AVR_TARGET_DIR]
#   AVR_TARGET_DIR must be absolute: the build runs in a temp dir, so a relative path would
#   land there and be deleted on exit. Defaults to <REPO_ROOT>/target/avr.
#
# The weekly early-warning check: drops the pinned `rev` so the example resolves
# avr-hal's tip instead. Pinning keeps PR builds reproducible but would hide upstream
# breakage, which this job exists to catch — a failure here means upstream moved, not
# that this repo is broken.
#
# Everything happens on a throwaway copy in a temp dir, so no tracked file is touched
# even transiently.

if [ $# -lt 2 ]; then
    printf 'Usage: %s <REPO_ROOT> <AVR_NIGHTLY> [AVR_TARGET_DIR]\n' "$0" >&2
    exit 2
fi

root="$1"
avr_nightly="$2"
avr_target_dir="${3:-$root/target/avr}"
case "$avr_target_dir" in
    /*) ;;
    *) printf 'error: AVR_TARGET_DIR must be absolute (got "%s").\n' "$avr_target_dir" >&2; exit 2 ;;
esac

probe="$(mktemp -d)"
trap 'rm -rf "$probe"' EXIT

mkdir -p "$probe/examples"
cp -R "$root/examples/avr-nano-tamer" "$probe/examples/avr-nano-tamer"
rm -rf "$probe/examples/avr-nano-tamer/target"

# Both symlinks are required, not just `crates`. The example's path deps reach ../../crates,
# and those crates use `version.workspace = true`, which needs a workspace root to resolve.
# Cargo walks up the SYMLINK path rather than the resolved one, so linking only `crates`
# never reaches the real repo root and fails with `failed to find a workspace root`.
ln -s "$root/crates" "$probe/crates"
ln -s "$root/Cargo.toml" "$probe/Cargo.toml"

cd "$probe/examples/avr-nano-tamer"

# Drop the pinned rev so Cargo re-resolves the three avr-hal git packages
# (arduino-hal, atmega-hal, avr-hal-generic) against the branch tip.
#
# The lockfile is deliberately KEPT. Deleting it re-resolves every transitive registry
# dependency too, which makes this job fail on unrelated crate churn rather than on
# upstream avr-hal breakage — the only thing it exists to detect. (The sibling
# rustyfarian-ws2812 canary went red for three weeks on `encoding_rs` / `slice::as_chunks`
# after the lockfile was deleted; avr-hal itself was fine throughout.)
#
# Removing the `rev` is enough on its own: it changes the git source spec, so the
# locked entries no longer match and Cargo must refetch those three packages.
# Registry deps stay at their locked versions unless upstream avr-hal genuinely
# requires newer ones — in which case failing IS the signal.
sed -i.bak '/^rev = /d' Cargo.toml && rm -f Cargo.toml.bak

# -Z build-std=core is passed on the command line rather than set in a child
# .cargo/config.toml: two config files MERGE their build-std arrays instead of
# overriding, which would yield ["std","panic_abort","core"] and fail on AVR.
cargo "+$avr_nightly" build --release -Z build-std=core --bins --target-dir "$avr_target_dir"
