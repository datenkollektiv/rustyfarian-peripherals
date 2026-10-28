# Feature: IRAM-Safe Interrupt Handler v1

**Status:** Designed, not started — deferred follow-up to [interrupt-driven-encoder-v1](archive/interrupt-driven-encoder-v1.md).
Trigger for implementation: a consumer combines the ESP-IDF `Encoder` with flash writes (the knob adopting OTA, or the clock / beekeeper adopting the encoder).

## Problem

The ISR in `rustyfarian_esp_idf_peripherals::rotary::Encoder` is **not IRAM-resident**, and the driver installs the GPIO ISR service with flags `0` (no `ESP_INTR_FLAG_IRAM`).

Actual behaviour today (verified against ESP-IDF v5.3.2 and esp-idf-hal 0.47.0, 2026-10-24):

- ESP-IDF calls `esp_intr_noniram_disable()` before every flash-cache-off window (NVS write, OTA sector erase/write, SPIFFS).
  Non-IRAM interrupts — including ours — are masked for the duration.
- The GPIO status bit latches, so the handler runs once the cache is back; several edges in one window coalesce into a single sample.
  The decoder sees a skipped quadrature state and drops or misreads a detent.
  **This is a glitch, not a crash.**
- It **does** crash if other code installs the shared GPIO ISR service with `ESP_INTR_FLAG_IRAM` first.
  The flag is process-global and first-caller-wins; our flash-resident trampoline would then run with the cache off.
  esp-idf-hal defaults the flag to `0`, so nothing in the current ecosystem triggers this.

Flash-resident pieces on the current ISR path, each of which must go before the IRAM flag can be set:

| Piece                                    | Where it lives                                      | Why it is flash-resident                                                                |
|:-----------------------------------------|:----------------------------------------------------|:----------------------------------------------------------------------------------------|
| `encoder_isr_trampoline` / `encoder_isr` | esp-idf tier `.text`                                | No `#[link_section]`.                                                                   |
| `QuadratureDecoder::update`              | `tamer` `.text`                                     | Monomorphic, not `#[inline(always)]`.                                                   |
| `QUAD_TABLE`                             | `tamer` `.rodata` → flash DROM                      | `static` lookup table.                                                                  |
| `gpio_get_level`                         | ESP-IDF `esp_driver_gpio`                           | Plain function; `CONFIG_GPIO_CTRL_FUNC_IN_IRAM` does not cover `get_level`.             |
| `IsrLocked::lock`                        | esp-idf-hal `IsrCriticalSection` (since 2026-10-28) | Inlined `enter`/`exit` wrappers land in flash `encoder_isr`; the port callees are IRAM. |

## Scope

Deliver an **opt-in** IRAM-safe mode for the ESP-IDF `Encoder` that keeps the handler and everything it touches in on-chip SRAM.

### Behavioral contract

- Default construction is unchanged: non-IRAM, deferred-and-coalesced edges during flash writes.
- With IRAM mode enabled, edges are serviced while the flash cache is off; no detent is dropped during NVS or OTA writes.
- IRAM mode never enables `ESP_INTR_FLAG_IRAM` behind the application's back.
  The application opts in explicitly and thereby accepts that *every* GPIO handler in the firmware must be IRAM-safe.
- Position reads, `set_position`, `reset`, and `Drop` keep their current semantics.

### Required changes

1. **ISR placement** — tag `encoder_isr_trampoline` and `encoder_isr` with `#[link_section = ".iram1.<name>"]` (esp-idf-hal's own precedent), and make every callee `#[inline(always)]`.
2. **Arithmetic decode in `tamer`** — replace the `QUAD_TABLE` lookup with bit arithmetic on `(prev, next)` so there is no `.rodata` to relocate and `tamer` stays placement-agnostic.
   Mark `QuadratureDecoder::update` `#[inline(always)]`.
   Host tests must prove the arithmetic form is bit-for-bit equivalent to the table over all 16 transitions.
3. **Direct register read** — replace `gpio_get_level` with a volatile read of `GPIO_IN_REG` / `GPIO_IN1_REG` (constants come from `esp-idf-sys`).
   Pins ≥ 32 on ESP32 / S3 live in `IN1`; C3 / C6 have a single register.
   Pre-compute the register address and bit mask at construction, store them in `IsrContext`.
4. **No `critical_section` in the ISR** — the ISR becomes the sole owner of the decoder (`UnsafeCell`), publishing via atomics only.
   `set_position` / `reset` request the change through an atomic the ISR consumes.
   `Drop` replaces the critical-section barrier with: disable both pin interrupts, then spin on an `in_isr` atomic until clear, then remove the handlers.
   This retires the `RefCell` panic path as well.
5. **Install flag** — in IRAM mode pass `ESP_INTR_FLAG_IRAM` to `gpio_install_isr_service`.
   If the call returns `ESP_ERR_INVALID_STATE` (service already installed by someone else), surface a distinct error rather than silently running non-IRAM.
6. **Context memory** — `IsrContext` stays a small `Box`; on S3 with PSRAM it is well under `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL`, so it lands in internal DRAM.
   Document this; do not add a `heap_caps` dependency.
7. **Placement check** — a device-build `just` recipe that reads the symbol table of the built example, asserts the trampoline address is inside the chip's IRAM range, and fails on any relocation from the ISR into flash.
   A host-only lint is not possible; this is the honest substitute.

### Required tests

- Host: arithmetic decoder equivalence over all 16 `(prev, next)` pairs, both directions, plus the existing rotary suite unchanged.
- Host: `set_position` request/consume handshake via a mocked ISR tick.
- Device (S3 and C3): placement recipe passes; encoder survives a tight NVS write loop and an OTA download with no dropped detents and no reset.

### Deferred

- esp-hal twin: bare-metal has no flash-cache-off masking of the same shape; revisit only if a consumer reports it.
- Shared encoder trait (ADR-006) is independent and should land first if both are in flight.

## Decisions

- **Opt-in, not always-on.** The IRAM flag is global and binds every other GPIO handler in the firmware; only the application can make that promise.
- **`tamer` owns the decode change, the tier owns placement.** Arithmetic decode removes the data-placement question entirely and keeps `tamer` pure; link sections and register reads are tier concerns.
- **Drop the critical section from the ISR rather than relocate it.** The esp-idf-hal wrappers are outside our control and a single-owner design is simpler to reason about.

## State

- [x] Design finalized (opt-in; arithmetic decode; direct register read; no `critical_section` in the ISR)
- [ ] `tamer` arithmetic decode + `#[inline(always)]` with equivalence tests
- [ ] ISR `.iram1` placement, register read, single-owner decoder, new `Drop` barrier
- [ ] `ESP_INTR_FLAG_IRAM` opt-in + distinct already-installed error
- [ ] Device placement-check recipe
- [ ] Device tests (NVS write loop, OTA download) on S3 and C3
- [ ] README caveat removed for IRAM mode; CHANGELOG; patch release of `rustyfarian-esp-idf-peripherals`

## Session Log

- 2026-07-15 — Skeleton created as deferred follow-up to interrupt-driven-encoder-v1. Problem statement (cache-off crash), scope (IRAM tags, QUAD_TABLE relocation, ESP_INTR_FLAG_IRAM), constraints, and open decisions recorded. Marked as demand-driven follow-up (when a consumer needs OTA without encoder glitch).
- 2026-10-24 — Ecosystem sweep flagged this as urgent for the clock and beekeeper. Verified against ESP-IDF v5.3.2 / esp-idf-hal 0.47.0: with flags `0` the failure is a deferred, coalesced edge (dropped detent), not a crash; the crash needs a foreign `ESP_INTR_FLAG_IRAM` install. Neither the clock nor the beekeeper consumes the ESP-IDF encoder; only the knob does, and it performs no flash writes. Inventoried every flash-resident piece on the ISR path (`gpio_get_level`, `critical_section` wrappers, `QUAD_TABLE`, `tamer` `update`). Settled the open decisions (opt-in; arithmetic decode; no critical section in the ISR). Corrected README, rustdoc, and ROADMAP wording. Implementation stays Mid term until a consumer combines the encoder with OTA / NVS.
