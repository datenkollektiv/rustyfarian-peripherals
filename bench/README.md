# Bench

Guided hardware checks for the final hardware pass, one directory per board ([ADR-011](../docs/adr/011-adopt-rustyfarian-bench.md)).
The runner is [`rustyfarian-bench`](https://github.com/datenkollektiv/rustyfarian-bench), pinned to `v0.5.0` in the justfile; `RUSTYFARIAN_BENCH=<path>` uses a local checkout.

| Where | Source of truth for |
|:--|:--|
| `<board>/<check>.toml` | what a check does (header comment and `say` lines) |
| `<board>/README.md` | the board's wiring and its check names |
| [`release-plan.md`](../release-plan.md) § Final hardware pass | gate results |
| `bench.toml` | build and flash commands, boards |

| Board | Directory | Last full pass |
|:--|:--|:--|
| CrowPanel 1.28" HMI (ESP32-S3) | [s3/](s3/) | 2026-11-11 |
| ESP32-C3 SuperMini | [c3/](c3/) | open |
| ESP32-C6-DevKitC-1 | [c6/](c6/) | open |
| ESP32-WROOM-32 DevKit | [esp32/](esp32/) | open |
| Adafruit ESP32 Feather V2 | [esp32-feather/](esp32-feather/) | open |

A full pass is every gate of the board in both modes, as recorded in the release plan; the date is taken from those rows, never ahead of the run.

## Running

```sh
just bench <board> <check> --unattended    # log-judged steps only; walk away
just bench <board> <check> --attended --no-build   # the steps that need a person
just bench <board>                          # every check of the board
just bench-list                             # all scenarios
just bench-detect                           # chip and MAC per port
```

Add `--port <port>` when two boards of the same chip are attached.
A gate passes only when both modes pass on the same commit; record it as one row in the release plan.
Logs and `results.jsonl` stay local and git-ignored, because they record board MACs.

## If it hangs

- No port: the board sleeps or sits in download mode; replug it, holding BOOT only if it is truly stuck.
- Output ends in `waiting for download` on the S3: power-cycle it without BOOT (see lore).
- Ctrl-C ends a run; it is recorded as `aborted`.
