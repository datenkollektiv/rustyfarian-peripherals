# Feature: Interrupt-Driven Rotary Encoder v1

A production, interrupt-driven rotary encoder with a debounced push button for ESP-IDF — the esp-idf tier's first library driver beyond the `tamer` re-export.
The pure quadrature and button decoding logic lives in `tamer` (`QuadratureDecoder`, `ButtonDecoder`); this driver owns the hardware glue: persistent GPIO edge interrupts, an ISR that samples A/B and feeds the decoder, atomic position tracking, and polled button timing on a fixed-cadence `update()` call.

## Background

- **Why the esp-idf tier, not `tamer`.**
  `tamer` is pure `no_std`, zero-FFI, zero-globals.
  `QuadratureDecoder` is a stateless struct with no hardware coupling.
  This driver is entirely ESP-IDF lifecycle management — `gpio_install_isr_service`, `gpio_isr_handler_add`, persistent `AnyEdge` interrupts, ISR teardown on drop — and belongs in the hardware adapter tier.

- **Why raw FFI, not HAL subscriptions.**
  `esp-idf-hal`'s `PinDriver::subscribe` + `enable_interrupt` is a one-shot pattern: the handler auto-disables after the first fire and must be re-armed.
  A rotary encoder generates a burst of edges per detent — easily missed if the main loop is slow (display DMA, I2C read).
  The knob solved this with raw `esp-idf-sys` `gpio_isr_handler_add` for persistent, non-one-shot `AnyEdge` interrupts.
  See ADR-005 for the full rationale and why this is the sanctioned pattern for edge-dense inputs on the esp-idf tier.

- **Why per-instance ISR context, not module statics.**
  The knob kept state in module-level statics, forcing one encoder per process (silent corruption on a second `new()`).
  This driver redesigned for per-instance `Box<IsrContext>`, unblocking multiple encoders and eliminating the behavior-change trap.
  See ADR-006 for the decision and trade-offs.

## Decisions

|                                                                                                    Decision | Reason                                                                                                                                                             |
|------------------------------------------------------------------------------------------------------------:|:-------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|                                                  Land as `rustyfarian_esp_idf_peripherals::rotary::Encoder` | First library driver, extends the pure `tamer::rotary::QuadratureDecoder` + button logic to hardware                                                               |
|                                                        Persistent raw-FFI interrupts, not HAL subscriptions | HAL one-shot pattern loses edges under main-loop latency; encoder proof-of-concept failed this way; use raw `esp-idf-sys::gpio_isr_handler_add`                    |
|                                                          Per-instance `Box<IsrContext>`, not module statics | Unblocks multiple instances; avoids silent corruption trap that would break pre-1.0                                                                                |
|                                                    Concrete `Encoder<'d>` struct, trait-readiness by design | esp-hal twin coming soon; extract shared trait when it lands and shape is verified; don't guess now                                                                |
|                                                                 Synchronous `update(now_ms)` API, not async | Button timing inherently needs caller timestamp; open risk: esp-hal may prefer async `wait_for_any_edge().await`; trait extracted then with full shape clarity     |
|                                      Return `Option<ButtonEvent>` from `update()`, unmodified `tamer` types | Full event richness (raw `Press`/`Release` + layered `Click`/`DoubleClick`/`LongPress`); consumers collapse to app-level events; never leak encoder-specific enums |
|                                                                    `EncoderConfig` with `#[non_exhaustive]` | `steps_per_detent`, debounce/long-press/double-click timings; `Default` for EC11; future config fields not a breaking change                                       |
|                                                    `EncoderError::InvalidConfig` on `steps_per_detent == 0` | Prevents panic in `QuadratureDecoder::new`; a public config must not panic                                                                                         |
|                                                                  Robust ISR teardown on constructor failure | Idempotent `gpio_intr_disable` + `gpio_isr_handler_remove` in `Drop`, even if constructor failed partway; no dangling ISR left behind                              |

## Behavioral contract

The mapping the implementation and its tests must satisfy.

- **Quadrature decoding runs inside the ISR.** Every edge on A or B (both monitored via persistent `AnyEdge` interrupts) immediately samples both pins and feeds `QuadratureDecoder::update` inside a critical section. A confirmed detent steps the atomic position counter (Relaxed ordering, but Release/Acquire on the `armed` tombstone flag). No edge is lost regardless of main-loop latency.
- **Button timing runs in `update(now_ms)` (polled).** The caller supplies a monotonic millisecond timestamp and calls `update()` on a fixed cadence. The button decoder advances from the live pin read and returns a debounced event: `Press`, `Release`, `Click`, `DoubleClick`, or `LongPress`.
- **Position is lock-free and atomic.** `position()` returns an `AtomicI32` load (Relaxed) without blocking. `set_position()` and `reset()` update both the atomic counter and the decoder's internal accumulator (inside a critical section).
- **Constructor validates config before creating state.** `steps_per_detent == 0` is rejected with `EncoderError::InvalidConfig` before `QuadratureDecoder::new`, which would panic.
- **Constructor seeds decoders from live pin reads.** No phantom button press if the button is already held at construction (reads current level before seeding `ButtonDecoder`). Quadrature decoder seeded from the live A/B state before interrupts are armed.
- **Both interrupts arm only after both pins are fully armed.** The `armed` tombstone is `false` until both A and B are ready; the ISR's first check bails if `armed == false`, so any edge caught on A while B is being configured is safely dropped.
- **`Drop` is load-bearing and closes the use-after-free race on dual-core SoCs.** (1) Disable both pins' interrupts (no new edges). (2) Remove both pins' handlers (ESP-IDF stops dispatching). (3) Store `armed = false` (Release), then take a `critical_section::with(|_| {})` barrier — this ensures any ISR mid-flight on another core has exited its own critical section before the heap is freed. (4) The `Box<IsrContext>` is freed.
- **No `unsafe` in public methods.** All FFI calls are in private helpers (`arm_gpio_isr`, `disarm_gpio_isr`, `encoder_isr_c`), each with `// SAFETY:` comments.
- **`Send` / `Sync` are compiler-derived.** The `critical_section::Mutex<RefCell<QuadratureDecoder>>` + atomics + `Copy` fields satisfy the auto-traits; no hand-written `unsafe impl`.

## Constraints

- Pure ESP-IDF hardware lifecycle only; no `no_std` requirement (the tier is `std`).
- Interrupt-safe: no allocation in ISR, no clock reads, no blocking calls, no re-entrancy hazard.
- Per-instance heap context (`Box<IsrContext>`); pointer stability is load-bearing.
- The critical-section backend must be ISR-safe (verified: `critical-section = "=1.2.0"` on esp-idf-hal 0.46 uses a mutex-based impl that survives ISR preemption). Pins exactly to avoid future regress.
- No trait definition yet (ADR-006 flags the trait extraction as future work when the esp-hal twin lands).

## Module & API surface (v1)

`crates/rustyfarian-esp-idf-peripherals/src/rotary.rs`:

- `EncoderConfig` — `#[non_exhaustive]` struct with `steps_per_detent: u8`, `debounce_ms`, `long_press_ms`, `double_click_ms: u64`; `Default` (EC11: 4 steps, 50/1000/300 ms).
- `Encoder<'d>` — concrete struct taking three pins (A, B, button) via `new()` or `new_with_config(config)`.
- Methods: `update(now_ms: u64) -> Option<ButtonEvent>`, `position()`, `set_position`, `reset`, `is_button_pressed`, `isr_count`, `pin_a_is_high`, `pin_b_is_high`.
- `EncoderError` — `#[non_exhaustive]` carrying `InvalidConfig`, `Pin(EspError)`, `Isr { call, pin, code }`.
- Re-exports from `tamer`: `ButtonEvent`, `EncoderDirection`.
- `#[must_use]` on `new`, `new_with_config`, `position`, `is_button_pressed`, `isr_count`, pin level getters (not `update`).

## Required tests (host / device)

**Host (unit tests in the rust crate):**

- Constructor rejects `steps_per_detent == 0` with `InvalidConfig`.
- Constructor succeeds and arms both interrupts on valid config.
- `position()` reads correctly before any event.
- `set_position()` overwrites the position and resets the decoder accumulator.
- `reset()` zeroes position.
- `is_button_pressed()` reflects debounced state.

**Device (on hardware, e.g., ESP32-S3):**

- CW and CCW rotation increments/decrements `position()` correctly across detent boundaries.
- `isr_count` accumulates continuously across fast spins (proves one-shot regression did not occur).
- Button press/release produces `Press`/`Release` events.
- Long-press (hold > 1000 ms) produces `LongPress`.
- Double-click (two clicks within 300 ms) produces `DoubleClick`.
- Constructor failure (invalid pin, ISR service failure) leaves no dangling ISR (verify with a second `new()` succeeding).

## Deferred (explicitly decided — not open)

- **Trait definition** → extracted when the esp-hal twin lands and its shape (async vs sync) is verified.
- **IRAM-safe ISR** (v1 limitation) → roadmapped separately (see `docs/features/iram-safe-isr-v1.md`).
- **ESP32-C3 variant** → S3 example first (target of the donation); C3 added if a second consumer needs it.

## Resolved

- **Per-instance context over module statics** → confirmed (per ADR-006).
- **Concrete struct over trait** → confirmed; trait extracted later when esp-hal shape is proven.
- **Persistent FFI over HAL subscriptions** → confirmed (per ADR-005).
- **Return ButtonEvent, not knob's EncoderEvent** → confirmed (clean seam between tier and app).

## State

- [x] Design approved (plan: merry-spinning-swing.md)
- [x] Per-instance ISR context core (IsrContext, Drop ordering, critical-section barrier)
- [x] Public API surface (EncoderConfig, Encoder<'d>, EncoderError)
- [x] Constructor validation (steps_per_detent == 0 rejection before panic)
- [x] Robust ISR teardown on constructor failure (idempotent disarm on partial arm)
- [x] Dependencies promoted and pinned (esp-idf-hal = "=0.46.2", esp-idf-svc = "=0.52.1", esp-idf-sys = "=0.37.2", embuild = "=0.33.1", critical-section = "=1.2.0")
- [x] Decode/timing logic host-tested upstream in `tamer` (`QuadratureDecoder`, `ButtonDecoder`); the driver itself is device-only and cannot be host-compiled (raw ESP-IDF FFI), so it carries no host tests of its own
- [x] Landed in rust crate with full `# Errors`, `# Safety`, `# Panics` rustdoc
- [x] Device-target compile verified: `just check-idf` (C3 lib) and the `idf_s3_rotary` example (xtensa-esp32s3-espidf) both build clean, 0 warnings — confirms the bindgen FFI symbols and esp-idf-hal 0.46 API resolve against real ESP-IDF headers
- [x] Link + flash verified on real ESP32-S3: `just flash idf_s3_rotary` links and flashes cleanly. NOTE: this surfaced a link-time bug that `check` could not — the `critical-section` impl provider (`esp-idf-svc`'s `critical-section` feature) was dropped when the dep was promoted to `workspace = true`; re-added on the crate. See `docs/project-lore.md`.
- [x] On-hardware behavioral verification (ESP32-S3, `just run idf_s3_rotary`, 2026-07-15): clean boot/init (both A/B ISRs armed, "encoder ready"), CW/CCW detent counting correct and symmetric, no lost edges across fast spins (one-shot-trap regression gone), and `Press`/`Release`/`Click`/`DoubleClick` all fire correctly. `LongPress` uses the same host-tested `ButtonDecoder` path and was not separately exercised in the capture. Constructor-failure teardown not exercised on hardware (host/logic reasoning + review only).
- [x] ADR-005 (raw FFI persistent interrupts pattern) written
- [x] ADR-006 (per-instance context + trait-readiness) written
- [x] Feature doc this file created
- [x] CHANGELOG [Unreleased] updated
- [x] ROADMAP updated (move to Done, add IRAM follow-up)
- [x] project-lore.md findings recorded
- [x] lib.rs reframing (interrupt patterns documented)
- [ ] esp-hal twin landing (async shape decision, trait extraction — future session)
- [ ] IRAM-safe ISR v2 (follow-up feature)

## Session Log

- 2026-07-15 — an Interrupt encoder driver landed in esp-idf tier (per-instance Box<IsrContext>, persistent raw-FFI AnyEdge interrupts, polled button timing, robust ISR teardown). Donation from `rustyfarian-knob` hardware-proven on CrowPanel ESP32-S3. ADR-005 and ADR-006 written (raw FFI pattern and instance/API shape decisions). Feature doc created. CHANGELOG + ROADMAP + README + project-lore updates.
  Status: implementation complete; on-hardware verification pending at time of writing.
- 2026-07-15 — Hardware bring-up on ESP32-S3: `just run idf_s3_rotary` first failed at **link** with `undefined reference to _critical_section_1_0_acquire/_release` — the `critical-section` impl provider (`esp-idf-svc`'s `critical-section` feature) was dropped when the dep moved to `workspace = true`. `cargo check`/`check-idf` could not catch it (no link step). Fixed by `esp-idf-svc = { workspace = true, features = ["critical-section"] }`; recorded in `docs/project-lore.md`. After the fix the example runs on device: clean boot, symmetric CW/CCW counting with no lost edges, and Press/Release/Click/DoubleClick events.
  Status: **Feature complete and hardware-verified on ESP32-S3.** Trait extraction deferred until esp-hal twin lands.
