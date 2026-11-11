# Bench — CrowPanel 1.28" HMI (ESP32-S3)

No wiring: knob A = GPIO 45, B = GPIO 42, button = GPIO 41.

## Commands

Builds, flashes and guides the button check (3 presses) on hal, then offers idf.
No step waits for Enter: read the instruction and act; each step passes as soon as its line arrives (60 s):

```sh
just bench s3 b3f
```

### More commands

Same, but skips the build when nothing changed:

```sh
just bench s3 b3f --no-build
```

Runs the rotary check (direction, round trip, click, double click, long press) on hal, then offers idf:

```sh
just bench s3 rotary
```

Runs the ISR rotary check under load (10+ detents each way without pausing, fast spin, heartbeat; no skipped edges):

```sh
just bench s3 rotary-isr
```

Runs every S3 check in turn:

```sh
just bench s3
```

Shows the chip and MAC on each connected port:

```sh
just bench-detect
```

## If it hangs

- No port at all: the board is in deep sleep; hold BOOT, replug, release BOOT.
- Output ends in `waiting for download`: unplug and replug the board (without BOOT) while the run keeps waiting.
- Ctrl-C ends a run; it is recorded as `aborted`.
