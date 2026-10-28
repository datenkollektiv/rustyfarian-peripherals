# Project Lore

This file records non-obvious technical discoveries: facts that caused surprising
failures, took significant time to debug, or would save a future developer 30+
minutes if known upfront.

Organise entries by topic. Keep each entry to a short paragraph: the surprising
symptom first, then the cause, then the fix.

---

## Build & Validation

**`just deny` can fail on `advisories` for crates your change never touched — suspect the esp-idf tier's deep build-dep chain, and note `Cargo.lock` is gitignored so there's no committed lock to diff against.**
Symptom: `deny` reports `advisories FAILED` (licenses/bans/sources pass), flagging crates like `crossbeam-epoch` or an `anyhow` `downcast_mut` unsoundness that appear nowhere in your diff.
Cause: RUSTSEC advisories publish continuously and `cargo deny check advisories` resolves them against the *current* graph, not your change; the esp-idf chain (`esp-idf-hal → embuild → globwalk → ignore → crossbeam-deque → crossbeam-epoch`, plus `anyhow`) periodically acquires fresh ones.
Fix: confirm the crate is in the esp-idf chain and not your new deps, then treat it as separate dependency-hygiene work — `cargo update -p <crate>` to the advisory's patched version, or a dated, commented `deny.toml` ignore if the pinned esp stack blocks it. Never silently ignore just to green a feature branch.
Confirmed 2026-07 on the `tamer::mpu6050` PR: RUSTSEC-2026-0204 and -0190 surfaced, while that PR's only new dep (`micromath`, zero transitive) was advisory-clean.

**`just check-idf` (`cargo check`) does not link, so it cannot catch a missing `critical-section` implementation — only a real device build/flash does.**
Symptom: `just check-idf` passes clean, but `just run`/`just flash` fails at link with `undefined reference to _critical_section_1_0_acquire` / `_critical_section_1_0_release` for the encoder ISR's `critical_section::with`.
Cause: two blind spots. (1) `cargo check` skips linking, so missing-symbol errors are invisible to the fast gate — `critical-section` provides only the *API* (`Mutex`, `with`); some crate must supply the *implementation* via a feature. (2) promoting the dep to `esp-idf-svc = { workspace = true }` silently dropped the donor's `features = ["critical-section"]` (a `workspace = true` dep inherits the version, not the consumer's old features).
Fix: re-declare the feature on the consuming crate — `esp-idf-svc = { workspace = true, features = ["critical-section"] }` — so esp-idf-svc supplies the FreeRTOS-backed impl (task-context only — it is *not* ISR-safe; see "ESP-IDF — critical-section backend ISR safety"). Never report "device-verified" from `check-idf` alone; run `just flash` to confirm the link.
Confirmed 2026-07 on ESP32-S3 (`idf_s3_rotary`): `check-idf` green, `just flash` failed to link, and adding the feature made the release build link and flash cleanly. See [[cargo-check-doesnt-link-critical-section]].

**`just verify` can fail a single `tamer` doctest with `no X in module` for a type that plainly exists and is re-exported — the doctest linked a stale `libtamer-*.rlib`, not your source.**
Symptom: `cargo test -p tamer --doc` reports `unresolved imports tamer::presence::PresenceSession, …` on a clean tree at a commit where the type is defined; every unit test and the other doctests pass.
Cause: `.cargo/config.toml` sends builds to `target/ide`, where several `libtamer-<hash>.rlib` accumulate across branches. Cargo treated an older one as fresh, so rustdoc's `--extern tamer=…` (visible with `cargo test -v`) pointed at an rlib built before the type existed — `strings <rlib> | grep -c PresenceSession` returned 0 for it and 63 for its newer sibling.
Fix: `touch` the source file (or `cargo clean -p tamer`) and re-run; the doctest passes once the rlib is rebuilt. Check the `--extern` path before suspecting the code. Hit 2026-09-25 on `presence-session` at `8659420`.

**A floating `nightly` satisfies `rust-version = "1.95"` yet can fail to compile the pinned esp-hal stack — `1.95.0-nightly` from January lacks features that 1.95.0 stabilised in March.**
Symptom: `just check-hal` (then `cargo +nightly … -Zbuild-std=core,alloc`) failed on `esp-sync 0.3.0` with `error[E0658]: use of unstable library feature cfg_select … this compiler was built on 2026-01-26`, while `just check-idf` on the `esp` toolchain and the stable toolchain both compiled the same crate.
Cause: Cargo compares only the numeric version, so a nightly older than a stabilisation date passes the MSRV gate and then rejects the code (the sibling `rustyfarian-ws2812` hit the same class on its AVR nightly). The RISC-V bare-metal targets have prebuilt `core` on stable, so `-Zbuild-std` — the only reason for nightly — was never needed there.
Fix: RISC-V bare-metal recipes (`check-hal`, `scripts/build-example.sh`) build on stable with the rustup target (as the sibling does); only Xtensa keeps `+esp -Zbuild-std=core,alloc`. Applied 2026-09-25 during the esp-hal 1.2.2 bump.

## Hardware — inputs

**KY-003 and "Hall sensor" marketplace labels are ambiguous — verify the actual chip before modeling behavior.**
KY-003 modules ship two different sensors in the same form: the A3144 (`3144xUA`) is a unipolar open-collector Schmitt *digital* switch (idles HIGH via pull-up, snaps LOW on one magnetic pole only), while SS49E (`49E`) / AH477 are linear *analog* sensors (idle ~VCC/2, proportional across both poles, need ADC + threshold).
Reading an A3144 through ADC + `tamer::hall::HallSensor` (which assumes a linear bipolar response) is the wrong abstraction: it idles near 4095, detects one polarity cleanly, and clips on the other because it never drives below ~VCC/2.
Fix: check the TO-92 marking (`3144EUA`, `SS49E`, `AH477`, …). For A3144 KY-003 modules read via `tamer::presence::DigitalPresence` (ActiveLow, 20 ms debounce), not the `tamer::hall` + ADC linear model.
Confirmed on ESP32-C3: the KY-003 shipped an A3144; switching from ADC + linear Hall to GPIO + `DigitalPresence` made its single-pole and no-magnet states cleanly distinguishable.

## ESP-IDF — console output

**ESP-IDF (std) code that prints with bare `println!` produces no serial output from inside an infinite loop.**
`println!` writes to a buffered newlib `stdout` that is never flushed in a non-terminating loop, so the lines sit in the buffer forever; the ESP-IDF boot log and `esp_log` output bypass that buffer (direct UART), which is why only the `I (…)` boot lines reach the monitor and the program's own first line never does (the log stops dead at `Calling app_main()`).
Fix: call `esp_idf_svc::log::EspLogger::initialize_default()` once at startup and use `log::info!` instead of `println!` — `esp_log` is unbuffered, the same path the boot lines use. This matches the convention every idf example in the sibling `rustyfarian-network` repo already follows.
Confirmed on ESP32-C3 with `idf_c3_b3f`: after the switch, the `B3F button ready` line and the press/release events appear immediately.

## esp-hal vs esp-idf — ADC raw floor

**A fixed-range ADC→output map (`RangeMap::new(0, 4095, …)`) reaches the rails on the ESP-IDF tier but not on esp-hal, because the two stacks floor the raw reading differently.**
Symptom: the same `poti_led` dimmer (pot → `RangeMap` → LEDC PWM) went fully dark and bright on esp-idf, but stayed in a compressed mid-range on esp-hal.
Cause: `esp-idf-hal`'s ADC oneshot applies the chip's efuse calibration (raw ~0 low, near full-scale high); `esp-hal`'s `read_oneshot` returns the *uncalibrated* SAR value with no `AdcCal*` scheme, which on the C3 floors ~100–200 and ceilings below 4095 — so a `0..=4095` map never hits its endpoints.
Fix: don't hard-code the ADC span for rail-reaching controls. Run an `AnalogCalibration` startup sweep and build the map from the observed range (`RangeMap::new(range.min(), range.max(), 0, 255)`); it self-adjusts per stack. (Or attach esp-hal's `AdcCalCurve`/`AdcCalLine`.)
Confirmed on ESP32-C3-DevKitM-1: after the calibration sweep, both tiers sweep fully dark → full bright.

## esp-hal — console output

**`esp-println` configured for `jtag-serial` is invisible on a board that only exposes its UART bridge.**
The `jtag-serial` transport routes output to the chip's internal USB Serial/JTAG controller (the native USB port), not UART0; a dev board wired through a UART-bridge chip (`/dev/cu.usbserial-*`, e.g. CP210x) with no native-USB connection shows the boot log (ROM/bootloader use UART0) but never the app's `esp-println` output. The symptom is identical to a dead program: boot lines appear, the app's first `println!` does not.
Fix: select `esp-println/uart` for the chip so output goes to UART0 and lands on the same `usbserial` bridge used to flash — or connect the board's native USB port and monitor the `/dev/cu.usbmodem*` device it enumerates. The C3/C6 hal examples here use `uart`; `rustyfarian-network`'s hal tier uses `jtag-serial` (it assumes the native-USB port).
Confirmed on ESP32-C3 with `hal_c3_b3f`: after switching to `uart`, output appears on `/dev/cu.usbserial-210`.

**`espflash monitor --non-interactive` without a TTY captures nothing from a native USB Serial/JTAG board — it stops after `Using flash stub` and the app never prints.**
Run from a script or agent, the monitor connects through the bootloader and leaves the chip in the flash stub instead of resetting into the app.
Fix: use `just capture <seconds> <logfile>`, which hard-resets with `espflash reset` and copies the raw port with `cat`.
The port re-enumerates on reset, so the capture waits briefly for it to return, and the boot banner is still lost; print measurements periodically, not only at boot.
Confirmed on the CrowPanel 1.28" ESP32-S3 (`/dev/cu.usbmodem2101`), espflash 4.6.0.

## ESP-IDF — interrupt subscriptions vs. persistent interrupts

**`PinDriver::subscribe` + `enable_interrupt` (HAL pattern) is one-shot and unsuitable for edge-dense inputs like rotary encoders.**
Symptom: a quadrature encoder silently drops edges under main-loop latency (display DMA, I2C reads) — the handler fires once, auto-disables, and must be re-armed, so bursts of detent edges arriving between polls are lost even though the CPU and ISR are idle.
Cause: `esp-idf-hal` 0.46's `PinDriver` interrupt subscriptions are a convenience abstraction for low-frequency, one-edge signals (e.g. waking on a button press) and do not expose the persistent registration the underlying ESP-IDF C API provides.
Fix: for edge-dense inputs, call `gpio_isr_handler_add` directly (via `esp_idf_svc::sys`) with persistent `AnyEdge` interrupts; see `rustyfarian_esp_idf_peripherals::rotary::Encoder` and ADR-005. Confirmed 2026-07 on the interrupt-encoder donation (raw FFI captured every edge; the HAL path lost half the detents/sec).

## ESP-IDF — critical-section backend ISR safety

**On ESP-IDF, `critical_section::with` is a FreeRTOS recursive mutex and must never be taken from an ISR; use `esp_idf_hal::interrupt::IsrCriticalSection` there.**
Symptom: `LoadProhibited` (EXCVADDR `0x4`) on the GPIO ISR core in `xTaskRemoveFromEventList` ← `xQueueGiveMutexRecursive` ← `EspCriticalSection::release` ← `encoder_isr`, after a few hundred cross-core `Encoder` construct/drop cycles with the knob turning (ESP32-S3, 2026-10-28).
Cause: `esp-idf-svc`'s `critical-section` feature forwards to `esp-idf-hal`'s `EspCriticalSection` (`src/task.rs:470-500`, `:780-800` in 0.47.0): `xQueueTakeMutexRecursive(.., portMAX_DELAY)` / `xQueueGiveMutexRecursive`. In an ISR the "current task" is whichever task was interrupted, so the ISR can block, wake a waiter from ISR context (the crash, when task code on the other core waits on the section), or recursively re-enter a section the interrupted task holds (a `RefCell` double borrow, even on single-core chips). The 2026-07 entry here that called this backend "ISR-safe, disables interrupts" confused it with `IsrCriticalSection` in `src/interrupt.rs`.
Fix: guard ISR-shared state with `IsrCriticalSection` (`xPortEnterCriticalTimeout` on a `portMUX_TYPE` on ESP32/S3, `vPortEnterCritical` on C3/C6 — interrupt mask plus spinlock, valid in ISR and task context), as `rotary::IsrLocked` does. The esp-hal tier's provider (`esp_sync::RawMutex`, interrupt mask + core-owned spinlock) *is* ISR-safe. Repeat this source check on every `esp-idf-hal` bump (see `maintenance-plan.md`).
