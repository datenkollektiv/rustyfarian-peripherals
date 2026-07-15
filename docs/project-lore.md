# Project Lore

This file records non-obvious technical discoveries: facts that caused surprising
failures, took significant time to debug, or would save a future developer 30+
minutes if known upfront.

Organise entries by topic. Keep each entry to a short paragraph: the surprising
symptom first, then the cause, then the fix.

No entries yet — this is a fresh skeleton. Add the first lore the moment a
hardware quirk or build gotcha costs you real time (bouncing-contact timings,
encoder detent alignment, pull-resistor surprises, esp-hal vs esp-idf GPIO
differences, linker glue, …).

---

## Build & Validation

**`just deny` can fail on `advisories` for crates your change never touched — suspect the esp-idf tier's build-dependency chain, and note there is no committed `Cargo.lock` to diff against (it is gitignored).**
Symptom: `just deny` (and CI's `deny` step) reports `advisories FAILED` while `licenses`/`bans`/`sources` pass, flagging crates like `crossbeam-epoch` or an `anyhow`-family `Error::downcast_mut` unsoundness that appear nowhere in your diff.
Cause: RUSTSEC advisories are published continuously, and `cargo deny check advisories` resolves them against the *current* dependency graph, not your change. The esp-idf tier pulls a deep build-dep chain (`esp-idf-hal → embuild → globwalk → ignore → crossbeam-deque → crossbeam-epoch`, plus `anyhow`) that periodically acquires fresh advisories. Because `Cargo.lock` is gitignored, there is no committed lock proving "this already failed on main" — confirm instead that the flagged crate sits in the esp-idf chain (grep `Cargo.lock` / `cargo tree`) and is absent from your change's own new dependencies.
Fix/triage: treat it as pre-existing dependency-hygiene work, separate from a feature PR — do **not** silently add a `deny.toml` ignore just to green a feature branch. Remediate deliberately: `cargo update -p <crate>` to the patched version named in the advisory's `Solution`, or a dated, commented `deny.toml` ignore if the exact-pinned esp stack blocks the upgrade, coordinated with the sibling repos' esp pins.
Confirmed 2026-07 during the `tamer::mpu6050` PR: RUSTSEC-2026-0204 (`crossbeam-epoch` 0.9.18 → upgrade ≥ 0.9.20) and RUSTSEC-2026-0190 (`downcast_mut` unsoundness) surfaced, while that PR's only new dependency — `micromath`, zero transitive deps — was advisory-clean and passed `deny` on licenses/bans/sources.

**`just check-idf` (`cargo check`) does not link, so it cannot catch a missing `critical-section` implementation — only a real device build/flash does.**
Symptom: `just check-idf` passes clean, but `just run`/`just flash` fails at link with `undefined reference to _critical_section_1_0_acquire` / `_critical_section_1_0_release` for the encoder ISR's `critical_section::with`.
Cause: two compounding blind spots. (1) `cargo check` type-checks but skips linking, so missing-symbol errors are invisible to the fast gate — the `critical-section` crate provides only the *API* (`Mutex`, `with`); some crate must provide the *implementation* via a feature. (2) The donor enabled it with `esp-idf-svc = { version = "0.52", features = ["critical-section"] }`, but promoting the dep to `esp-idf-svc = { workspace = true }` silently dropped that per-crate feature (a `workspace = true` dep inherits the version, not the consuming crate's old `features`).
Fix: re-declare the feature on the consuming crate — `esp-idf-svc = { workspace = true, features = ["critical-section"] }` — so esp-idf-svc supplies the FreeRTOS-backed, ISR-safe impl. Never report "device-verified" from `check-idf` alone; run `just flash` to confirm the link.
Confirmed 2026-07 on ESP32-S3 (`idf_s3_rotary`): `check-idf` green, `just flash` failed to link, and adding the feature made the release build link and flash cleanly. See [[cargo-check-doesnt-link-critical-section]].

## Hardware — inputs

**KY-003 and "Hall sensor" marketplace labels are ambiguous — verify the actual chip before modeling behavior.**
KY-003 modules in the market mix two very different sensors that share similar physical form: the A3144 (`3144xUA-S`) is a unipolar, open-collector, Schmitt-trigger *digital* switch (idles HIGH via onboard pull-up, snaps LOW on a single magnetic pole only, ignoring the opposite pole), while industrial specs like SS49E (`49E`) or AH477 (`AH477`) are linear *analog* sensors that idle near VCC/2, output proportional voltage across both poles, and require ADC + threshold logic.
Reading an A3144 via ADC and `tamer::hall::HallSensor` (which assumes linear behavior and symmetric bipolar response) produces the wrong abstraction: the sensor's single-pole output idles near 4095 (12-bit maximum), one polarity detects cleanly, and the opposite polarity clips to saturation because the sensor never drives below ~VCC/2.
Fix: Always check the physical chip marking (look for `3144EUA`, `3144LUA`, `SS49E`, `AH477`, etc. on the TO-92 package). For A3144 KY-003 modules, read via `tamer::presence::DigitalPresence` (ActiveLow, 20 ms debounce) as a digital switch, not the `tamer::hall` + ADC linear model.
Confirmed on ESP32-C3 test board: KY-003 module shipped with an A3144; switched the example from ADC + `SlidingAverage` + linear Hall model to GPIO + `DigitalPresence`, and the sensor's single-pole and no-magnet states became cleanly distinguishable.

## ESP-IDF — console output

**ESP-IDF (std) code that prints with bare `println!` produces no serial output from inside an infinite loop.**
`println!` writes to a buffered newlib `stdout` that is never flushed in a non-terminating loop, so the lines sit in the buffer forever; the ESP-IDF boot log and `esp_log` output bypass that buffer (direct UART), which is why only the `I (…)` boot lines reach the monitor and the program's own first line never does (the log stops dead at `Calling app_main()`).
Fix: call `esp_idf_svc::log::EspLogger::initialize_default()` once at startup and use `log::info!` instead of `println!` — `esp_log` is unbuffered, the same path the boot lines use. This matches the convention every idf example in the sibling `rustyfarian-network` repo already follows.
Confirmed on ESP32-C3 with `idf_c3_b3f`: after the switch, the `B3F button ready` line and the press/release events appear immediately.

## esp-hal vs esp-idf — ADC raw floor

**A fixed-range ADC→output map (`RangeMap::new(0, 4095, …)`) reaches the rails on the ESP-IDF tier but not on esp-hal, because the two stacks floor the raw reading differently.**
Symptom: the identical `hal_c3_poti_led` / `idf_c3_poti_led` dimmer (potentiometer → `tamer::range_map::RangeMap` → LEDC PWM LED) drove the LED to full dark *and* full bright on the ESP-IDF tier, but on the esp-hal tier the LED never fully darkened or reached full brightness — it stayed in a compressed mid-range.
Cause: `esp-idf-hal`'s ADC oneshot driver applies the chip's factory (efuse) ADC calibration, so its raw reading reaches ~0 at the low rail and near full-scale at the high rail. `esp-hal`'s `read_oneshot` returns the *uncalibrated* SAR value when no `AdcCal*` scheme is attached; on the C3 that floors well above 0 (~100–200 counts) and ceilings below 4095, so a map hard-wired to `0..=4095` never reaches its input endpoints (`in_min`→duty 0, `in_max`→duty 255). This is the same ESP32 SAR ADC non-linearity noted under *Hardware — inputs*, surfacing as a **tier discrepancy** rather than an outright failure.
Fix: never hard-code the ADC span for a control that must reach its output rails. Run an `AnalogCalibration` startup sweep and build the map from the observed range (`RangeMap::new(range.min(), range.max(), 0, 255)`); it self-adjusts to whatever floor/ceiling each stack produces, and both tiers then reach both rails. (Alternatively, attach esp-hal's `AdcCalCurve`/`AdcCalLine` scheme to lower the hal floor — but calibration is tier-agnostic and matches the `hal_c3_poti` precedent.)
Confirmed on ESP32-C3-DevKitM-1 with the `poti_led` twin: after adding the calibration sweep, both the esp-hal and esp-idf examples sweep fully dark → full bright.

## esp-hal — console output

**`esp-println` configured for `jtag-serial` is invisible on a board that only exposes its UART bridge.**
The `jtag-serial` transport routes output to the chip's internal USB Serial/JTAG controller (the native USB port), not UART0; a dev board wired through a UART-bridge chip (`/dev/cu.usbserial-*`, e.g. CP210x) with no native-USB connection shows the boot log (ROM/bootloader use UART0) but never the app's `esp-println` output. The symptom is identical to a dead program: boot lines appear, the app's first `println!` does not.
Fix: select `esp-println/uart` for the chip so output goes to UART0 and lands on the same `usbserial` bridge used to flash — or connect the board's native USB port and monitor the `/dev/cu.usbmodem*` device it enumerates. The C3/C6 hal examples here use `uart`; `rustyfarian-network`'s hal tier uses `jtag-serial` (it assumes the native-USB port).
Confirmed on ESP32-C3 with `hal_c3_b3f`: after switching to `uart`, output appears on `/dev/cu.usbserial-210`.

## ESP-IDF — interrupt subscriptions vs. persistent interrupts

**`PinDriver::subscribe` + `enable_interrupt` (HAL pattern) is one-shot and unsuitable for edge-dense inputs like rotary encoders.**
Symptom: a quadrature encoder loses edges under main-loop latency (long display DMA, I2C read) — the handler fires once after the first edge, auto-disables, and must be explicitly re-armed.
A fast encoder generates a burst of edges per detent; if the main loop is slow, edges arrive between polls and are lost, even though the CPU and ISR are idle and perfectly capable of servicing them.
The knob's interrupt-driven encoder proof-of-concept failed this way before switching to raw `esp-idf-sys::gpio_isr_handler_add` (persistent `AnyEdge`, never auto-disables).
Cause: `esp-idf-hal` 0.46's `PinDriver` interrupt subscriptions are a convenience abstraction designed for low-frequency, one-edge-at-a-time signals (e.g., waking on a button press).
The HAL's abstraction does not expose the persistent interrupt registration that the underlying ESP-IDF C API provides.
Fix: For edge-dense inputs, bypass the HAL's subscription API and call `gpio_isr_handler_add` directly (via `esp_idf_svc::sys`) with persistent `AnyEdge` interrupts; see `rustyfarian_esp_idf_peripherals::rotary::Encoder` and ADR-005.
Confirmed 2026-07 during the interrupt-encoder donation: encoder silently dropped half the detents per second under a realistic main-loop load; raw FFI persistent interrupts captured every edge.

## ESP-IDF — critical-section backend ISR safety

**The critical-section backend in use on ESP-IDF must be ISR-safe and survive preemption from another core.**
Symptom: accessing a `critical_section::Mutex` from interrupt context and expecting it to block higher-priority ISRs risks a deadlock or panic if the wrong backend is active.
The esp-idf tier's interrupt-driven encoder wraps its decoder in a `critical_section::Mutex<RefCell<QuadratureDecoder>>`, taken from ISR context on every edge.
A task-only critical section (spinlock) or one that does not disable interrupts would assert/crash when an ISR tries to acquire it.
Cause: `critical-section` crate supports pluggable backends; the default is platform-independent but may not be ISR-safe.
The esp-idf-hal crate selects a mutex-based backend that disables interrupts during the critical section, safe for ISR preemption.
Fix: Pin the `critical-section` version (`= "=1.2.0"` exact) and verify the backend in use for the target ESP-IDF version (currently confirmed safe on `esp-idf-hal` 0.46 with `critical-section` 1.2).
Log the pinning in the crate's dependency comments so a future esp-idf-hal upgrade does not silently break the ISR contract.
Confirmed 2026-07 during the encoder driver review: `critical-section` 1.2.0 on esp-idf-hal 0.46 uses a mutex backend that is ISR-safe; pinned in the workspace `[workspace.dependencies]` and consumed by `crates/rustyfarian-esp-idf-peripherals/Cargo.toml`.
