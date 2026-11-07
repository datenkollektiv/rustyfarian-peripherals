# Bench

Guided, rerunnable hardware checks for the final hardware pass ([`release-plan.md`](../release-plan.md) § Final hardware pass).
One directory per board; a board is one bench session, finished before the next.
The adoption and its reasons are recorded in [ADR-011](../docs/adr/011-adopt-rustyfarian-bench.md).

The runner is the sibling project [`rustyfarian-bench`](../../rustyfarian-bench) (Python + uv).
The `just bench` recipes pin it to `v0.3.0` through `uvx --from git+https://github.com/datenkollektiv/rustyfarian-bench@v0.3.0`.
To run against a local checkout instead, set `RUSTYFARIAN_BENCH=/path/to/rustyfarian-bench`.

| Board                        | Directory    | State                                                                 |
|:-----------------------------|:-------------|:----------------------------------------------------------------------|
| CrowPanel 1.28" HMI (S3)     | [s3/](s3/)   | in progress: hal `b3f` passed 2026-11-06; `rotary` and `rotary-isr` not run |
| ESP32-C3 SuperMini           | [c3/](c3/)   | scenarios written, not run                                            |
| ESP32-C6-DevKitC-1           | [c6/](c6/)   | scenarios written, not run                                            |
| ESP32 WROOM-32 / Feather V2  | [esp32/](esp32/) | scenarios written, not run                                        |

## Running a check

```sh
just bench s3 b3f       # one check (hal, then offers the idf twin)
just bench hal_s3_b3f   # start at one example
just bench s3 rotary --no-build   # flags pass through
just bench s3           # every check for the board
just bench s3 --unattended   # only the steps the log can judge; walk away
just bench s3 --attended     # only the steps that need a person
just bench-list         # all scenarios
just bench-detect       # chip and MAC per connected port
```

The runner builds first, waits for the board (plug it in when asked), flashes, resets, and guides each step with an automatic verdict.
Steps that need a person carry `attended = true` (or a `confirm`); every `b3f` and `rotary` check starts with a hands-off 5 s no-panic watch, so an unattended run still proves boot on each chip.
An armed `duration` step accepts a different length for one run (e.g. a longer soak); the result records the scenario value next to it.

## Layout

- `bench.toml` — build/flash commands and the boards (chip, optional MAC).
- `<board>/<check>.toml` — one scenario: the examples it runs and the shared steps.
- `<board>/logs/` — captured serial output per run; local and git-ignored.
- `<board>/results.jsonl` — one JSON line per example run; local and git-ignored, because both can record board MAC addresses.

## Recording a result

- A gate passes only when both `--unattended` and `--attended` runs pass; each result records its `mode`, and a pass in one mode is never a full pass.
- When a check passes on a tier, tick its gate item in `release-plan.md` § Final hardware pass.
  Each tick carries the commit hash, the board and the date.
- Record the same `pass (date, commit, board)` in the matching cell of `docs/features/chip-matrix-v1.md` § Hardware pass; a cell that has not run stays `not run`.
- A build or link result never ticks a hardware cell.
- Record any non-obvious gotcha in `docs/project-lore.md`.
