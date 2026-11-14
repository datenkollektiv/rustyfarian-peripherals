# ADR-010: Shared tier logic follows a three-rung ladder

## Status
Accepted (2026-11-04).
Adopts the ladder of `rustyfarian-power` ADR-004 (shared-logic ladder); both adapt `rustyfarian-network` ADR-022 decision 10.
Resolves the VISION open question "Interrupt vs. polling boundary".
Amended (2026-11-07): the sibling ladders are `rustyfarian-network` ADR-023 and `rustyfarian-ws2812` ADR-012.
They adopt the same rungs and review rule.
The instances, the retrofit and a correction to the Consequences below are dated notes in this ADR.
The hardware pass for this ladder runs through the bench of ADR-011.

## Context
- Where both tiers sequence the same flow, each re-implements the order of the steps, and the two copies drift.
- `rustyfarian-network` ADR-022 decision 10 answers that drift with sans-IO state machines.
  - Inputs go in and actions come out.
  - A thin loop per tier performs the actions.
- `rustyfarian-power` ADR-004 turns this into a three-rung ladder, so a flow gets a machine only when a generic driver or a pure function cannot cover it.
- This repo adopts the same ladder, so the rungs, rules and vocabulary match across the family.
- Here rung 1 carries most of the weight: `embedded-hal` 1.0 covers the I/O, and both pinned stacks implement it.
- ADR-009 therefore puts whole drivers such as `Bme680<I2C>` in `tamer`, generic over `I2c` and `DelayNs`.
- Rung 2 already exists here, unlike in `rustyfarian-power`.
- `QuadratureDecoder::update(a, b) -> Option<EncoderDirection>` is fed by the esp-hal GPIO hook, the ESP-IDF ISR, and the polled `QuadratureInput`.
  - The interrupt and polling paths therefore share one machine.
- `ButtonDecoder`, `Debouncer`, `TouchTracker` and `PresenceSession` have the same shape, with time passed in as `now`.
- The open gap is a flow that blocks.
  - `Bme680::measure` waits over 150 ms in `DelayNs` during the gas heater phase.
  - That wait stalls an esp-hal async executor.

## Decision
1. Shared logic takes the lowest rung that fits:
   - **Rung 1 — generic driver.** The flow lives in `tamer`, generic over `embedded-hal` traits (ADR-009).
   - Rung 1 is the default.
   - **Rung 2 — event machine.** The core decodes samples: `update(sample, now) -> Option<Event>`.
   - A tier, an ISR, or an `embedded-hal` adapter feeds the rung-2 machine.
   - **Rung 3 — action machine.** The core emits I/O requests: `poll(&mut self, input) -> Action`.
   - A thin loop per tier performs the rung-3 actions.
2. Rung 3 is used only when a flow meets one of these triggers:
   - It must run under more than one execution model, such as blocking, async, or a cooperative tick loop.
   - The tiers perform I/O outside `embedded-hal`, such as interrupt registers, but must share the order of the steps.
3. Flat logic stays as pure functions or stateless types (`compensate`, `res_heat`, `RangeMap`), not machines.
4. Review rule, as in power ADR-004 point 7 (from network ADR-022 decision 10): a branch in a tier adapter that is not an error or event mapping is logic and moves into `tamer`.
5. Time is an input.
   - A machine never reads a clock.
   - The caller passes `now` or reports elapsed waits.
6. A rung-3 machine answers out-of-phase input with an explicit no-op action, so a host test of a tier loop can detect it.
7. Every rung-3 flow reaches a terminal action, and the tier loop treats the no-op action as an error exit, so the loop is bounded.
8. Per-chip behaviour is a runtime value passed in by the tier, not a `cfg` gate inside the machine.
9. Tier-specific facts stay in the adapters: ISR safety, interrupt arming and teardown order, lock order, and register access.
   - The rotary interrupt plumbing in both tiers is adapter I/O under this rule.

## Consequences
Positive:
- Rungs 1 and 2 already cover every shipped driver, so no existing code changes.
  - *Correction (2026-11-07):* this did not hold for the tier adapters and the examples, which the retrofit below changed.
  - The drivers' signatures, errors and bus traffic stayed the same.
  - `Bme680::measure` and `Mpu6050::calibrate` now run over rung-3 machines, so their internals did change.
- The review rule gives `code-reviewer` a concrete check for the sans-IO boundary.
- A blocking flow has a defined path to non-blocking use without a second copy of the sequence.

Negative:
- Rung 3 adds one indirection: a reader follows an action from `tamer` into the tier loop.
- Host tests prove the order of the steps, not the hardware.
- Rig runs remain the arbiter.
- Constraints on the tier loop, such as running inside a critical section or reading a pin late, are not checked by any type.

Constraints:
- Machines are `no_std`, allocation-free, and free of async.
- Inputs, actions and the machine are small `Copy` values where practical.
- Rung 2 and 3 machines need tick-boundary and state-transition tests in `tamer`.
- Moving a flow up a rung is demand-driven: the first consumer that needs it pays for it.

The first rung-3 implementations are `ForcedConversion` in `tamer::bme680` (behind `Bme680::measure`) and `OffsetSampler` in `tamer::mpu6050` (behind `Mpu6050::calibrate`).
Both blocking methods are thin runners over the machine.
The async runners now exist: `Bme680::measure_async` and `Mpu6050::calibrate_async` (feature `hal-async`) drive the same machines.

## Instances

Dated 2026-11-07.
Paths are module paths in `tamer`; the feature in brackets gates the type.

- **Rung 1 — generic driver.**
  - `bme680::Bme680<I2C>` [`bme680`, `hal`] and `mpu6050::Mpu6050<I2C>` [`hal`], generic over `embedded_hal::i2c::I2c` and `DelayNs`.
  - The `InputPin` adapters `debounce::DebouncedInput`, `rotary::QuadratureInput`, `button::ButtonInput` and `presence::DigitalPresenceInput` [`hal`] each wrap a rung-2 decoder.
  - Rung 1 allows this: the rung-1 driver may contain a rung-2 decoder, as `rustyfarian-ws2812` ADR-012 decision 1 notes.
  - The tier adapter `analog::AdcReader` in `rustyfarian-esp-hal-peripherals` and `rustyfarian-esp-idf-peripherals` (added 2026-11-08) is a rung-1 adapter over `tamer::analog::AnalogRead`.
  - It holds only ADC1 bus setup (init code and attenuation fixed, raw counts, errors mapped to `AdcError`).
  - The range, deadband and smoothing logic stays in `tamer`.
- **Rung 2 — event machine** (`update(sample, now) -> Option<Event>` or a close variant):
  - `rotary::QuadratureDecoder::update(a, b)`, fed by the esp-hal GPIO hook, the ESP-IDF ISR and `QuadratureInput`.
  - `button::ButtonDecoder::update(pressed, now)`.
  - `debounce::Debouncer::update(raw, now)` and `debounce::EdgeDetector::update(raw, now)`.
  - `touch::TouchTracker::update(touch, now)`.
  - `presence::PresenceSession::update(level, now)`, with `snapshot` and `restore` to resume across a deep-sleep reset.
  - `burst::BurstDetector::update(transition, now_ms)`.
  - `hotplug::Hotplug::observe(ok, now)`.
  - `tone::ToneSequencer::update(now)` and `morse::MorseKeyer::update(now)` take time as their only input.
- **Flat logic, no rung.**
  - These are pure functions or stateful helpers with no sample input, so a machine would be ceremony.
  - `latch::OnceLatch` (`trip`, `rearm`), `hall::HallSensor::pole` with `Pole`, `tone::ToneOutput::duty`, `analog::AnalogCalibration::range_or`.
  - `bme680::compensate` and `res_heat`, `mpu6050::parse_raw`, and `range_map::RangeMap`.
- **Rung 3 — action machine.**
  - `bme680::ForcedConversion` (`poll(ConversionInput) -> ConversionAction`), run by `Bme680::measure` and `Bme680::measure_async` [`hal-async`].
  - `mpu6050::OffsetSampler` (`poll(SamplerInput) -> SamplerAction`), run by `Mpu6050::calibrate` and `Mpu6050::calibrate_async` [`hal-async`].
  - Both machines answer out-of-phase input with an explicit `Error::Sequence`.
  - Both action enums are exhaustive, so a new variant is a breaking change.

## Retrofit

Dated 2026-11-07.
The code extraction ran in the 2026-11-07 retrofit.
Its hardware evidence is the final hardware pass in `release-plan.md`, which blocks the next release.
The user-visible side of each change is in the CHANGELOG `[Unreleased]` section.

### Moved into `tamer`

Each item was a branch or a constant that an example or a tier adapter held, and now lives in `tamer`.

- `tone::ToneOutput::duty`: the square-wave duty rule moved out of the passive-buzzer arpeggio examples, and both tiers now use the same 10-bit timer.
- `analog::AnalogCalibration::range_or`: the observed-range fallback moved out of the poti examples.
- `latch::OnceLatch`: the report-once logic moved out of the poti, poti-LED and linear Hall examples.
- `hall::HallSensor::pole` and `Pole`: the pole classification moved out of the linear Hall examples, which now only print it.
- `burst::BurstDetector`: replaces the hand-written burst detector in the tilt-motion examples.
- `rotary::QuadratureDecoder::try_new` and `DetentStepsError`: the zero-detent check moved out of both encoder tiers, and each tier only maps the error to `EncoderError::InvalidConfig`.
- `rotary::QuadratureDecoder::edge_count`: the esp-hal `Encoder` and `InterruptEncoder` no longer keep their own edge counter, and the ESP-IDF encoder passes the count through.

### Restructured inside `tamer`

- `Bme680::measure` and `Mpu6050::calibrate` are thin blocking runners over `ForcedConversion` and `OffsetSampler`.
  - Their signatures, I2C transactions and delay calls are unchanged.
  - The only visible change is the `Error::Sequence` variant for an out-of-phase machine answer.

### Deliberately left in the adapters

Each item is tier I/O or a shape decision, and the reason is given.

- **Rotary ISR arming and teardown** (esp-hal `on_gpio_interrupt` and the `InterruptEncoder` drop, ESP-IDF `gpio_isr_handler_add` and `gpio_intr_enable`).
  - Interrupt registers, ISR safety and teardown order are tier facts (decision 9).
  - The decoder is shared, the wiring is not.
- **ISR critical section.**
  - The ESP-IDF tier uses esp-idf-hal's `IsrCriticalSection`, and the esp-hal tier uses its own primitive.
  - Which primitive is ISR-safe is a platform fact (decision 9).
- **IRAM placement** of the ESP-IDF encoder ISR trampoline, and the choice of `ESP_INTR_FLAG_IRAM`.
  - Section placement and interrupt-service flags are linker and runtime facts, so `tamer` cannot own them.
- **LEDC timer and channel setup, and the duty writes.**
  - These are register I/O in each example (decision 9).
  - The buzzer's resolution is no longer a tier difference, because `ToneOutput::duty` takes `max_duty` and both tiers run 10-bit.
  - The poti-LED keeps 8-bit resolution to match `RangeMap`'s `u8` output.
  - That is a per-example constant and not a tier difference.
- **`EncoderConfig`** (both tiers, `Default` for EC11) stays in the tier crates.
  - Constructors in `tamer` stay positional, as ADR-007 decided for the family, so `tamer` gains no config struct.
  - The tier type is public API of the tier crate, not a branch to move.

### Hardware validation

The hardware pass for this retrofit is the final hardware pass in `release-plan.md`, and the bench of ADR-011 runs the checks that have a scenario.
Host tests prove the order of the steps and the state transitions, not the waveforms or the interrupt timing.

## Alternatives Considered
| Alternative                   | Pros                              | Cons                                            | Why Rejected                                  |
|:------------------------------|:----------------------------------|:------------------------------------------------|:----------------------------------------------|
| Repo-specific pattern         | Tuned to `embedded-hal` drivers   | Rules and vocabulary drift from the siblings    | Power ADR-004 rung 1 already fits ADR-009     |
| Action machines everywhere    | One uniform shape                 | Ceremony for drivers that `embedded-hal` covers | No drift to fix; adds indirection for nothing |
| Generic drivers only          | Simplest; already in place        | A blocking flow has no non-blocking path        | Leaves the BME680 async gap open              |
| Tier-owned sequencing         | Each tier tunes its own flow      | Two copies of the order drift apart             | The failure mode `try_sleep` hit in power     |
