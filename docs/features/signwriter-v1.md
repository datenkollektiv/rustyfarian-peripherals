# Feature: signwriter v1

## Goal
Firmware drives whichever 128x64 I2C OLED is fitted, an SSD1306 or an SH1106, and the module can be swapped while the device runs.
The application sees one `embedded-graphics` `DrawTarget` and a hot-plug event stream, and never handles the controller type itself.
The first consumers are the ESP32-C3 examples `hal_c3_oled` and `idf_c3_oled`, which move from the upstream `ssd1306` driver onto this crate.
The decision and its alternatives are recorded in [ADR-013](../adr/013-signwriter.md).

## Decisions
| Decision | Reason | Rejected Alternative |
|----------|--------|----------------------|
| New workspace crate `crates/signwriter`, `no_std`, alloc-free, `MIT OR Apache-2.0`, MSRV `1.88` | Graphics stay out of `tamer` ([ADR-008](../adr/008-display-scope.md)); the crate is a graphics-side driver | A display driver inside `tamer` behind a feature |
| One in-house driver for both controllers | They differ only in the init table and a column offset of 0 or 2 | Upstream `ssd1306` (advisory, bus errors erased, no read path) plus `sh1106` (on `embedded-hal` 0.2) |
| Written from scratch, no code copied | Keeps the licence and provenance clean; the driver is small | Adapting an existing SH1106 driver |
| Controller detected by a RAM read-back before every init | Neither controller has an ID register; the read-back only works on the SH1106 | Status-byte matching (a log line only, it varies between clones) |
| Hot-plug through `tamer::hotplug::Hotplug` | The attach/detach state machine is already pure and host-tested | A second debounce implementation in the driver |
| Manual `redetect()` for a swap that happens between two probes | Keeps idle bus traffic at zero; a fast swap is rare and the bench data is not in yet | Periodic re-detection every N ms |
| Owned generic `I2c` bus, no auto-scan of the address | Sharing is the application's choice through `embedded-hal-bus`; the address is known per board | A bus wrapper inside the driver; scanning 0x3C and 0x3D at boot |
| `tamer` and `signwriter` 0.2.0 released in one wave (lockstep version) | `signwriter` needs `tamer::hotplug`, which is not yet on crates.io | Publishing `signwriter` on a path dependency only |

## Constraints
- `signwriter` is `no_std` and allocation-free; every write is at most 31 bytes, which fits the 32-byte ESP FIFO.
- No `unwrap`, `expect` or `panic` on library paths; the only documented panic is `Oled::new` with `detach_errors == 0`, as in `Hotplug`.
- `tamer` takes no `embedded-graphics` dependency, not even feature-gated.
- `signwriter` depends on `tamer` for `hotplug` only; the `hal` feature of `tamer` is a dev-dependency for `MockI2c`.
- The workspace `justfile` gates `signwriter` with its own `signwriter_flags`, not with `host_flags`, so `tamer`'s `hal` feature stays off in `tamer`'s own tests.
- `signwriter` is written from the datasheets and the spike, not by copying third-party driver code.
- No upstream `ssd1306` dependency, so `deny.toml` needs no ignore for its RUSTSEC-2024-0370 (`proc-macro-error`) chain.

## Crate layout
- `src/lib.rs`: constants (`ADDRESS_PRIMARY` 0x3C, `ADDRESS_SECONDARY` 0x3D, `WIDTH` 128, `HEIGHT` 64, `PAGES` 8, `MAX_WRITE_LEN` 31), `Controller`, `Error<E>`, `Event`, crate docs and the doctest.
- `src/command.rs`: init tables and the address and page byte builders.
- `src/framebuffer.rs`: `Framebuffer` with a page-major `[u8; 1024]` buffer and a dirty-page mask; `DrawTarget<Color = BinaryColor>` and `OriginDimensions`.
- `src/bus.rs`: the stateless functions `probe`, `detect`, `init`, `display_on` and `write_page`.
- `src/oled.rs`: `Oled<I2C>`, the blocking runner, which is also a `DrawTarget`.
- Tests live beside each module under `#[cfg(test)]`, using `tamer::mock::MockI2c`.

## API sketch
- Controller and events: `Controller::{Ssd1306, Sh1106}` with `column_offset()` and `init_commands()`; `Event::{Attached(Controller), Detached}`.
- Errors: `Error<E>` with `Bus(E)` and `Detached`, `is_nack()` and `Display`.
- Stateless functions: `probe`, `detect`, `init`, `display_on` and `write_page`, each taking `&mut I2C` and an address.
- Runner: `Oled::new(i2c, address, attach_debounce_ms, detach_errors, probe_every_ms)` and `Oled::with_controller(..)` with a pinned controller, `tick(now_ms) -> Option<Event>`, `flush()`, `redetect(now_ms) -> Option<Event>`, `framebuffer()`, `controller()`, `is_attached()`, `last_error()` and `release()`.

An application loop draws into the framebuffer on every pass and lets `tick` decide what reaches the panel.
The loop never blocks on the bus, because `tick` is the only call that touches it.

## Detection rule
The rule comes from the bench spike recorded in [project lore](../project-lore.md) under "OLED — telling an SSD1306 from an SH1106".
For each of the two test values `0xA5` and `0x5A`, the driver:

1. Sets page 0 and column 130 with the command bytes `0x00 0xB0`, `0x02` and `0x18`.
2. Writes the test value as data after the `0x40` control byte.
3. Sets page 0 and column 130 again.
4. Reads two bytes after the `0x40` control byte; the first is a dummy byte and the second is the RAM value.

The panel is an SH1106 only when both values read back unchanged.
Any other result classifies it as an SSD1306, including a clone that answers differently.
A read that the device rejects after its writes were acknowledged also classifies it as an SSD1306, because the datasheet documents its serial mode as write-only while the SH1106 always supports RAM read-back.
A failure of the address-select or test-value writes is returned to the caller and the panel is not classified.
`Oled::with_controller(i2c, address, controller, attach_debounce_ms, detach_errors, probe_every_ms)` pins the controller and skips detection; attach and `redetect()` then go straight to init.
The status byte read after control `0x00` is logged for diagnosis and is never used to decide.

Column 130 is off-screen on the SH1106, and the full flush on attach overwrites it on the SSD1306.

## Hot-plug behaviour
- `tick` probes with `probe` (NOP command `0xE3`) only while detached, and only once `probe_every_ms` has elapsed.
- A successful probe is fed to `Hotplug::observe`; the panel attaches after `attach_debounce_ms` of continuous successes.
- On attach, the driver runs detect, init, marks every page dirty, does a full flush of the framebuffer, and then sends display-on (`AF`).
  The first visible frame is therefore the application's current buffer, and `AF` is sent only after the first full flush.
- If any attach step fails, the attach is treated as a failed probe and retried.
- While attached, every write result is fed to `Hotplug::observe`.
  `detach_errors` consecutive failures detach the panel and set `controller` to `None`.
- While attached and idle, the driver sends a NOP probe every `probe_every_ms`, so an unplug is noticed on a static screen.
- While detached, drawing continues into the framebuffer, and the next attach shows it.
- `flush()` while detached returns `Error::Detached` and does not touch the bus.
- `redetect(now_ms)` forces detect, init, full flush and display-on, and returns `Attached(controller)` when the controller changed.
  It is the only way to pick up a swap between two probes.

## Test plan
Host tests use `tamer::mock::MockI2c`, and every test ends with `.release().done()`.

- Detection: `detect_sh1106_both_values_match`, `detect_ssd1306_status_0x45`, `detect_one_match_is_ssd1306`, `detect_nack_is_error`, `detect_read_rejected_is_ssd1306`, `detect_write_nack_is_error`.
- Init: `init_bytes_ssd1306`, `init_bytes_sh1106`, `init_has_no_display_on`.
- Page writes: `write_page_offset_0_and_2`, `write_page_chunks_31`.
- Flush: `flush_only_dirty_pages`, `flush_midpage_nack_keeps_page_dirty_and_resends`, `flush_when_detached_errs_without_bus`.
- Framebuffer: `pixel_bit_mapping`, `pixel_clipping`, `dirty_only_on_byte_change`.
- Hot-plug: `tick_attach_after_debounce_full_flush_then_af`, `tick_detach_after_n_errors`, `tick_draws_while_detached_appear_on_reattach`, `tick_swap_sh1106_to_ssd1306_uses_offset_0`.
- Idle and retry: `tick_idle_probe_respects_interval`, `tick_attach_failure_retries`.
- Runner: `redetect_reports_changed_controller`, `last_error_set_on_detach`, `tick_attaches_ssd1306_that_rejects_reads`, `fixed_controller_skips_detect`, `fixed_controller_redetect_reinits_without_detect`.
- Plus the crate doctest.

Gates: `just test`, `just clippy`, `just doc` and `just verify` run the signwriter tests through `signwriter_flags`.
`just check-hal`, `just check-idf`, `just deny` (green, no OLED-related ignore), and `just build-example hal_c3_oled` and `idf_c3_oled` cover the tiers.

## Bench plan
- `bench/c3/oled.toml` runs on both tiers: the SSD1306 is attached after boot and the picture is confirmed, then a live swap to an SH1106 is checked by the logged `controller=` value, which must differ from the first run, then the module is detached and the detach is noticed.
- `hal_c3_oled_identify` stays as the standalone diagnostic for the read-back on a new panel.
- The bench gate `c3/oled` in `release-plan.md` ticks only after both modes pass on the same commit.

## Stages
- **Stage 1, spike (done):** the read-back and the init and address facts were measured on the C3 rig (`hal_c3_oled_identify`).
- **Stage 2, this feature:** the `signwriter` crate, `tamer` 0.2.0 with `hotplug`, the two C3 examples on `signwriter`, the `c3/oled` scenario, and the release wave.
- **Stage 3, done (hardware pass pending):** the C6 and classic ESP32 OLED examples on both tiers (`hal_c6_oled`, `idf_c6_oled`, `hal_esp_oled`, `idf_esp_oled`), with their I2C pins taken from a board profile ([ADR-014](../adr/014-board-profiles.md)); the ESP32 pair builds for both the `esp32-wroom` and the `feather-v2` profile; the S3 is `n/a`, because its I2C 6/7 is the touch bus with no free connector.
  The C3 OLED examples move onto the same profile with no pin change (SDA 6 / SCL 7).
  The Feather V2 (SDA 22 / SCL 20, socket power on GPIO 2) is a second ESP32 profile, so `hal_esp_oled` and `idf_esp_oled` run unedited on a WROOM-32 and on a Feather V2.
- **Stage 4, not scoped:** an async runner with a rung-3 machine, once an async consumer needs it ([ADR-010](../adr/010-shared-logic-ladder.md)); and a revisit of periodic re-detection after bench data.

## Open Questions
- [x] Does the read-back classify correctly with `0x5A` at column 130? Yes: `hal_c3_oled` on the C3 rig classified the SSD1306 and the SH1106 correctly across a live swap SSD1306 → SH1106 → SSD1306 (2026-11-08).
- [x] Is the SH1106 wire on the rig flaky? Yes, one wire on that module (2026-11-08); fix it before the live swap runs. The driver re-initialises after a brief contact loss.
- [ ] How often does a clone answer the read-back differently, and is the SSD1306 classification then a wrong column offset on screen?
- [ ] How long a swap between two probes can go undetected in practice, and does it justify periodic re-detection? Revisit after bench data.
- [ ] Pins for the C6 and classic ESP32 OLED examples (stage 3) come from their board profiles ([ADR-014](../adr/014-board-profiles.md)) and must be confirmed on each board; the Feather V2 (incl. its GPIO 2 power switch) is deferred to the next release, because the STEMMA QT cable is not on hand.

## State
- [x] Design approved
- [x] Core implementation
- [x] Tests passing
- [x] Documentation updated

## Session Log
- 2026-11-08 — Feature doc created; design approved with the maintainer decisions: manual `redetect()` only, `tamer` 0.2.0 and `signwriter` 0.1.0 in one wave, `signwriter` written from scratch.
- 2026-11-08 — Version decision changed: `signwriter` joins the lockstep workspace version and ships as 0.2.0 with `tamer` 0.2.0.
- 2026-11-08 — Stage 3 scoped: board profiles per tier ([ADR-014](../adr/014-board-profiles.md)), C6 and ESP32 OLED examples, S3 recorded `n/a`.
- 2026-11-08 — Stage 3 done in code: board profiles on both tiers, four new OLED examples, bench scenarios for C6, ESP32 and Feather V2 (split per controller on 2026-11-12); gates and `build-matrix` (16 builds) green; hardware runs pending.
