# Feature: Morse Keyer v1

A pure, sans-IO Morse-code keyer for `tamer`.
It encodes a caller-owned, borrowed `&[u8]` ASCII message into International (ITU-R M.1677-1) Morse code.
It reports the on/off key state a downstream buzzer/LED adapter should be driving, via a separate `output()` query.
This is `tamer`'s second output/actuator primitive, after `tone`.
The module imports no GPIO, PWM, or chip crate; the hardware write is entirely downstream.

It exists because the `tone::ToneSequencer` arpeggio examples only work on a **passive** piezo, which is driven by varying the PWM frequency.
An **active** electromagnetic buzzer (e.g. TMB12A03) has a fixed internal oscillator and can only be gated on/off, which is exactly the shape Morse keying needs.
This feature adds the missing "active buzzer" story and splits the buzzer examples by buzzer type.

## Background

- **Why `tamer`.** Same charter as `tone`: pure output/actuator logic, host-testable, no chip coupling. Morse keying is timing logic over a message, so the natural home is the pure core.
- **Why mechanism name (`morse`).** Matches the crate convention (`debounce`, `rotary`, `button`, `tone`); no chip coupling or datasheet justifies a device name.
- **Why in-house, not a crate.** `tamer` is `no_std`, alloc-free, and near-dependency-free (only `micromath`, for `tilt`). The ITU code table is about 40 `const` entries; a Morse dependency would violate that policy for no gain.
- **Why borrowed `&'msg [u8]`.** Zero-alloc, `Copy`-cheap; a demo message is naturally a `const` / `&str::as_bytes()`. Matches `ToneSequencer`'s borrowed-slice precedent.

## Decisions

|                                                                                        Decision | Reason                                                                                                    |
|------------------------------------------------------------------------------------------------:|:----------------------------------------------------------------------------------------------------------|
|                                    Land as `tamer::morse`, the second output/actuator primitive | Follows `tone`; pure timing-over-message logic belongs in the core                                        |
|                                                     Name `morse` (mechanism), not a device name | Matches crate convention; no chip coupling                                                                |
|                                                 In-house `const` ITU table, no Morse dependency | Preserves `tamer`'s `no_std` / alloc-free / near-zero-dep policy                                          |
|                                          Borrow `&'msg [u8]` ASCII message, not an owned buffer | Zero-alloc; mirrors `ToneSequencer`'s `&'notes [Note]`                                                    |
|                                        Timing-agnostic: caller passes `unit_ticks` (dot length) | Speed is a policy the example owns (a `Speed` enum); the core stays pure timing                           |
|      Tick contract mirrors `tone`/`debounce`/`button`: `u64`, `saturating_sub`/`saturating_add` | Consistency with established sibling primitives, and panic-free near `u64::MAX`                           |
|   Advance accumulates schedule (`since = since.saturating_add(duration)`), not rebased to `now` | **Deliberate divergence** from edge detectors: the internal schedule must not drift under jittery polling |
|                            `output()` is a separate side-effect-free query returning `KeyState` | Lets the downstream adapter re-read without `Option` branching (mirrors `tone`/`presence`)                |
|                                                  `KeyState { Off, On }` enum, not a bare `bool` | Call-site readability and future-proofing; matches the crate's `Edge`/`Polarity` enum precedent           |
|                                  Events filtered to `CharacterStarted(usize)` + `Finished` only | Intra-character and word gaps are numerous and not actionable; only character starts aid logging          |
|                `CharacterStarted` payload is the **byte index into the original message slice** | Unambiguous under skipped bytes/spaces; mirrors `ToneSequencer`'s index-into-the-slice payload            |
|                                        Unsupported bytes are silently skipped and cost no ticks | A skip-byte carries no schedule; it is resolved eagerly in the lookahead scan, transparent to timing      |
|                                          A space is a 7-unit word gap; runs of gaps never stack | ITU word spacing; collapsing avoids unbounded stacked silence, including across the loop seam             |
|              Empty / all-unsupported message has no keyable content and is immediately finished | Panic-free by construction; matches `ToneSequencer`'s empty-slice spirit                                  |
|                                   No `hal`-feature adapter in v1; values only, write downstream | Mirrors `tone`; stay demand-driven until a rustyfarian crate needs the glue                               |

## Behavioral contract

The mapping the implementation and its tests must satisfy.

- **ITU timing, parameterized by `unit_ticks`.** dot = 1u, dash = 3u, intra-character gap = 1u, inter-character (letter) gap = 3u, word (space) gap = 7u, where `u = unit_ticks`.
- **`update()` advances at most one segment per call, and emits at most one event.** A "segment" is one dot, dash, or gap. Reaching a boundary advances exactly one segment; `update` never loops internally past more than one boundary. Most segment transitions emit no event (only entering a new character's first symbol emits `CharacterStarted`, and completion emits `Finished`).
- **The internal schedule is drift-free, but audible catch-up is bounded by the poll rate.** On advance, the baseline moves by the expiring segment's own duration (`since = since.saturating_add(duration_ticks)`), never rebased to `now`, so poll lateness is never baked into later segments. However, because at most one segment advances per call, a caller that falls behind will physically hold each overdue segment for one poll interval until it catches up. **Operational requirement:** poll substantially faster than `unit_ticks` (the examples poll every 5 ms against a >=60 ms dot). This is a **deliberate divergence** from edge detectors, mirroring `ToneSequencer`.
- **`output()` is total and side-effect-free.** It returns `KeyState::On` during a dot or dash, `KeyState::Off` during any gap, and `Off` when inactive or finished.
- **`CharacterStarted(i)` fires on entering a keyed character's first symbol, where `i` is that character's byte offset in the original `message` slice.** Under skipped bytes or spaces, `i` is the byte offset of the encodable character, not a count of characters played. The very first keyed character does not fire this event via `update`, because it is already loaded by `start()` (mirroring `ToneSequencer`'s first note). The caller logs that keying has started after `start()` (the examples name the message); the contract is that events report *transitions*, not every character played.
- **`Finished` fires exactly once (`OneShot`).** Reaching the end returns `Some(Finished)` once; every later `update` returns `None` and every `output()` returns `Off`, until `start()`.
- **`Loop` never finishes.** Reaching the end wraps to the first encodable character and emits `Some(CharacterStarted(i))`, where `i` is that first encodable character's byte offset (not necessarily `0`). `is_finished()` stays `false` indefinitely.
- **Unsupported bytes are transparent.** Any byte outside `A-Z` / `a-z` (folded) / `0-9` / space is skipped: no segment, no gap, no event, no tick cost. `b"C!Q"` keys identically to `b"CQ"`.
- **Spaces are meaningful timed content, and gaps never stack.** A space produces one 7-unit word gap; a run of spaces and skip-bytes collapses to a single 7-unit gap, never stacked.
- **The loop seam carries exactly one word gap, regardless of edge spaces.** In `Loop` mode the silence between one pass's last keyed symbol and the next pass's first keyed symbol is always exactly one 7-unit word gap, collapsing any trailing spaces, the seam, and any leading spaces. A leading edge space therefore produces no gap before the first pass's first symbol either: `Loop` always begins keying immediately. `b"CQ "`, `b" CQ"`, and `b" CQ "` all loop with exactly one 7-unit gap between Q and the next C. (`OneShot`, by contrast, honors leading and trailing spaces literally.)
- **Non-monotonic `now` never panics or advances spuriously.** All elapsed arithmetic uses `now.saturating_sub(since)`; out-of-order timestamps clamp elapsed to zero.
- **Content behavior by message shape.** Empty (`&[]`) or all-unsupported (no encodable character and no space) has no keyable content: `is_finished()` is `true` immediately and `output()` is `Off`. An all-space message is timed content: `OneShot` holds `Off` for one 7-unit word gap then fires `Finished` once; `Loop` is perpetually `Off`, never finishes, and emits no events (bounded work per call, no inner loop).
- **`unit_ticks == 0` advances one segment per `update`, never looping internally.** This matches `ToneSequencer`'s zero-duration-note contract: pathological but panic-free.
- **`start()` always resets to the first encodable character with a fresh baseline**, regardless of prior state, and clears `finished`.
- **`stop()` suspends and silences without marking the keyer finished.** After `stop()`, `output()` is `Off` and `update()` is a no-op until the next `start()`; `is_finished()` is unchanged (it reflects only natural `OneShot` completion, so it stays `false` unless the keyer had already finished).
- **Clock rollover is outside v1's contract.** The keyer never panics near `u64::MAX` (saturating arithmetic), but because a saturated baseline can stall advancement, completion is not promised across a monotonic-clock wraparound past `u64::MAX`.

## Constraints

- Pure `no_std`, alloc-free, host-testable; **no GPIO / PWM / HAL / chip crate coupling**. MSRV 1.88.
- Zero-allocation: a slice reference plus scalar fields only. The segment stream is walked lazily, never materialized, because a message's segment count is data-dependent.
- Tick type and non-monotonic handling match `tone`/`debounce`/`button`: caller-owned `u64`, `saturating_sub`, `saturating_add`. Same deliberate schedule-accumulation divergence as `tone`.
- Value types derive `Debug, Clone, Copy, PartialEq, Eq, Hash`; constructors are `const fn` where possible; `#[must_use]` on `Option`/state-returning methods; no `Result` in the public API.
- No panicking paths anywhere in the module, including content-free messages and `unit_ticks == 0`.
- No hardware trait, so **no `Noop*` mock is required** (matches `tone` / `range_map` / `smoothing`).
- Enums are not `#[non_exhaustive]`, matching the crate-wide posture (`SequenceEvent`/`SequenceMode`/`ButtonEvent`).

## Module & API surface (v1)

`tamer::morse`:

```rust
pub const fn code_for(byte: u8) -> Option<&'static str>;
pub const DOT_UNITS: u64;
pub const DASH_UNITS: u64;
pub const INTRA_CHAR_GAP_UNITS: u64;
pub const INTER_CHAR_GAP_UNITS: u64;
pub const WORD_GAP_UNITS: u64;

pub enum KeyState { Off, On }
pub enum MorseMode { OneShot, Loop }
pub enum MorseEvent { CharacterStarted(usize), Finished }

pub struct MorseKeyer<'msg> { /* private fields */ }

impl<'msg> MorseKeyer<'msg> {
    pub const fn new(message: &'msg [u8], unit_ticks: u64, mode: MorseMode) -> Self;
    pub fn start(&mut self, now: u64);
    pub fn stop(&mut self);
    pub fn update(&mut self, now: u64) -> Option<MorseEvent>;
    pub fn output(&self) -> KeyState;
    pub const fn is_finished(&self) -> bool;
    pub const fn is_active(&self) -> bool;
}
```

`code_for` uppercase-folds and returns the ITU pattern of `.`/`-` for `A-Z` and `0-9`, and `None` otherwise (including space, which the walker handles separately as a word gap).
It is a `const fn match` over `u8`, which compiles to a compact jump table and avoids ASCII-gap index arithmetic.
Key entries are `b'C' => "-.-."` and `b'Q' => "--.-"`.
The exact `.`/`-` strings become stable per ITU-R M.1677-1 once shipped.

Export policy mirrors `tone` exactly, resolving root re-export versus prelude cut:

- **Crate root** (`tamer::`) re-exports all four public types: `KeyState`, `MorseEvent`, `MorseKeyer`, `MorseMode`. This matches `tone`, whose `lib.rs` re-exports all five of its types.
- **Prelude** (`tamer::prelude`) exports only the construction-facing hero types `MorseKeyer` and `MorseMode`. This matches `tone`'s prelude, which exports `ToneSequencer`/`SequenceMode`/`Note` and excludes the return/event value types.
- `lib.rs` gets `pub mod morse;`, the root `pub use`, the prelude entry, and a `morse` bullet in the module-doc "Status" list, all mirroring the `tone` block.
- No `hal` feature surface: values only; the hardware write is downstream.

## Required tests (host)

Mirrors `tone`'s coverage plus Morse-specific cases:

- `code_for` returns the correct patterns for `C` and `Q`, folds lowercase, and returns `None` for space, punctuation, and newline.
- The exact `CQ` `OneShot` on/off timeline at `unit_ticks = 10` (the canonical table below), asserting boundary-inclusive advance and the 3-unit inter-character gap between C and Q.
- `CQ` `Loop` continuation: a 7-unit word gap before the wrap, `CharacterStarted` for C's byte offset on wrap, and it never finishes.
- Gap sizes: intra-character = 1u, inter-character = 3u, word = 7u; consecutive spaces collapse to one 7u gap (`b"A  B"`).
- Skip-bytes are transparent and tick-free: `b"C!Q"` matches the `b"CQ"` timeline and events.
- Event indexes reflect byte offsets under skipping: `b"!A B"` fires `CharacterStarted` with the byte offsets of `A` and `B`, not `0` and `1`.
- A late poll demonstrates the documented bounded catch-up: after skipping several boundaries, successive `update` calls advance exactly one segment each.
- Leading space delays the first `OneShot` symbol by one word gap; a trailing space adds a word gap before `Finished`.
- Loop seams collapse to one 7u gap for `b"CQ "`, `b" CQ"`, and `b" CQ "`, and `Loop` begins keying immediately with no leading gap.
- Content behavior: empty slice and all-unsupported are immediately finished; all-space `OneShot` fires one `Finished` after 7u; all-space `Loop` emits zero events and stays `Off`.
- `unit_ticks == 0` advances one segment per `update`, `Finished` on the final call, with no panic or inner loop.
- Non-monotonic `now` saturates without advancing spuriously; near-`u64::MAX` timestamps never panic, with completion asserted only below saturation (rollover is out of scope).
- `start()` restarts from the first encodable character and re-baselines, including when the first segment is a leading word gap.
- `stop()` silences `output()`, suspends `update`, and leaves `is_finished()` unchanged.

**Canonical `CQ` `OneShot` timeline** (`unit_ticks = 10`, `start(0)`, `-.-.` `--.-`): key `On` from `t=0`; C's symbol boundaries fall at 30/40/50/60/90/100, its final dot ends at 110 where a 3u inter-character gap holds `Off` to 140, at which point `CharacterStarted(1)` fires; Q's symbols run to 270, then `Some(Finished)` at 270 with `output()` `Off` thereafter.
Under `Loop`, `t=270` enters a 7u word gap and `t=340` fires `CharacterStarted(0)`, repeating shifted by +340 forever.
The precise per-`update` table lives in the `rust-engineer` design output and becomes the on-device timeline test.

## Device examples

Two new examples both drive an **active** buzzer by pure GPIO on/off (no LEDC/PWM), keying `CQ` in a loop.
Each reads `MorseKeyer::output()` every ~5 ms poll and sets GPIO6 high or low.

### Shared

- **Pin.** GPIO6 push-pull output, the same safe choice the arpeggio examples justify (not strapping 2/8/9, not on-board WS2812 8, not USB 18/19, not UART 20/21, not in-package flash 11-17). Only the load changes versus the arpeggio example, not the pin.
- **`Speed` enum**, local to each example and not part of `tamer`: `Relaxed` (100 ms dot, about 12 WPM) and `Standard` (60 ms dot, about 20 WPM), with `const fn unit_ms(self) -> u64`, selected by one `const SPEED`. `Relaxed` is the default. `Standard` is labelled hardware-dependent: see the caveat below.
- **Poll loop.** A millis clock feeds `keyer.update(now_ms)`, whose returned `MorseEvent` is logged. After `start()`, the example logs that keying has started (naming the message), because the first character emits no `CharacterStarted` event. A change-detected `output()` drives `set_high`/`set_low`. Change detection updates its cached `last` state only after a successful GPIO write, so a failed write is retried on the next poll rather than silently desyncing the pin.

### `hal_c3_active_buzzer_morse` (esp-hal 1.1.0, `no_std`/`no_main`)

Verified against the pinned esp-hal 1.1.0 GPIO API. `OutputConfig::default()` already resolves to push-pull drive; `set_high`/`set_low` are infallible and return `()`:

```rust
use esp_hal::gpio::{Level, Output, OutputConfig};

let mut buzzer = Output::new(peripherals.GPIO6, Level::Low, OutputConfig::default());
buzzer.set_high();
buzzer.set_low();
```

The clock is `Instant::now().duration_since_epoch().as_millis()`, the delay is `Delay::delay_millis`, and logging is `esp_println::println!`, alongside a panic handler, `#[main]`, and `esp_bootloader_esp_idf::esp_app_desc!()`.
The Cargo stanza is `required-features = ["esp32c3", "rt", "unstable"]`.

### `idf_c3_active_buzzer_morse` (esp-idf-hal 0.46.2, std)

Verified against the pinned esp-idf-hal 0.46.2 GPIO API. Here `set_high`/`set_low` are fallible, so the example matches on the result, logs a warning on error, and advances `last` only on success:

```rust
use esp_idf_hal::gpio::PinDriver;

let mut buzzer = PinDriver::output(peripherals.pins.gpio6)?;

let write = match out {
    KeyState::On => buzzer.set_high(),
    KeyState::Off => buzzer.set_low(),
};
match write {
    Ok(()) => last = Some(out),
    Err(err) => log::warn!("buzzer GPIO write failed, will retry: {:?}", err),
}
```

The clock is `std::time::Instant`, the delay is `FreeRtos::delay_ms`, and logging is `log::info!`/`warn!`, alongside `link_patches()`, `EspLogger::initialize_default()`, and an `anyhow::Result` main.
The Cargo stanza is name-only, with no `required-features`, matching the other `idf_*` examples.

### Wiring (both tiers)

The TMB12A03 is an active electromagnetic buzzer with a built-in oscillator, so keying GPIO6 high sounds it and keying it low silences it.
It is wired directly to the pin.

```text
Active buzzer (TMB12A03)   ESP32-C3
─────────────────────      ────────
+ (signal)                 GPIO 6
− (ground)                 GND
```

Note: the TMB12A03 draws up to about 30 mA at 3 V (sampled units roughly 20-25 mA), at the upper end of what a C3 GPIO should source.
A small, high-impedance unit runs directly off the pin as wired above.
For a loud or low-impedance buzzer, or to stay well within the GPIO's current limit, drive it through a small NPN transistor (GPIO6 through a base resistor, buzzer on the collector, emitter to GND) with a flyback diode across the buzzer, rather than direct drive.

**Caveat: `Standard` (60 ms dot) is hardware-dependent.** Some TMB12A03 datasheets list an acoustic response time up to 50 ms at the lowest operating voltage. A 60 ms dot leaves little margin, especially with 5 ms polling, so keep `Relaxed` (100 ms) as the default and verify `Standard` on the exact unit before relying on it.

## Rename (passive arpeggio examples)

The existing passive-piezo arpeggio examples are renamed to disambiguate them from the new active-buzzer Morse examples by both buzzer type and pattern:

- `hal_c3_buzzer` becomes `hal_c3_passive_buzzer_arpeggio`.
- `idf_c3_buzzer` becomes `idf_c3_passive_buzzer_arpeggio`.

The rename touches **live references only**:

- The two `.rs` example files, via `git mv` to preserve history.
- Both crates' `Cargo.toml` `[[example]]` `name` fields.
- The in-file `//! just build-example` and `//! just flash` doc lines.
- `AGENTS.md`'s naming-convention example, which currently cites `hal_c3_buzzer`.

**Historical records are left untouched.**
Released `CHANGELOG.md` entries and `docs/features/archive/tone-sequencer-v1.md` keep the old names, so history stays honest.
A single new `## [Unreleased]` CHANGELOG entry records the rename, the new `tamer::morse` module, and the two active-buzzer examples.

## Deferred (explicitly decided, not open)

- **`hal`-feature GPIO adapter** stays downstream and demand-driven, mirroring `tone`.
- **Punctuation, prosigns, and extended charset** are out of v1; only `A-Z` and `0-9` are supported. Add on real demand.
- **`total_duration_ticks()` / `current_character_index()` convenience** is speculative; add when a consumer needs it.
- **Runtime speed or message change without `start()`** is not needed for the demo.
- **Clock rollover past `u64::MAX`** is out of v1's contract (no panic is guaranteed; completion across rollover is not).

## Status

Shipped 2026-07-30 and archived on completion.
The release entry lives in the root `CHANGELOG.md`, and the implementation trail (core, examples, passive-example rename, review rounds) is in git history.
Real-hardware validated: `hal_c3_active_buzzer_morse` keys `CQ` on an ESP32-C3 SuperMini with a TMB12A03-labelled active buzzer, GPIO6 direct-driven at 40 mA drive strength.
