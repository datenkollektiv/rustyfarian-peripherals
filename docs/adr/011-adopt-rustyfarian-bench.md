# ADR-011: Adopt rustyfarian-bench for on-device checks

## Status
Accepted (2026-11-07).
Follows `rustyfarian-power` ADR-005, `rustyfarian-network` ADR-024 and `rustyfarian-knob` ADR-016, which run their checks through the same runner.
It runs the hardware pass that ADR-010 requires.

## Context
- The on-device checks of this repo are run by hand from `release-plan.md` § Final hardware pass: flash, read the console, then tick the item with the board, port and date.
- A rerun means re-reading the steps, and the record is a tick with no comparable output.
- The rustyfarian family now has `rustyfarian-bench`, a guided runner (Python + uv).
  A consumer describes boards and checks as TOML under `bench/`; the runner builds, waits for the board, flashes, resets, captures serial and judges each step against regexes or an operator answer.
- Most examples here come in `hal_*` / `idf_*` pairs with stable log lines (for example `hal_c3_bme680` / `idf_c3_bme680` and `hal_s3_rotary` / `idf_s3_rotary`), which fit one scenario that runs both twins through shared steps.
- The runner calls one build and one flash command per example name.
  Unlike the siblings, this repo needs no bench-only recipes for that: `just build-example` and `just flash` already dispatch on the `hal_` / `idf_` prefix through `scripts/lib.sh` (`resolve_example`).
- Results record board MACs, and this repository is public.

## Decision
1. On-device checks of the default example builds are written as `rustyfarian-bench` scenarios under `bench/<board>/<check>.toml`, one per check, each running the hal example first and then its idf twin where one exists (`c3/bme680-async` and `s3/rotary-isr` are hal only).
   The boards are `s3` (CrowPanel 1.28" HMI), `c3`, `c6` and `esp32`, as listed in the `[boards.*]` tables of `bench/bench.toml`.
2. `bench/bench.toml` calls the existing `just build-example {example}` and `just flash {example} {port}`.
   No bench-only build or flash recipe is added.
3. Boards are bench sessions: one board is finished before the next one starts, and a board's checks are the evidence for its cells in the final pass.
4. `just bench <board> [check]`, `just bench-list` and `just bench-detect` run `rustyfarian-bench` pinned to the `v0.3.0` release through `uvx --from git+https://github.com/datenkollektiv/rustyfarian-bench@v0.3.0`.
   Bumping the tag is how this repo opts into a new runner.
   `RUSTYFARIAN_BENCH=<path>` points the recipes at a local checkout instead, for bench development.
5. Run logs and results stay git-ignored: `bench/*/logs/` and `bench/*/results.jsonl` record board MACs, and `bench.toml` pins no MAC.
6. `release-plan.md` § Final hardware pass stays the authority for what must pass before a release.
   A bench run is evidence for its items.
   The gate id is `<board>/<check>`, the same string as the `just bench <board> <check>` command.
7. State on 2026-11-07: the s3 board is in progress; the c3, c6 and esp32 scenarios are written and not yet run.

## Consequences
- The hardware pass becomes rerunnable and comparable across the four boards, with an automatic verdict where serial output proves a step and an explicit operator answer where it cannot (a button press, a turn, a reading on a meter).
- Bench runs depend on `uv` and on network access to GitHub on first use, then the uv cache; builds, `just verify` and CI are unaffected.
- Build-time switches and cargo features do not reach the runner's build step, because `bench.toml` passes only the example name; features that need a build flag keep their existing recipes.
- Scenario regexes couple to example log lines; changing an example's log text means updating its scenario.
- Moving to a new runner is a tag edit, so the runner version is visible in review.
- 2026-11-07: pinned to `v0.3.0` for `--unattended` / `--attended` runs.
  Scenarios mark steps that need a person with `attended = true`; a gate needs both modes to pass on one commit.
  Every `b3f` and `rotary` check gains a hands-off 5 s no-panic watch so the unattended run proves boot per chip.

## Alternatives Considered
| Alternative                                      | Pros                            | Cons                                                       | Why Rejected                                              |
|:-------------------------------------------------|:--------------------------------|:-----------------------------------------------------------|:----------------------------------------------------------|
| Keep the hand-run checks in `release-plan.md`    | No tooling to maintain          | Each rerun re-reads the steps; the record is a bare tick   | Not rerunnable or comparable across boards                |
| One bespoke bench script per board               | No sibling dependency           | One more driver per board                                  | The family converged on one runner                        |
| Bench-only build and flash recipes               | Isolates the bench from the dev recipes | Duplicates the prefix dispatch `build-example` and `flash` already do | Not needed here; the existing recipes already cover both tiers |
| Sibling checkout only, no tag pin                | No network on first use         | The runner version drifts with the checkout                | The tag makes the runner version an explicit review item  |

## References
- `bench/README.md` — how to run a check and what each board directory holds.
- `docs/adr/010-shared-logic-ladder.md` — the ladder whose hardware pass this runner executes.
- `rustyfarian-power` ADR-005, `rustyfarian-network` ADR-024 and `rustyfarian-knob` ADR-016 — the sibling adoptions.
