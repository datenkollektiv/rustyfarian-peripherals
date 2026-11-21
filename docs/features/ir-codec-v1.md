# Feature: IR codec v1

A pure infrared pulse codec, `tamer::ir` (feature `ir`), for a downstream project's props that drive and read consumer IR remotes.
The hardware bridge is `rustyfarian-esp-hal-peripherals::ir` (feature `ir`), `IrTransmitter` and `IrReceiver` on the RMT peripheral.

Status: implemented; C6 → C3 NEC end to end passed on hardware (2026-11-21), RMT carrier frequency not yet measured.

## Decisions
| Decision | Reason | Rejected Alternative |
|----------|--------|----------------------|
| One flat module `tamer::ir`, feature `ir = []`, no dependency | Matches `morse`/`tone`; NEC is only a preset, so not named `nec` | A `nec` module |
| `&[u32]` µs pulses, mark first, odd length legal; `PulseBuf<N>` for encode only | Natural RMT/timer width; the idle gap is never a pulse | `u16` pulses |
| `PulseCodec` is data; positional validating `const fn new` returns `Result` | Invariants (1..=32 bits, distinguishable bits) hold; no panics | Public fields |
| Bits are classified by mark and space | A downstream project's 8-bit remote differs from NEC in both | Space only |
| Header mark is checked as an upper bound only; the header space is the anchor | Captures start with marks of 61..8721 us | Two-sided header mark |
| Pulse count is checked before bit classification | A 24-bit frame against NEC reports `WrongBitCount`, not a misleading bit error | Classify first |
| `Tolerance { Percent, Micros }`, inclusive, division-free matching | No divide on RISC-V C3 | Fixed windows as in the Python reference |
| Glitch filter merges: a short pulse is removed and the previous pulse reopened | Keeps alternation and total duration; the reference only dropped it | Drop and keep going |
| `FrameAssembler::edge(now, mark)` / `update(now)`, lazy reset, explicit `arm` | Same idiom as `ToneSequencer`; stray space edges never open a frame | Implicit late-arm |
| NEC repeat: `NEC_REPEAT` and `is_nec_repeat` only (about 15 lines) | Cheap, host-tested; the ~110 ms cadence stays with the caller | A repeat state machine |
| Device timings stay downstream; here only a test fixture | Device tables are not a `tamer` concern | A device preset |
| `carrier_ticks(clock_hz, freq_hz, duty)` lives in `tamer` | The bridge does no arithmetic and the result is host-tested | Computing ticks in the tier |
| Bridge types own the RMT channel creator and the pin, by value | Generic over `Tx` / `RxChannelCreator`: no per-chip cfg; owning prevents sharing | Per-chip channel types |
| RX uses RMT RX, not GPIO edge capture | Hardware 1 us timestamps, no ISR, no static state | GPIO edge plus timer (kept as the fallback for gaps over 32 ms) |
| `IrReceiver::receive(timeout_ms)` blocks, re-arms per call, buffer on the stack | `RxTransaction` borrows its buffer, so it cannot live in the receiver | A stored transaction (self-referential) |
| RMT frames feed `FrameAssembler` as edges, running-sum timestamps, end gap 0 | The RMT already delimited the frame; the 20 us glitch filter stays in `tamer` | Hardware glitch filter (about 3 us at most) |
| Global RMT clock 80 MHz (`IR_RMT_CLOCK`), channel divider 80, so 1 tick is 1 us | The one clock all four chips accept; `tamer` durations map 1:1 | Per-chip clocks |
| TX config: carrier level High, idle output Low, memsize 1 | The esp-hal default carrier level is Low and would carry the space | Defaults |
| A repeat arrives as its own short RMT frame | The 20 ms idle is below the roughly 40 ms repeat gap | A repeat scheduler in the tier |
| `PulseSink` / `PulseSource` traits (+ mocks) on the bridges, as `AnalogRead` | `MockPulseSink` / `MockPulseSource` keep firmware host-testable | Concrete RMT types only |
| `FrameAssembler::feed_segments` (segments to pulses) / `validate_pulses` (list) | Sans-io: the tier only maps `PulseCode` fields; edge cases are host-tested | Logic in the tier |
| Headline types at the `tamer` root; errors and parameters under `tamer::ir::` | Names like `Tolerance` or `DecodeError` would clash as other codecs grow | Flattening everything |
| `is_nec_repeat` fixes the tolerance at 25 % | Cheap helper; a tuned codec does not retune it | A tolerance parameter |
| esp-idf tier: later | Maintainer call; group-clock `rmt_tx_modulate_carrier` suits `carrier_ticks` | Mirroring now |
| TX examples send the NEC preset only | Device-specific codes and timings stay with their downstream profile | A second, device-style frame |
| C6 TX on GPIO 3, the 5V pogo (ADR-015) | The output-capable pogo; J1 15 / 14 / 13 are GND / 5V / GPIO 3 in pogo order | GPIO 2, the 3V3 pogo (an ADC / wake input) |
| TX examples log the clock, the tick pair and both carrier predictions | One boot log tells the group clock (about 38 kHz) from the channel clock (about 475 Hz) | A scope-only check |

- `IrReceiver::receive` keeps the channel across calls via `Channel::reborrow`.

## Constraints
- `no_std`, alloc-free, panic-free; the pulse and edge feed is the seam, and `PulseSink` / `PulseSource` ship with mocks for the bridge.
- `FrameAssembler` has no locking; a tier that calls `edge` from an interrupt must serialise access.
- `update` must run at least once per end-of-frame gap, or two frames merge.
- Headerless codecs cannot tolerate a truncated first mark.

## Carrier-clock evidence
- esp-hal 1.2.2 writes `carrier_high` / `carrier_low` raw to the `CHn_CARRIER_DUTY` register and documents them only as "ticks"; the channel divider is a separate register.
- ESP-IDF v5.3.2 `rmt_tx.c` computes the TX carrier from `group->resolution_hz` ("carrier module works base on group clock").
- The group divider is 1 there, so that is the unscaled source clock, the equivalent of the `Rmt::new` frequency.
- The legacy IDF driver does the same (`rmt_source_clk_hz / carrier_freq_hz`).
- RX demodulation counts in the channel clock instead; the bridge does not use it.
- At 80 MHz and 38 kHz: total 2105 ticks, high 694, low 1411, about 38.00 kHz and 32.97 %.
- esp-hal writes the half-periods raw, with no `- 1`; if the TRM counts `N + 1` ticks the carrier is about 0.1 % off, which the scope check also covers.
- This is source-derived only.
- C6 (esp-hal 1.2.2): the default RMT source is PLL_F80M, selected by PCR `rmt_sclk_sel = 1` with `sclk_div_num = 0`, so `Rmt::new(.., IR_RMT_CLOCK)` holds 80 MHz.
- `rmt.frequency()` returns the stored rate derived from the 80 MHz constant, not a hardware read-back; the C6 ESP-IDF `rmt_ll.h` writes the carrier ticks raw as well.
- On the C3, a downstream receive bench printed `RMT clock 80000000 Hz` (2026-11-21).

## Open Questions
- **Hardware check, not yet done:** scope or logic analyser on GPIO 10 must show a 38 kHz, about 33 % burst per mark, no carrier in spaces, line low at rest.
  A reading near 475 Hz means the channel clock is used and `carrier_ticks` must be fed `clock / divider`.
- **Hardware check, not yet done:** `hal_c3_ir_rx` against a real remote must print the same command as the MicroPython receiver.
  The first captured pulse is a Low mark (a leading High segment is ignored by the assembler either way), and a repeat shows as a separate short frame.
- **Hardware check, not yet done:** `hal_c3_ir_tx` aimed at the downstream project's remote, or at a TSOP feeding `hal_c3_ir_rx` on a second board.
- RMT idle threshold tops out at 32767 us at 1 us ticks, so the reference's 150 ms end gap is not reachable.
  The bridge delimits frames with a 20 ms idle: above the longest internal gap of NEC and the candle remote (the 4.5 ms header space), below the NEC repeat gap.
  Protocols with internal gaps over 20 ms split into several frames; a GPIO-edge receiver feeding `FrameAssembler` would be needed for them (deferred).
  The six real captures have no internal gap above 9 ms.
- `IrReceiver::receive` captures nothing between two calls and drops a frame still in flight at the timeout; a stored non-blocking `poll()` is deferred.
- The hardware glitch filter counts group-clock cycles in an 8-bit register (about 3 us at 80 MHz), so it stays off.
- `IrTransmitter::send` busy-waits for the frame (about 70 ms for NEC).
- C3 pins follow ADR-012: TX on GPIO 10 (NPN output row, shared with the buzzer), RX on GPIO 4 (3-pin pogo row).
- C6 TX on GPIO 3, the 5V pogo (ADR-015); no C6 RX example yet.
- Hardware evidence by path: C6 transmit (`c6/ir-tx`) and C3 receive (the `c6/ir-tx` judge, plus a downstream bench with a real remote).
- Compile-only paths: C3 transmit and C6 receive (bridge and examples link in `ci-riscv`; no C6 RX example), and ESP32 and S3 (library only, `check-matrix-hal`, no example).
- ESP32 has no RX wrap, so an RX `CODES` must fit one RAM block.
- An emitter module may make its own 38 kHz: the OPEN-SMART "Infrared Emitter" carries an 8-pin IC (marking unreadable), two SOT-23 transistors and "38k" on the silkscreen.
  If it does, it wants an unmodulated envelope on SIG; yet it decoded cleanly when driven with the RMT carrier (`c6/ir-tx` run 2).
  A carrier-off TX mode (`with_carrier_modulation(false)`) would serve these modules; it is a public API change, not done.
- esp-idf tier: later, out of scope.

## State
- [x] Design approved
- [x] `tamer::ir` implementation
- [x] Host tests passing (six real captures as golden data)
- [x] esp-hal bridge (`ir` module) and examples `hal_c3_ir_tx`, `hal_c3_ir_rx`; `just check-hal`, `clippy-hal`, `check-matrix-hal` and both example builds pass
- [x] `hal_c6_ir_tx` (GPIO 3), in `ci-riscv`; bench check `c6/ir-tx` prepared
- [x] End to end on hardware: C6 transmit → C3 receive, NEC, `c6/ir-tx` (2026-11-21, emitter module)
- [ ] Hardware verification (RMT carrier frequency; `hal_c3_ir_rx` with a NEC remote)
- [x] C6 → C3, 8-bit codec frames every 2 s against `receive(500)`: 16 of 16 decoded with the settle-after-abort fix, 1 of 15 without (2026-11-21)
- [x] C6 carrier near 38 kHz: a consumer device with a demodulating receiver follows the C6 at 3.5 m with the emitter module on 5V (2026-11-21)

## Bench — C6 transmit, C3 receive
- Emitter: `hal_c6_ir_tx` on the ESP32-C6-DevKitC-1, the OPEN-SMART emitter module on J1 15 / 14 / 13 (GND / 5V / GPIO 3).
- Judge: an ESP32-C3 DevKitM-1 with a demodulating receiver on GPIO 5, running a downstream receive bench that decodes NEC and logs `NEC address=0x00 command=0x45 (ok N failed M)`.
- The C6 side runs as `just bench c6 ir-tx --port <c6>`; it judges the clock, the tick pair and 20 frames sent.
- Pass: 20 consecutive NEC decodes at about 1 m in the receiver log, `failed 0`.
- With two boards on the host, every flash and run names its port.

### Results
- **Run 2, 2026-11-21 20:25 — pass.** Tree `ir-round` on 7896418 plus this change (uncommitted), frame gap 700 ms.
  - C6 DevKitC-1: `just bench c6 ir-tx` 2/2; boot `RMT clock 80000000 Hz`, `high 694 low 1411 ticks`, `carrier if group clock: 38005 Hz`, `carrier if channel clock (1 MHz): 475 Hz`, 20 × `sent NEC 0x00/0x45 (67 pulses)`.
  - C3 DevKitM-1 judge: 28 consecutive `67 pulses: NEC address=0x00 command=0x45`, ok 1 → 28, no failure after the reflash.
    The 8 failures before them are frames of the run-1 firmware, received before the reflash.
  - Logs: `bench/c6/logs/hal_c6_ir_tx-20261121-202534.log` (git-ignored) and the downstream project's receive-bench log (not tracked here).
  - 21:37: after a review renamed the 700 ms "period" to a gap (frames start about 768 ms apart, as `send` blocks for the 68 ms frame), the C6 side reran 2/2 on the reworded ready line; timing unchanged (`hal_c6_ir_tx-20261121-213739.log`).
- **Run 1, 2026-11-21 20:21 — fail (judge timing, not signal).** Frame gap 1000 ms.
  - C6 side passed 2/2; the judge decoded the first frame, then 64 frames arrived as 43 pulses, `not candle nor NEC`, 0 receive errors.
  - The missing 24 pulses are the header and the first 11 bits (about 29 ms); the rest is the NEC 0x00/0x45 tail with marks 516–594 us and spaces 536–590 / 1691–1748 us.
  - Cause: the judge re-arms `receive(1000 ms)` about 29 ms after each frame (20 ms idle plus its log line); a 1000 ms gap puts that timeout 29 ms into every next frame, and `receive` drops the frame in flight.
  - Fix: both TX examples now wait 700 ms; a frame then closes about 760 ms after the re-arm.
  - Logs: `bench/c6/logs/hal_c6_ir_tx-20261121-202105.log`, plus the downstream project's receive-bench log.
- What the pass shows: C6 RMT TX → emitter → 38 kHz band-pass receiver → C3 RMT RX → `FrameAssembler` → NEC decode, bit timing well inside tolerance.
- It also rules out the channel-clock case: a 475 Hz carrier (694 us on, 1411 us off) would split the 9 ms header mark into several pulses, and every frame arrived whole with 67 pulses.
- What it does not show: that the 38 kHz comes from the RMT rather than the module, which may generate its own (see Open Questions).
  A bare LED or a scope run confirms the 38005 Hz prediction for drivers where the RMT carrier is the only carrier.
- Distance: the emitter faced the receiver at the operator's placement (aimed at about 1 m, not measured).

## Session Log
- 2026-11-22 — Review: `IrReceiver` settle after a timeout floored at 10 ms (`RX_SETTLE_MIN`), independent of `idle_us`; Xtensa matrix green.
- 2026-11-21 — `c6/ir-tx` run 1 failed on judge timing (1 s gap = 1 s receive timeout); TX gap now 700 ms; run 2 passed with 28 consecutive decodes.
- 2026-11-21 — C6 transmit request: `hal_c6_ir_tx` on GPIO 3, NEC only (the C3 example lost its device-style frame), carrier predictions in both boot logs, `c6/ir-tx` prepared; first try with the emitter module as-is.
- 2026-11-20 — Request retired from the review queue; dates: RMT spike Thu 26 Nov, C3 transmit Sat 28 Nov (own fallback if unusable by Mon 23 Nov), event Sat 5 Dec; candle profile verified downstream
