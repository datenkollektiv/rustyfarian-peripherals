# Feature: IR codec v1

A pure infrared pulse codec, `tamer::ir` (feature `ir`), for the Krampusnacht props that drive and read consumer IR remotes.
The hardware bridge is `rustyfarian-esp-hal-peripherals::ir` (feature `ir`), `IrTransmitter` and `IrReceiver` on the RMT peripheral; the request is `review-queue/rustyfarian-peripherals-ir-codec-v1.md`.

Status: implemented, hardware unverified.

## Decisions
| Decision | Reason | Rejected Alternative |
|----------|--------|----------------------|
| One flat module `tamer::ir`, feature `ir = []`, no dependency | Matches `morse`/`tone`; NEC is only a preset, so not named `nec` | A `nec` module |
| Pulse list is `&[u32]` microseconds, mark first, odd length legal; `PulseBuf<N>` only for encode output | Natural RMT/timer width; the idle gap is never a pulse | `u16` pulses |
| `PulseCodec` is data with a validating positional `const fn new` returning `Result` | Invariants (1..=32 bits, distinguishable bits) hold; no panics | Public fields |
| Bits are classified by mark and space | The ALDI remote differs from NEC in both | Space only |
| Header mark is checked as an upper bound only; the header space is the anchor | Captures start with marks of 61..8721 us | Two-sided header mark |
| Pulse count is checked before bit classification | A 24-bit frame against NEC reports `WrongBitCount`, not a misleading bit error | Classify first |
| `Tolerance { Percent, Micros }`, inclusive, division-free matching | No divide on RISC-V C3 | Fixed windows as in the Python reference |
| Glitch filter merges: a short pulse is removed and the previous pulse reopened | Keeps alternation and total duration; the reference only dropped it | Drop and keep going |
| `FrameAssembler::edge(now, mark)` / `update(now)` with lazy reset; late start via explicit `arm` | Same idiom as `ToneSequencer`; stray space edges never open a frame | Implicit late-arm |
| NEC repeat: `NEC_REPEAT` and `is_nec_repeat` only (about 15 lines) | Cheap; the ~110 ms cadence stays with the caller | A repeat state machine |
| ALDI timings stay downstream; here only a test fixture | Device tables are not a `tamer` concern | An ALDI preset |
| `carrier_ticks(clock_hz, freq_hz, duty)` lives in `tamer` | The bridge does no arithmetic and the result is host-tested | Computing ticks in the tier |
| Bridge types `IrTransmitter` / `IrReceiver` take the RMT channel creator and the pin by value, generic over `TxChannelCreator` / `RxChannelCreator` | No per-chip cfg; ownership prevents sharing | Per-chip channel types |
| RX uses RMT RX, not GPIO edge capture | Hardware 1 us timestamps, no ISR, no static state | GPIO edge plus timer (kept as the fallback for gaps over 32 ms) |
| `IrReceiver::receive(timeout_ms)` blocks and re-arms per call, with the capture buffer on the stack | esp-hal's `RxTransaction` borrows its buffer, so it cannot live in the receiver; `Channel::reborrow` keeps the channel | A stored transaction (self-referential) |
| RMT frames are fed to `FrameAssembler` as edges with running-sum timestamps and an end gap of 0 | The RMT already delimited the frame; the 20 us glitch filter stays in `tamer` | Hardware glitch filter (about 3 us at most) |
| Global RMT clock 80 MHz (`IR_RMT_CLOCK`), channel divider 80, so 1 tick is 1 us | The one clock all four chips accept; `tamer` durations map 1:1 | Per-chip clocks |
| TX config: carrier level High, idle output Low, memsize 1 | The esp-hal default carrier level is Low and would carry the space | Defaults |
| NEC repeat: `NEC_REPEAT` / `is_nec_repeat` only, no repeat state machine | About 15 lines, host-tested; a repeat arrives as its own short RMT frame because the 20 ms idle is below the roughly 40 ms repeat gap | A repeat scheduler in the tier |
| `PulseSink` / `PulseSource` traits with `MockPulseSink` / `MockPulseSource`, implemented by `IrTransmitter` / `IrReceiver` | Firmware holding a bridge stays host-testable; mirrors `AnalogRead` / `MockAnalogRead` | Concrete RMT types only |
| `FrameAssembler::feed_segments` and `validate_pulses` hold the segment-to-pulse and pulse-list checks | Sans-io: the tier only maps `PulseCode` fields; edge cases are host-tested | Logic in the tier |
| Only headline types at the `tamer` root; error and parameter types stay under `tamer::ir::` | Names like `Tolerance` or `DecodeError` would clash as other codecs grow | Flattening everything |
| `is_nec_repeat` fixes the tolerance at 25 % | Cheap helper; a tuned codec does not retune it | A tolerance parameter |
| esp-idf tier: later | Maintainer decision; `rmt_tx_modulate_carrier` is group-clock based, so `carrier_ticks` fits it too | Mirroring now |

## Constraints
- `no_std`, alloc-free, panic-free; the pulse and edge feed is the seam, and `PulseSink` / `PulseSource` ship with mocks for the bridge.
- `FrameAssembler` has no locking; a tier that calls `edge` from an interrupt must serialise access.
- `update` must run at least once per end-of-frame gap, or two frames merge.
- Headerless codecs cannot tolerate a truncated first mark.

## Carrier-clock evidence
- esp-hal 1.2.2 writes `carrier_high` / `carrier_low` raw to the `CHn_CARRIER_DUTY` register and documents them only as "ticks"; the channel divider is a separate register.
- ESP-IDF v5.3.2 `rmt_tx.c` computes the TX carrier from `group->resolution_hz` ("carrier module works base on group clock"), and the group divider is 1 there, so that is the unscaled source clock, the equivalent of the `Rmt::new` frequency.
- The legacy IDF driver does the same (`rmt_source_clk_hz / carrier_freq_hz`).
- RX demodulation counts in the channel clock instead; the bridge does not use it.
- At 80 MHz and 38 kHz: total 2105 ticks, high 694, low 1411, about 38.00 kHz and 32.97 %.
- esp-hal writes the half-periods raw, with no `- 1`; if the TRM counts `N + 1` ticks the carrier is about 0.1 % off, which the scope check also covers.
- This is source-derived only.

## Open Questions
- **Hardware check, not yet done:** scope or logic analyser on GPIO 10 must show a 38 kHz, about 33 % burst per mark, no carrier in spaces, line low at rest.
  A reading near 475 Hz means the channel clock is used and `carrier_ticks` must be fed `clock / divider`.
- **Hardware check, not yet done:** `hal_c3_ir_rx` against a real remote must print the same command as the MicroPython receiver, with the first captured pulse a Low mark (a leading High segment is ignored by the assembler either way) and a repeat showing as a separate short frame.
- **Hardware check, not yet done:** `hal_c3_ir_tx` aimed at the ALDI candle, or at a TSOP feeding `hal_c3_ir_rx` on a second board.
- RMT idle threshold tops out at 32767 us at 1 us ticks, so the reference's 150 ms end gap is not reachable.
  The bridge delimits frames with a 20 ms idle: above the longest internal gap of NEC and the candle remote (the 4.5 ms header space), below the NEC repeat gap.
  Protocols with internal gaps over 20 ms split into several frames; a GPIO-edge receiver feeding `FrameAssembler` would be needed for them (deferred).
  The six real captures have no internal gap above 9 ms.
- `IrReceiver::receive` captures nothing between two calls and drops a frame still in flight at the timeout; a stored non-blocking `poll()` is deferred.
- The hardware glitch filter counts group-clock cycles in an 8-bit register (about 3 us at 80 MHz), so it stays off.
- `IrTransmitter::send` busy-waits for the frame (about 70 ms for NEC).
- C3 pins follow ADR-012: TX on GPIO 10 (NPN output row, shared with the buzzer), RX on GPIO 4 (3-pin pogo row).
- Compile-verified only on C6, ESP32 (no RX wrap, `CODES` must fit one RAM block) and S3.
- esp-idf tier: later, out of scope.

## State
- [x] Design approved
- [x] `tamer::ir` implementation
- [x] Host tests passing (six real captures as golden data)
- [x] esp-hal bridge (`ir` module) and examples `hal_c3_ir_tx`, `hal_c3_ir_rx`; `just check-hal`, `clippy-hal`, `check-matrix-hal` and both example builds pass
- [ ] Hardware verification (carrier, receive, end to end)
