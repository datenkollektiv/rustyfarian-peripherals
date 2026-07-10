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

*(none yet)*

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
