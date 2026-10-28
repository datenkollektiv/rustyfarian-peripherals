# ADR-006: Per-instance interrupt-encoder ISR context and API shape

## Status
Accepted

## Context
The first interrupt-driven encoder for the ESP-IDF tier arrived as a donation (a production device running on CrowPanel 1.28" ESP32-S3).
The knob's implementation kept decoder, position, and pins in **module-level statics**, passing `null_mut()` as the ISR context argument — a pattern that enforces one-instance-per-process (fine for a single physical encoder, fine for the knob).

The crate's own guidance (`AGENTS.md`, `rustyfarian-esp-idf-power` precedent) says hardware drivers are trait-first: a public API should not carry limits that are awkward to reverse.
A reusable upstream primitive shipping a one-instance constraint as implicit behavior is such a limit: a second `Encoder::new()` would silently corrupt the first with no error.

Separately, the knob is dual-target (a variant exists for esp-hal), and both variants use the same pure `tamer::rotary::QuadratureDecoder` and `tamer::button::ButtonDecoder`.
The **intended contract** is that ISR-driven and polled interfaces both feed the same pure state machines, and a consumer can swap between them based on latency needs.
This means the API shape should be extractable mechanically into a shared trait once both implementations exist.

## Decision
**Decision 1: Per-instance ISR context, not module statics.**

Replace module-level statics with a `Box<IsrContext>` field on the public `Encoder<'d>` struct.
The `IsrContext` is heap-allocated so its address is stable across the constructor's `return Self` by value.
Pass `&*ctx as *const _ as *mut c_void` to `gpio_isr_handler_add` for both A and B pins.
Never use `Box::into_raw` / `from_raw` — the owned `Box` field gives the exact "valid until `Encoder` drops" invariant with no manual reconstruction.

This unblocks multiple `Encoder` instances in the same process and eliminates the silent-corruption behavior change from one→multi later.

**Decision 2: Concrete `Encoder<'d>` struct now, with trait-readiness by design.**

Ship `Encoder<'d>` as a concrete struct (not behind a trait), but **design the method surface to map mechanically onto a shared contract** when the esp-hal twin lands and the trait is extracted.

The current API surface: `update(now_ms: u64) -> Option<ButtonEvent>`, `position() -> i32`, `set_position`, `reset`, `is_button_pressed`, diagnostics.
This is intentionally **synchronous** (not async): the ISR drives the decoder on every edge, `update` only advances button timing (inherently needing a caller-supplied timestamp), and a caller polled at any cadence gets the current state.

**Flag as an open risk:** an esp-hal async implementation might prefer `async fn wait_for_any_edge().await -> EncoderDirection` rather than a synchronous `update(now_ms)` poll.
If so, the trait extracted later will need a second, async variant or a complex dual-API shape.
That is *not* decided now — the trait is extracted when the esp-hal twin lands and its shape is verified against hardware.
This ADR **records the departure** from AGENTS.md's "every hardware interaction behind a trait" and commits to extracting the trait then, not guessing now from one impl.

## Consequences
- Multiple `Encoder` instances are now possible and safe (no silent corruption).
- The heap allocation (`Box<IsrContext>`) is stable and long-lived; it survives the constructor return and is freed only in `Drop`, after ISR teardown.
- The `Drop` implementation's load-bearing order (disable interrupts → remove handlers → tombstone + critical-section barrier → free heap) closes a use-after-free race on dual-core SoCs (e.g., ESP32-S3).
- The polled button half reuses `tamer::button::ButtonDecoder` directly (no seam); the interrupt half uses a per-instance context pattern that is **specific to raw ESP-IDF FFI** and not trait-ified.
- The public API (synchronous, `update(now_ms)` poll) is compatible with a later trait extraction, but the async shape question is **deliberately deferred**: when the esp-hal twin lands and its design is proven on hardware, the trait is extracted and both impls are unified.
  If the esp-hal side needs async, that is discovered then and the trait is designed to subsume both shapes (or both are kept separate, per AGENTS.md's demand-driven rule).

## Teardown ordering and residual risk

`Drop` must free `Box<IsrContext>` without racing an ISR that may be mid-flight on another core (dual-core ESP32/ESP32-S3). The order is load-bearing:

1. `gpio_intr_disable` both pins — no *new* edge can trigger the ISR.
2. `gpio_isr_handler_remove` both pins — ESP-IDF stops dispatching to the trampoline.
3. Store `armed = false` (`Release`), then take a `critical_section::with(|_| {})` barrier. Steps 1–2 only stop *future* interrupts; an edge that fired just before them can still be running `encoder_isr` on the other core. The ISR wraps its *entire* body (tombstone check included) in one `critical_section::with`, and a critical section is mutually exclusive across cores, so this barrier cannot return until any in-flight invocation that already entered its critical section has exited it. Any invocation that enters *after* the barrier sees `armed == false` and returns without touching `self.ctx`.
4. The compiler frees `Box<IsrContext>` after `drop` returns — safe only because 1–3 proved nothing can still observe it.

Steps 1–2 are best-effort: failures are logged (`log::warn!`), never propagated (`Drop` cannot return `Result`).

**Residual risk — fixed in code, pending device confirmation (2026-10-28).**
The barrier in step 3 only synchronizes with an ISR invocation that has *already entered* its critical section.
ESP-IDF v5.3.3 does not close the earlier window: `gpio_isr_loop` loads `fn` and `args` from the handler table without a lock (`esp_driver_gpio/src/gpio.c:465-466`), and `gpio_isr_handler_remove` only nulls them under a spinlock the dispatcher never takes (`gpio.c:542-553`).
The GPIO ISR service runs on the core of the first `gpio_install_isr_service` caller (`gpio.c:597-603`).
On dual-core chips (ESP32, ESP32-S3), a `drop` on the other core could therefore free `Box<IsrContext>` after the ISR core loaded `args` but before `encoder_isr` entered its critical section.
Fix: on dual-core builds, `drop` now runs a no-op `esp_ipc_call_blocking` on each core after steps 1–3 and before the free; the IPC task on a core can only run once any ISR in flight there has returned.
Because `fn` and `args` are loaded separately, the trampoline also returns early on a null `arg`.
If either pin fails to disarm or the IPC barrier fails, `drop` leaks the context instead of freeing it, since a still-registered handler could reach it.
This argument rests on the ESP-IDF v5.3.3 sources cited above (`ESP_IDF_VERSION` in `.cargo/config.toml.dist`); re-verify it on every ESP-IDF bump.
Single-core chips (ESP32-C3, ESP32-C6) were never affected: `drop` runs in task context on the ISR core, so no dispatch can be mid-flight.
`drop` must run in task context: never from an ISR or an IPC callback, inside a critical section, with interrupts masked, or with the scheduler suspended — `esp_ipc_call_blocking` would block forever.

## Alternatives Considered
|                                              Alternative | Pros                                                          | Cons                                                                                                                       | Why Rejected                                                                                                             |
|---------------------------------------------------------:|:--------------------------------------------------------------|:---------------------------------------------------------------------------------------------------------------------------|:-------------------------------------------------------------------------------------------------------------------------|
|            Keep module statics (lift-and-shift the knob) | Minimal change; code is proven on hardware                    | Implicit one-instance limit; silent corruption on second `new()`; expensive to reverse pre-1.0                             | The constraint is easy to break now, expensive to fix later; unacceptable for a reusable crate                           |
|       Trait-first, extract now (guess the esp-hal shape) | Satisfies AGENTS.md; gives a unified API surface immediately  | Speculative without esp-hal twin; preempts its design; the async-vs-sync question is open                                  | esp-hal's deadline is soon, not now; guessing inverts the demand-driven principle                                        |
|   Async trait from day one (async `wait_for_any_edge()`) | If esp-hal also chooses async, the trait is correct from v1   | This tier is `std` and can do async, but the esp-hal tier can *also* do async in `std` mode; guessing is still speculative | Same: defer until the esp-hal twin lands and its choice (async or sync) is verified on hardware                          |
|      Per-instance static `[Option<IsrContext>; N]` array | Avoids heap allocation (no `Box`)                             | Requires a compile-time encoder count; uses stack space for unused slots; still needs a lifetime-safe pointer scheme       | Rigid and less flexible than heap allocation; does not simplify the ISR context pointer handoff                          |

## Amendment (2026-10-14) — esp-hal twin shape

The esp-hal twin settles the open sync-vs-async risk from Decision 2.

**Async rotation, sync button, no blocking variant.**
`rustyfarian_esp_hal_peripherals::rotary::Encoder` awaits detents with `async fn wait_for_step(&mut self) -> EncoderDirection`, built on esp-hal's one-shot `Input::wait_for_any_edge`.
Decoding runs in task context: no ISR, no statics, no heap, no `unsafe`, and the position lives in the `QuadratureDecoder`.
Each wait reads A/B, arms both edge futures, then re-reads the levels, because arming clears any edge that landed after the first read.
The button stays a synchronous `update(now_ms) -> Option<ButtonEvent>`, as in the ESP-IDF tier.
No blocking variant ships; firmware without an executor uses `tamer::rotary::QuadratureInput` plus `tamer::button::ButtonDecoder` directly.
The driver is executor-agnostic and always compiled; the `embassy` feature only pulls in the example runtime (`esp-rtos`).

**Consequence for the shared trait.**
The trait, when extracted, is the synchronous core: `update`, `position`, `set_position`, `reset`, `is_button_pressed`.
Rotation delivery stays tier-specific (ISR-updated position on ESP-IDF, awaited step on esp-hal), so no async trait and no dual-API shape is needed.

**Status.**
The esp-hal twin passed hardware validation on the CrowPanel 1.28" ESP32-S3 on 2026-10-14: slow and fast spins both ways decoded one step per detent, and click, double click and long press decoded with the default tuning, also while turning.
Trait extraction is now unblocked and lands on demand, per the demand-driven rule.

## Amendment (2026-10-27) — decoding under blocking work

**Problem.**
The async esp-hal `rotary::Encoder` decodes in task context.
A ~90 ms blocking redraw between two `wait_for_step` calls loses about half the detents on the CrowPanel 1.28" ESP32-S3.
A rotation-only task on the same cooperative executor is still stalled, so the async shape cannot fix this by itself.

**Spikes.**
Both candidates ran on the same board: 30 detents per revolution, a 90 ms busy-wait per position change, and round trips to a marked position.
A pass is at least 30 counts per trip in each direction with zero net drift and zero skips.

| Candidate                       | Normal ×2 | Fast ×2 | Reversals | Net drift | Skipped |
|:--------------------------------|:----------|:--------|:----------|:----------|--------:|
| Baseline (cooperative executor) | 16/15     | 12/16   | 17/17     | −3        |      30 |
| A: `InterruptExecutor` prio 3   | 60/60     | 60/60   | 37/37     | 0         |       0 |
| B: decode in GPIO ISR           | 60/60     | 61/61   | 27/27     | 0         |       0 |

**Decision: B, an ISR-decoded `rotary::InterruptEncoder`.**
B decodes in the GPIO interrupt, so `position()` stays current whatever the task is doing.
B was chosen for tier parity with the ESP-IDF `Encoder`: the ISR keeps `position()` current on both tiers, so a consumer switches tiers by changing the import and constructor.

**Rejected: A, interrupt-priority executor plus `split()`.**
A was also lossless, but it forces embassy onto consumers, needs cross-executor channels and `Send` halves, and diverges from the ESP-IDF shape.

**Shape of B.**
- The app installs the single esp-hal GPIO handler and calls `rotary::on_gpio_interrupt()` from it.
  The library does not own that handler, because esp-hal dispatches one GPIO handler for the whole chip.
- The hook works on raw GPIO registers and clears its own status bits on every call, so it never returns with them set and cannot storm.
- The hook never nests `critical_section` outside the ISR.
  Thread-side code never calls `listen`, `unlisten`, or `clear_interrupt` inside a `critical_section`, which avoids a `GPIO_LOCK` and `critical_section` lock inversion on dual-core S3.
- One instance per app: a second `InterruptEncoder` returns `EncoderError::AlreadyInUse`.
- The async `Encoder` stays unchanged.

**Consequence for the shared trait.**
The trait is the synchronous core: `update`, `position`, `set_position`, `reset`, and `is_button_pressed`.
Rotation now maps to `position()` on both tiers, which refines the 2026-10-14 amendment's "tier-specific rotation delivery".
`wait_for_step` stays an esp-hal extra outside the trait.

**Status.**
Spike-validated on hardware.
The library type `InterruptEncoder` passed hardware validation on the CrowPanel 1.28" ESP32-S3 on 2026-10-27 with `hal_s3_rotary_isr`: round trips at normal and fast speed and fast reversals netted to zero with no skipped Gray states under a 90 ms blocking redraw, and click, double click and long press decoded.
A downstream knob firmware confirmed it on 2026-10-28 with the real ~90 ms GC9A01 full redraw: up to three detents per blocked interval all reached `position()`, with no skipped Gray states at 24 detents/s.

## Amendment (2026-10-28) — trait extracted

The shared trait deferred above is now extracted as `tamer::rotary::RotaryEncoder`, with a `NoopRotaryEncoder` mock shipped in the same change.
It has the five methods `update`, `position`, `set_position`, `reset`, and `is_button_pressed`, and is also re-exported at the `tamer` root and in `tamer::prelude`.
The implementors are `rustyfarian_esp_idf_peripherals::rotary::Encoder` and `rustyfarian_esp_hal_peripherals::rotary::InterruptEncoder`, both delegating to their unchanged inherent methods (an additive change).
The async esp-hal `rotary::Encoder` does not implement it, because its `position()` only advances while `wait_for_step` is polled, so behind the trait it would silently lose detents under load.
Diagnostics (`isr_count`, `edge_count`, `skipped_count`, `pin_*_is_high`) stay outside the trait.
