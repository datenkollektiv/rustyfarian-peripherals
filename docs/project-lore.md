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

*(none yet)*

## ESP-IDF — console output

**ESP-IDF (std) code that prints with bare `println!` produces no serial output from inside an infinite loop.**
`println!` writes to a buffered newlib `stdout` that is never flushed in a non-terminating loop, so the lines sit in the buffer forever; the ESP-IDF boot log and `esp_log` output bypass that buffer (direct UART), which is why only the `I (…)` boot lines reach the monitor and the program's own first line never does (the log stops dead at `Calling app_main()`).
Fix: call `esp_idf_svc::log::EspLogger::initialize_default()` once at startup and use `log::info!` instead of `println!` — `esp_log` is unbuffered, the same path the boot lines use. This matches the convention every idf example in the sibling `rustyfarian-network` repo already follows.
Confirmed on ESP32-C3 with `idf_c3_b3f`: after the switch, the `B3F button ready` line and the press/release events appear immediately.

## esp-hal — console output

**`esp-println` configured for `jtag-serial` is invisible on a board that only exposes its UART bridge.**
The `jtag-serial` transport routes output to the chip's internal USB Serial/JTAG controller (the native USB port), not UART0; a dev board wired through a UART-bridge chip (`/dev/cu.usbserial-*`, e.g. CP210x) with no native-USB connection shows the boot log (ROM/bootloader use UART0) but never the app's `esp-println` output. The symptom is identical to a dead program: boot lines appear, the app's first `println!` does not.
Fix: select `esp-println/uart` for the chip so output goes to UART0 and lands on the same `usbserial` bridge used to flash — or connect the board's native USB port and monitor the `/dev/cu.usbmodem*` device it enumerates. The C3/C6 hal examples here use `uart`; `rustyfarian-network`'s hal tier uses `jtag-serial` (it assumes the native-USB port).
Confirmed on ESP32-C3 with `hal_c3_b3f`: after switching to `uart`, output appears on `/dev/cu.usbserial-210`.
