# ADR-016: Clocked-GPIO sensor drivers live in `tamer`; the interrupt mask is a tier impl

## Status
Accepted (2026-11-19).
It answers the open point of [ADR-009](009-i2c-sensor-drivers-in-tamer.md): a device that is not I2C needs its own placement decision.
The first instance is the HX711 load-cell ADC (`tamer::hx711`, [feature doc](../features/hx711-v1.md)).

## Context
The beekeeper weighs a hive with an HX711 and four load cells and asked for the driver here (`review-queue/feature-request-peripherals-hx711.md`).
The HX711 speaks a two-wire clocked protocol, DOUT in and PD_SCK out, not I2C, so ADR-009 excluded it.

- A conversion is read as 24 PD_SCK pulses, MSB first, plus one to three pulses that select the input and gain of the next conversion.
- PD_SCK held high for more than 60 µs powers the chip down, and the sample in flight is garbage.
- Under ESP-IDF a FreeRTOS preemption inside the pulse train is realistic once Wi-Fi or LoRa tasks run.
- `critical_section::with` does not prevent that on ESP-IDF: the provider there is a recursive FreeRTOS mutex, task-context only (lore "ESP-IDF — critical-section backend ISR safety").
- The esp-hal provider masks interrupts and takes a core spinlock, so there it is enough.
- ADR-010 decision 9 keeps ISR-safety primitives in the tier adapters, and rung 1 makes a generic `embedded-hal` driver the default.

Both pinned stacks implement `embedded_hal::digital::{InputPin, OutputPin}` and `DelayNs`, esp-hal 1.2.2 with `Input` / `Output` / `Delay` and esp-idf-hal 0.47.0 with `PinDriver` / `Ets`.
The calibration math that turns counts into grams is pure, and the consumer wants to persist it across reboots and ship it in a downlink.

## Decision
1. A clocked-GPIO device joins the ADR-009 placement.
   - A device-named `tamer` module behind a feature of the same name holds the pure part: decode, pulse counts, constants, calibration math.
   - The driver is compiled with the device feature and `hal`, generic over `InputPin`, `OutputPin` and `DelayNs`.
   - The driver owns its pins and hands them back through `release()`.
2. Timing-critical pin sequences run inside a caller-provided interrupt mask.
   - `tamer::hx711::InterruptMask` is a trait with one method, `masked(&self, f: impl FnOnce() -> R) -> R`, that runs `f` with interrupts masked on the calling core.
   - The contract is in the trait docs: a mutex is not enough, `f` runs exactly once, and `f` must not log, allocate or block.
   - The driver takes the mask per call, as it takes the delay, and keeps the masked region to the pulse train.
   - `tamer::hx711::NoopInterruptMask` runs `f` directly, for host tests and for the mocks.
3. Each tier ships the real mask as `hx711::MaskedSection`, a tier fact under ADR-010 decision 9.
   - `rustyfarian-esp-hal-peripherals` wraps `critical_section::with`, whose esp-hal provider masks interrupts.
   - `rustyfarian-esp-idf-peripherals` wraps `esp_idf_hal::interrupt::IsrCriticalSection`, as the rotary ISR already does, and never `critical_section::with`.
   - A tier `hx711` feature enables `tamer/hx711`; the tier module re-exports the `tamer` items and a conformance guard pins the impl to the trait.
4. The pulse delays come from a busy-wait `DelayNs`: esp-hal `Delay`, esp-idf-hal `Ets`, never `FreeRtos`.
   - A yielding delay inside the mask would block with interrupts off or hand the core to another task.
   - *Note (2026-11-19):* esp-hal's `Delay` quantises to whole microseconds, so a 1 µs request can last under 1 µs.
     The driver therefore clocks with `PULSE_US = 2`, so each PD_SCK phase lasts more than 1 µs on both tiers.
     The masked window is about 100 to 135 µs for 25 to 27 pulses, and the decision above is unchanged.
5. The driver offers blocking reads only; there is no `hal-async` runner.
   - The whole pulse train must sit inside one mask, so there is no await point to expose; waiting for DOUT to fall is the caller's business.
   - Under ADR-010 this is rung 1: one execution model, and the I/O goes through `embedded-hal`.
6. Calibration is one `Copy` value type with a byte codec.
   - `to_bytes()` yields 16 bytes, little-endian: magic, version, tare offset `i32`, counts per gram `f32`, reference temperature `i16` in tenths of a degree, drift `f32` in grams per degree.
   - `from_bytes()` returns `None` on a wrong magic or version and on a non-finite or zero scale or non-finite drift, so a restored value is always usable.
   - Where the bytes live (NVS, a downlink) stays with the consumer.
7. The signed window average the pipeline needs is `smoothing::SignedSlidingAverage<N>`, a public sibling of `SlidingAverage`, not a private ring and not a generic rewrite of the existing type.

## Consequences
- One host-tested HX711 driver serves both tiers, and the preemption hazard is visible in the signature: a read cannot be written without naming a mask.
- `tamer` gains a second kind of hardware trait beside `embedded-hal`, with its mock in the same module as `ir` does it.
- The tiers gain a module each with one type; the ESP-IDF one documents why it is not `critical_section`.
- Host tests prove the pulse order, the gain pulses and that every PD_SCK edge happened inside the mask; they cannot prove the 60 µs budget, which is a rig run.
- A consumer that polls DOUT from an ISR or an async executor still calls the blocking `read` once the line is low.
- The 16-byte frame quantises the reference temperature to 0.1 °C, which is below the drift fit's own error.

## Alternatives Considered
|                              Alternative | Pros                                 | Cons                                                             | Why Rejected                                                 |
|-----------------------------------------:|:-------------------------------------|:-----------------------------------------------------------------|:-------------------------------------------------------------|
|          Driver in each tier, pure math only in `tamer` | Tier owns its own critical section | Two copies of the pulse train drift; the decode is tested twice  | ADR-009 reasoning holds for a clocked protocol too           |
|               Driver documents the mask, does not take one | Smallest signature             | The 60 µs hazard is silent; the next consumer forgets it         | The request names a field failure mode, not a style point    |
|                  `read_with(\|run\| ...)` closure hook | No trait                          | Nothing enforces that the hook masks; mocks cannot assert it     | A trait gives the compiler and the test rig something to hold |
|      `critical_section::with` inside `tamer` | One impl, no tier code            | Wrong on ESP-IDF; adds a `tamer` dependency                      | The ESP-IDF provider is a mutex (lore entry)                 |
|                 Driver owns the mask as a third generic | Nothing to pass per call        | Constructor grows; a shared tier mask cannot guard two drivers   | Stateless masks need no owner                                |
|                           `hal-async` runner as for SHT3x | Tier parity with the I2C drivers | No await point inside a masked pulse train                      | Nothing to make asynchronous                                 |
|                       20-byte frame with an `f32` reference | No quantisation                 | Breaks the requested `[u8; 16]`                                  | 0.1 °C is below the fit's error                              |
|   Make `SlidingAverage` generic over the sample type | One average type                | Breaks every `SlidingAverage::<N>` caller                        | Additive sibling costs nothing                               |
