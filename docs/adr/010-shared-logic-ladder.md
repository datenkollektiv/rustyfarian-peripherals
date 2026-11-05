# ADR-010: Shared tier logic follows a three-rung ladder

## Status
Accepted (2026-11-04).
Adopts the ladder of `rustyfarian-power` ADR-004 (shared-logic ladder); both adapt `rustyfarian-network` ADR-022 decision 10.
Resolves the VISION open question "Interrupt vs. polling boundary".

## Context
Where both tiers sequence the same flow, each re-implements the order of the steps, and the two copies drift.
`rustyfarian-network` ADR-022 decision 10 answers that drift with sans-IO state machines: inputs go in, actions come out, and a thin loop per tier performs them.
`rustyfarian-power` ADR-004 turns this into a three-rung ladder, so a flow gets a machine only when a generic driver or a pure function cannot cover it.
This repo adopts the same ladder, so the rungs, rules and vocabulary match across the family.
Here rung 1 carries most of the weight: `embedded-hal` 1.0 covers the I/O, and both pinned stacks implement it.
ADR-009 therefore puts whole drivers such as `Bme680<I2C>` in `tamer`, generic over `I2c` and `DelayNs`.
Rung 2 already exists here, unlike in `rustyfarian-power`.
`QuadratureDecoder::update(a, b) -> Option<EncoderDirection>` is fed by the esp-hal GPIO hook, the ESP-IDF ISR, and the polled `QuadratureInput`, so the interrupt and polling paths share one machine.
`ButtonDecoder`, `Debouncer`, `TouchTracker` and `PresenceSession` have the same shape, with time passed in as `now`.
The open gap is a flow that blocks: `Bme680::measure` waits over 150 ms in `DelayNs` during the gas heater phase, which stalls an esp-hal async executor.

## Decision
1. Shared logic takes the lowest rung that fits:
   - **Rung 1 — generic driver.** The flow lives in `tamer`, generic over `embedded-hal` traits (ADR-009). Use this by default.
   - **Rung 2 — event machine.** The core decodes samples: `update(sample, now) -> Option<Event>`. A tier, an ISR, or an `embedded-hal` adapter feeds it.
   - **Rung 3 — action machine.** The core emits I/O requests: `poll(&mut self, input) -> Action`, and a thin loop per tier performs them.
2. Rung 3 is used only when a flow meets one of these triggers:
   - It must run under more than one execution model, such as blocking, async, or a cooperative tick loop.
   - The tiers perform I/O outside `embedded-hal`, such as interrupt registers, but must share the order of the steps.
3. Flat logic stays as pure functions or stateless types (`compensate`, `res_heat`, `RangeMap`), not machines.
4. Review rule, as in power ADR-004 point 7 (from network ADR-022 decision 10): a branch in a tier adapter that is not an error or event mapping is logic and moves into `tamer`.
5. Time is an input. A machine never reads a clock; the caller passes `now` or reports elapsed waits.
6. A rung-3 machine answers out-of-phase input with an explicit no-op action, so a host test of a tier loop can detect it.
7. Every rung-3 flow reaches a terminal action, and the tier loop treats the no-op action as an error exit, so the loop is bounded.
8. Per-chip behaviour is a runtime value passed in by the tier, not a `cfg` gate inside the machine.
9. Tier-specific facts stay in the adapters: ISR safety, interrupt arming and teardown order, lock order, and register access.
   The rotary interrupt plumbing in both tiers is adapter I/O under this rule.

## Consequences
Positive:
- Rungs 1 and 2 already cover every shipped driver, so no existing code changes.
- The review rule gives `code-reviewer` a concrete check for the sans-IO boundary.
- A blocking flow has a defined path to non-blocking use without a second copy of the sequence.

Negative:
- Rung 3 adds one indirection: a reader follows an action from `tamer` into the tier loop.
- Host tests prove the order of the steps, not the hardware; rig runs remain the arbiter.
- Constraints on the tier loop, such as running inside a critical section or reading a pin late, are not checked by any type.

Constraints:
- Machines are `no_std`, allocation-free, and free of async; inputs, actions and the machine are small `Copy` values where practical.
- Rung 2 and 3 machines need tick-boundary and state-transition tests in `tamer`.
- Moving a flow up a rung is demand-driven: the first consumer that needs it pays for it.

The first rung-3 implementations are `ForcedConversion` in `tamer::bme680` (behind `Bme680::measure`) and `OffsetSampler` in `tamer::mpu6050` (behind `Mpu6050::calibrate`).
Both blocking methods are thin runners over the machine.
The async runners now exist: `Bme680::measure_async` and `Mpu6050::calibrate_async` (feature `hal-async`) drive the same machines.

## Alternatives Considered
| Alternative                   | Pros                              | Cons                                            | Why Rejected                                  |
|:------------------------------|:----------------------------------|:------------------------------------------------|:----------------------------------------------|
| Repo-specific pattern         | Tuned to `embedded-hal` drivers   | Rules and vocabulary drift from the siblings    | Power ADR-004 rung 1 already fits ADR-009     |
| Action machines everywhere    | One uniform shape                 | Ceremony for drivers that `embedded-hal` covers | No drift to fix; adds indirection for nothing |
| Generic drivers only          | Simplest; already in place        | A blocking flow has no non-blocking path        | Leaves the BME680 async gap open              |
| Tier-owned sequencing         | Each tier tunes its own flow      | Two copies of the order drift apart             | The failure mode `try_sleep` hit in power     |
