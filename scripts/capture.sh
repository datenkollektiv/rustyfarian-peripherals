#!/usr/bin/env bash
set -euo pipefail
# capture.sh — reset the board and record its serial output to a file.
# Usage: scripts/capture.sh <seconds> <logfile> [port]
#
# Non-interactive counterpart of `just monitor` for hardware measurements:
# `espflash monitor` needs a TTY, and without one it can leave the chip in the
# flash stub. This hard-resets the board, waits for the port to come back (a
# native USB Serial/JTAG port re-enumerates on reset) and copies raw bytes for
# <seconds>. Port defaults to the first /dev/cu.usbmodem* or /dev/cu.usbserial*.

if [ $# -lt 2 ]; then
    printf 'Usage: %s <seconds> <logfile> [port]\n' "$0" >&2
    exit 2
fi

seconds="$1"
logfile="$2"
port="${3:-}"

case "$seconds" in
    ''|*[!0-9]*) printf 'error: <seconds> must be a whole number, got %s\n' "$seconds" >&2; exit 2 ;;
esac

# `timeout` bounds each read; without it the loop below would fail every read
# silently and report an empty log as a successful capture.
if command -v timeout >/dev/null 2>&1; then
    timeout_bin=timeout
elif command -v gtimeout >/dev/null 2>&1; then
    timeout_bin=gtimeout
else
    printf 'error: capture needs `timeout` (GNU coreutils; macOS: brew install coreutils)\n' >&2
    exit 1
fi

if [ -z "$port" ]; then
    for p in /dev/cu.usbmodem* /dev/cu.usbserial*; do
        [ -e "$p" ] && port="$p" && break
    done
fi
if [ -z "$port" ]; then
    printf 'error: no serial port found; pass one explicitly\n' >&2
    exit 1
fi

mkdir -p "$(dirname "$logfile")"
espflash reset --port "$port" >/dev/null

# Give the reset time to drop the port, then wait up to 5 s for it to return.
sleep 0.3
for _ in $(seq 50); do
    [ -e "$port" ] && break
    sleep 0.1
done

printf 'Capturing %s for %ss into %s...\n' "$port" "$seconds" "$logfile"
# Reopen the port until the deadline: a read handle opened just before the
# reset drops the port goes dead silently.
: > "$logfile"
end=$((SECONDS + seconds))
while [ "$SECONDS" -lt "$end" ]; do
    if [ -e "$port" ]; then
        stty -f "$port" raw -echo 2>/dev/null || true
        "$timeout_bin" "$((end - SECONDS))" cat "$port" | tr -d '\r' >> "$logfile" || true
    fi
    sleep 0.2
done
lines="$(wc -l < "$logfile" | tr -d ' ')"
printf 'Capture done: %s lines\n' "$lines"
if [ "$lines" = 0 ]; then
    printf 'warning: no output captured — check the port, the board, and that the firmware prints periodically\n' >&2
fi
