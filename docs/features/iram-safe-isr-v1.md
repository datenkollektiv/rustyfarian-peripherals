# Feature: IRAM-Safe Interrupt Handler v1 (Skeleton)

**Status:** Skeleton only — deferred follow-up to interrupt-driven-encoder-v1.

## Problem

The ISR in `rustyfarian_esp_idf_peripherals::rotary::Encoder` is **not IRAM-resident**.
An edge on the quadrature pins that arrives during a flash-cache-disabled window (NVS write, OTA update, SPIFFS operation) cannot be serviced and can crash the device.

The handler itself (`encoder_isr_trampoline`) lives in flash `.text`, and the lookup table it reads (`tamer::rotary::QUAD_TABLE`) lives in flash `.rodata`.
When ESP-IDF disables the cache to write flash, both are unreachable.

## Scope Sketch

Deliver an IRAM-safe variant that keeps the handler and its data in on-chip SRAM, safe to execute while the flash cache is off.

**Required changes:**

- Tag `encoder_isr_trampoline` with `#[link_section = ".iram0.text"]` and any inlined helpers.
- Place or reference `tamer::rotary::QUAD_TABLE` in IRAM (either `#[link_section = ".iram0.data"]` on the `tamer` side, or copy it into SRAM at startup on the esp-idf side).
- Use `ESP_INTR_FLAG_IRAM` flag in `gpio_install_isr_service()` if available on the target chip.

**Constraints:**

- IRAM is limited (~96 kB on ESP32, ~160 kB on S3); the handler and table must fit.
- The encoder's heap-allocated `IsrContext` stays in DRAM (fine; the ISR only reads its atomics, never allocates).
- `critical_section` calls inside the ISR must use an IRAM-safe backend (verify with the pinned `critical-section` version).

## Open Decisions

- Should IRAM-safety be a compile-time feature flag (opt-in, saves flash/SRAM for users who never do OTA), or always-on for the encoder?
- Which side owns the `QUAD_TABLE` placement — `tamer` (generic, but adds an IRAM feature to the core) or the esp-idf tier (specific, but keeps `tamer` pure)?

## State

- [ ] Design finalized (feature flag vs always-on; table placement ownership)
- [ ] IRAM section tagging + QUAD_TABLE relocation
- [ ] `ESP_INTR_FLAG_IRAM` integration in `gpio_install_isr_service`
- [ ] critical-section backend verification (ISR-safe under flash-cache-off)
- [ ] Host tests (no regression; IRAM layout tools)
- [ ] Device tests (interrupt serviced correctly during flash operation — NVS write, OTA)
- [ ] CHANGELOG + documentation

## Session Log

- 2026-07-15 — Skeleton created as deferred follow-up to interrupt-driven-encoder-v1. Problem statement (cache-off crash), scope (IRAM tags, QUAD_TABLE relocation, ESP_INTR_FLAG_IRAM), constraints, and open decisions recorded. Marked as demand-driven follow-up (when a consumer needs OTA without encoder glitch).
