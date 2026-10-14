---
gate: none
desk-work: available
---

# Feature: esp-hal Rotary Encoder Twin v1

Async esp-hal twin of the ESP-IDF interrupt-driven `rotary::Encoder` ([ADR-006](../adr/006-interrupt-encoder-instance-and-api-shape.md)).
First consumer: an embassy firmware on `esp-rtos` driving the CrowPanel 1.28" ESP32-S3 rotary knob.

## Decisions
| Decision                                       | Reason                                                                  | Rejected Alternative                     |
|:-----------------------------------------------|:------------------------------------------------------------------------|:-----------------------------------------|
| `async fn wait_for_step() -> EncoderDirection` | Awaits edges via `Input::wait_for_any_edge`; resolves per detent        | Sync `update` poll for rotation          |
| Button stays sync `update(now_ms)`             | IDF parity; timing needs a caller clock, keeps `embassy-time` out       | Async button with an internal timer      |
| No blocking variant                            | `tamer::rotary::QuadratureInput` over esp-hal `Input` already serves it | `update` also samples A/B; a second type |
| Read → arm → re-read levels                    | Edge wait is one-shot; re-read closes the re-arm race                   | Trust edge futures alone                 |
| Position lives in the decoder                  | Single source of truth; task context needs no atomics                   | Mirrored atomic as in the IDF ISR        |
| Driver always compiled                         | Needs only esp-hal; two-pin select is a hand-rolled `poll_fn`           | Gate the module behind `async`           |
| `embassy` feature for the example runtime      | esp-rtos needs a chip feature, so it cannot be a dev-dependency         | Unconditional esp-rtos dependency        |
| No shared trait yet                            | ADR-006: extract only after hardware validation                         | Extract now from two impls               |

## Constraints
- `no_std`, no `alloc`, no `unsafe` in the tier.
- Exact pins: `esp-hal =1.2.2`, `esp-rtos =0.4.0`, `embassy-executor =0.10.0`, `embassy-time =0.5.1`, `embassy-futures =0.1.2`.
- All decoding stays in `tamer::rotary::QuadratureDecoder` and `tamer::button::ButtonDecoder`.
- Defaults match the IDF `EncoderConfig`: 4 steps/detent, 50 ms debounce, 1000 ms long press, 300 ms double click.
- CrowPanel pins: A = GPIO45 (strapping, input-only use is safe), B = GPIO42, button = GPIO41.
- `just check-hal` keeps the C3 line and gains an S3 line on `cargo +esp -Zbuild-std=core,alloc`.
- Out of scope: CST816S driver (ADR-007 demand-driven rule).

## Behavioral contract
- Each `wait_for_step` call samples A/B and feeds the decoder before arming, so cancellation in a `select` loses no state.
- A detent decoded during the call resolves it; sub-detent edges loop internally.
- Missed edges surface only as a 2-bit jump, which the decoder drops as bounce.
- Callers polling the button alongside rotation use a `Ticker`, not a fresh `Timer` per loop.

## Open Questions
- [x] Do fast spins on the CrowPanel ever miss or double a detent with task-level re-arming? No: every detent took exactly 4 edges (plus rare 2-edge bounce), across 59 fast detents.

## State
- [x] Design approved
- [x] Core implementation (`rotary` module, `hal_s3_rotary`, `check-hal` S3 line)
- [x] Checks passing (`just verify`, `just check-hal`, `just build-example hal_s3_rotary`)
- [x] Hardware validated on the CrowPanel
- [x] Documentation updated (ADR-006 amendment, README, CHANGELOG, ROADMAP, maintenance plan)

## Session Log
- 2026-10-14 — Feature doc created via /feature from the knob's esp-hal parity request.
- 2026-10-14 — Implemented; host gates, check-hal C3+S3, S3 clippy and example builds green; hardware run pending.
- 2026-10-14 — CrowPanel S3 run passed: slow and fast spins both ways, click, double click, long press, presses while turning.
