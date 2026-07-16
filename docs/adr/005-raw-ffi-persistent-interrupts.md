# ADR-005: Raw FFI persistent interrupts for edge-dense inputs

## Status
Accepted

## Context
`rustyfarian-esp-idf-peripherals` is the ESP-IDF `std` hardware tier where drivers bind pure `tamer` logic to real ESP32 GPIO.
Its skeleton assumed interrupt-driven inputs would use `esp-idf-hal`'s `PinDriver::subscribe` + `enable_interrupt` pattern — a built-in async-notification abstraction where the handler fires once and auto-disables, requiring explicit re-arming after each edge.

The first downstream driver — an interrupt-driven rotary encoder — exposed a fatal gap: `PinDriver`'s one-shot subscription pattern cannot keep up with edge-dense inputs.
A rotary encoder generates a burst of quadrature transitions per physical detent.
If the main-loop tick rate is coarse (e.g., a long display DMA transfer or I2C read blocks the scheduler), edges arriving between ticks are lost.
The one-shot pattern forces a re-arm after every edge, a race condition where a fast encoder can produce more edges than fit in one polling interval.

The encoder driver sidesteps this by calling the underlying ESP-IDF C API directly: `gpio_isr_handler_add` (via `esp_idf_svc::sys`) registers a persistent handler that stays armed after every edge, so no edge is lost regardless of main-loop latency.
This correction inverts the skeleton's implicit assumption that polled, one-shot HAL subscriptions are the universal solution for interrupt-driven input.

## Decision
Accept raw-FFI persistent `AnyEdge` interrupts (`gpio_install_isr_service`, `gpio_isr_handler_add`, `gpio_intr_enable` / `disable`) via `esp_idf_svc::sys` as the **sanctioned pattern for edge-dense inputs** in the ESP-IDF tier.

This is distinct from polled one-shot HAL subscriptions: choose **polled one-shot** for low-frequency, one-edge-at-a-time signals (e.g. waking on a button press) and **persistent raw-FFI** for edge-dense streams (e.g. quadrature rotation).
Default to poll (no FFI required) unless a downstream project has an established, hardware-verified need for zero event loss under load — that is a deliberately high bar.

## Consequences
- Edge-dense inputs (rotary encoders, stepper feedback, quadrature distance sensors) have a correct, non-losing implementation path rather than silently dropping edges under load.
- The encoder driver correctly defines the per-instance ISR context using a `Box<IsrContext>` (stable heap address, passed to the FFI as a `*mut c_void` argument) and a load-bearing `Drop` implementation with a critical-section barrier to prevent use-after-free races against ISRs mid-flight on other cores.
- Raw FFI calls are encapsulated in private helpers (`arm_gpio_isr`, `disarm_gpio_isr`) with `// SAFETY:` comments; the public API exports zero `unsafe`.
- **Link-time contract:** the ISR synchronizes shared state through `critical_section::with`, which is an *interface* — some crate must supply the `critical-section` **implementation**. The tier gets it from `esp-idf-svc`'s `critical-section` feature (`esp-idf-svc = { workspace = true, features = ["critical-section"] }`), which provides a FreeRTOS-backed, ISR-safe backend. This is a hard build requirement of choosing the raw-FFI + `critical_section` path: without a provider, linking fails with `undefined reference to _critical_section_1_0_acquire/_release`. It is easy to miss because `cargo check` (and thus `just check-idf`) does **not** link — only a real device build/flash surfaces it — and because promoting the dep to `workspace = true` silently drops per-crate `features`. Verify with `just flash`, not `check-idf` alone. See `docs/project-lore.md`.
- Documentation must distinguish the two patterns clearly (see [`rustyfarian_esp_idf_peripherals`](../../crates/rustyfarian-esp-idf-peripherals/src/lib.rs) module docs).
- **Operational limitation (v1): the ISR is not IRAM-resident** — an edge during a flash-cache-disabled window (NVS/OTA write) can crash the device. Roadmapped fix and full rationale: [`docs/features/iram-safe-isr-v1.md`](../features/iram-safe-isr-v1.md).
- This is a **deliberate departure** from `AGENTS.md`'s guideline "every hardware interaction is behind a trait" — the raw-FFI path is too specific to the ESP-IDF C API to be usefully trait-ified without losing the whole point (persistent edge capture). A trait-first polled adapter (`QuadratureInput` in `tamer::rotary` behind the `hal` feature) complements the raw-FFI driver; users pick the right tool for the latency budget.

## Alternatives Considered
|                                                        Alternative | Pros                                                     | Cons                                                                                                 | Why Rejected                                                                                              |
|-------------------------------------------------------------------:|:---------------------------------------------------------|:-----------------------------------------------------------------------------------------------------|:----------------------------------------------------------------------------------------------------------|
|                       Polled one-shot HAL subscriptions everywhere | Keeps all interrupt logic inside the HAL; trait-friendly | Loses edges under main-loop latency; unusable for quadrature; the whole encoder problem persists     | The encoder proof-of-concept already failed this way; it is not viable for the target use case            |
|       Polling `pin.is_high()` / `is_low()` from the main loop only | Pure, no ISR; minimal coupling                           | Requires a very fast tick rate to not miss edges; defeats low-power interrupt benefit                | Acceptable for slow inputs (buttons), not for encoders; forces a one-tool-per-latency decision either way |
|   Trait-ified FFI (async `wait_for_edge().await`, `Handler` trait) | Reusable, composable, matches AGENTS.md guidance         | Speculative without a proven async consumer; preempts the esp-hal twin's async shape; vendor lock-in | The esp-idf tier is `std` and *can* support async, but that shape is decided when the esp-hal twin lands  |
|                 Continue with the one-shot subscription assumption | Minimal skeleton change                                  | Encoder silently loses edges; no way for a downstream project to discover the bug except on hardware | Blocks the first real driver and misleads future consumers about what "interrupt driven" means            |
