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

**Residual risk (needs device confirmation):** the barrier only synchronizes with an ISR invocation that has *already entered* its critical section when `drop` makes its own `critical_section::with` call. It does not, by itself, prove that an invocation dispatched a few instructions earlier — after the hardware edge fired but before `encoder_isr` reaches its critical section — cannot race a subsequent free. That window is a handful of instructions (trampoline pointer cast + call). ESP-IDF's shared GPIO ISR service is *expected* to serialize `gpio_isr_handler_remove` against any in-flight dispatch on another core, which would make step 2 alone sufficient and the barrier pure defense-in-depth — but this has not been confirmed against ESP-IDF's internal locking. Verify by reading the IDF GPIO driver source for this wave, or by a construct/drop stress test while another core spins the encoder, before relying on this in a safety-critical context. The `rotary` module's `Drop` doc points here rather than restating this proof.

## Alternatives Considered
|                                              Alternative | Pros                                                          | Cons                                                                                                                       | Why Rejected                                                                                                             |
|---------------------------------------------------------:|:--------------------------------------------------------------|:---------------------------------------------------------------------------------------------------------------------------|:-------------------------------------------------------------------------------------------------------------------------|
|            Keep module statics (lift-and-shift the knob) | Minimal change; code is proven on hardware                    | Implicit one-instance limit; silent corruption on second `new()`; expensive to reverse pre-1.0                             | The constraint is easy to break now, expensive to fix later; unacceptable for a reusable crate                           |
|       Trait-first, extract now (guess the esp-hal shape) | Satisfies AGENTS.md; gives a unified API surface immediately  | Speculative without esp-hal twin; preempts its design; the async-vs-sync question is open                                  | esp-hal's deadline is soon, not now; guessing inverts the demand-driven principle                                        |
|   Async trait from day one (async `wait_for_any_edge()`) | If esp-hal also chooses async, the trait is correct from v1   | This tier is `std` and can do async, but the esp-hal tier can *also* do async in `std` mode; guessing is still speculative | Same: defer until the esp-hal twin lands and its choice (async or sync) is verified on hardware                          |
|      Per-instance static `[Option<IsrContext>; N]` array | Avoids heap allocation (no `Box`)                             | Requires a compile-time encoder count; uses stack space for unused slots; still needs a lifetime-safe pointer scheme       | Rigid and less flexible than heap allocation; does not simplify the ISR context pointer handoff                          |
