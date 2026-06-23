#!/usr/bin/env bash
# lib.sh — shared helpers for scripts/. Source this file; do not execute it.

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    printf 'Error: lib.sh must be sourced, not executed directly.\n' >&2
    exit 2
fi

# is_ramdisk_mounted <path>
# Returns 0 if <path> is a live mounted volume on macOS, 1 otherwise. Uses
# `diskutil info` rather than a directory existence check, so a stale
# /Volumes/<name> directory (left after a failed detach) is correctly reported
# as unmounted.
is_ramdisk_mounted() {
    local path="$1"
    [ -n "$path" ] || return 1
    diskutil info "$path" >/dev/null 2>&1
}

# get_example_features_from_toml <example> <crate_dir>
# Prints the example's `required-features` as a comma-separated list, or nothing
# if the example has none (some ESP-IDF examples need no features).
get_example_features_from_toml() {
    local example_name="$1" crate_dir="$2"
    [ -f "$crate_dir/Cargo.toml" ] || return 0

    local in_example=0 found=0 features=""
    while IFS= read -r line; do
        if [[ "$line" == "[[example]]" ]]; then
            in_example=1
            found=0
            features=""
            continue
        fi
        if [ $in_example -eq 1 ]; then
            if [[ "$line" =~ ^name\ =\ \"([^\"]+)\" ]]; then
                if [ "${BASH_REMATCH[1]}" = "$example_name" ]; then found=1; else in_example=0; fi
            fi
            if [ $found -eq 1 ] && [[ "$line" =~ ^required-features\ =\ \[(.*)\] ]]; then
                features="${BASH_REMATCH[1]}"
                break
            fi
            if [[ "$line" =~ ^\[\[ && ! "$line" =~ ^\[\[example\]\] ]]; then in_example=0; fi
        fi
    done < "$crate_dir/Cargo.toml"

    printf '%s' "$(printf '%s' "$features" | tr -d '"' | tr -d ' ')"
}

# resolve_example <example> <hal_dir> <idf_dir>
# Parses a `{hal|idf}_{chip}_{name}` example name and exports the build context
# as globals: EX_TIER EX_CHIP EX_MCU EX_TARGET EX_PKG EX_TARGET_DIR EX_FEATURES EX_XTENSA.
resolve_example() {
    local example="$1" hal_dir="$2" idf_dir="$3"
    EX_TIER="${example%%_*}"
    EX_CHIP="$(printf '%s' "$example" | cut -d_ -f2)"
    EX_XTENSA=0

    case "$EX_TIER" in
        hal)
            EX_PKG="rustyfarian-esp-hal-peripherals"
            EX_TARGET_DIR="$hal_dir"
            case "$EX_CHIP" in
                c3)      EX_TARGET="riscv32imc-unknown-none-elf";  EX_MCU="esp32c3" ;;
                c6)      EX_TARGET="riscv32imac-unknown-none-elf"; EX_MCU="esp32c6" ;;
                esp32)   EX_TARGET="xtensa-esp32-none-elf";        EX_MCU="esp32";   EX_XTENSA=1 ;;
                esp32s3) EX_TARGET="xtensa-esp32s3-none-elf";      EX_MCU="esp32s3"; EX_XTENSA=1 ;;
                *) printf 'Unknown chip "%s" in "%s" (expected c3|c6|esp32|esp32s3).\n' "$EX_CHIP" "$example" >&2; return 1 ;;
            esac
            ;;
        idf)
            EX_PKG="rustyfarian-esp-idf-peripherals"
            EX_TARGET_DIR="$idf_dir"
            case "$EX_CHIP" in
                c3)      EX_TARGET="riscv32imc-esp-espidf";  EX_MCU="esp32c3" ;;
                c6)      EX_TARGET="riscv32imac-esp-espidf"; EX_MCU="esp32c6" ;;
                esp32)   EX_TARGET="xtensa-esp32-espidf";    EX_MCU="esp32";   EX_XTENSA=1 ;;
                esp32s3) EX_TARGET="xtensa-esp32s3-espidf";  EX_MCU="esp32s3"; EX_XTENSA=1 ;;
                *) printf 'Unknown chip "%s" in "%s" (expected c3|c6|esp32|esp32s3).\n' "$EX_CHIP" "$example" >&2; return 1 ;;
            esac
            ;;
        *)
            printf 'Error: example name must start with "hal_" or "idf_".\n' >&2
            return 1
            ;;
    esac

    EX_FEATURES="$(get_example_features_from_toml "$example" "crates/$EX_PKG")"
    # Bare-metal examples need a chip feature; default it if the manifest omits one.
    if [ "$EX_TIER" = hal ] && [ -z "$EX_FEATURES" ]; then
        EX_FEATURES="esp32${EX_CHIP},rt,unstable"
    fi
}
