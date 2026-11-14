# ADR-013: signwriter — one driver for 128x64 I2C OLEDs (SSD1306 and SH1106)

## Status
Accepted (2026-11-08); it adds a graphics-side crate beside `tamer` and leaves [ADR-008](008-display-scope.md) and [ADR-009](009-i2c-sensor-drivers-in-tamer.md) unchanged.

## Context
Firmware should drive whichever 128x64 I2C OLED is plugged in, and the module can be swapped while the device is running.
The two common controllers are the SSD1306 (0.96" modules) and the SH1106 (1.3" modules).
Both answer at address 0x3C, and neither has an ID register, so the controller cannot be read from the bus directly.

The surrounding constraints are these.

- The application should see one `embedded-graphics` `DrawTarget`, whatever controller is fitted and whichever tier runs it.
- The upstream `ssd1306` 0.10 crate pulls in `proc-macro-error` (RUSTSEC-2024-0370, unmaintained), so depending on it would need a `deny.toml` ignore.
- The upstream `ssd1306` crate also erases bus errors, has no read path, and flushes in horizontal addressing mode.
- The upstream `sh1106` 0.5 crate is still on `embedded-hal` 0.2.
- [ADR-008](008-display-scope.md) keeps `tamer` free of `embedded-graphics`.
- [ADR-009](009-i2c-sensor-drivers-in-tamer.md) puts I2C sensor drivers in `tamer`.
  - A display is not a sensor, so ADR-009 does not place it.
- The two controllers differ only in their init sequence and in a column offset of 0 or 2, so one driver can cover both.
- A spike on the C3 bench rig found a RAM read-back that tells the controllers apart, recorded in [project lore](../project-lore.md) under "OLED — telling an SSD1306 from an SH1106".
  - The read-back works on the SH1106 only, because the SSD1306 returns its status byte for every read.

## Decision
- Add a new workspace crate `signwriter`: `no_std`, allocation-free, licensed `MIT OR Apache-2.0`, MSRV `1.88`.
  - It depends on `embedded-hal` 1.0, `embedded-graphics-core` 0.4 and `tamer` (for `tamer::hotplug`).
  - The crate is written from scratch; no code is copied from any other project.
- One in-house driver covers both controllers.
  - The controllers differ only in the init table (`init_commands()`) and the column offset (`column_offset()`, 0 for the SSD1306 and 2 for the SH1106).
  - Both are driven in page addressing mode with dirty-page tracking.
  - Every I2C write is at most 31 bytes.
- The controller is detected before init, on every attach, with the spike's two-value read-back.
  - A value written to page 0, column 130, must read back for both test values to classify the panel as an SH1106.
  - A read that fails after the writes were acknowledged classifies the panel as an SSD1306 (its serial mode is write-only).
  - `Oled::with_controller` pins the controller and skips detection.
  - Anything else is classified as an SSD1306.
  - A bus error is returned as an error and the panel is not classified.
  - Status bytes are logged for diagnosis and never decide the controller.
- Hot-plug is driven by `tamer::hotplug::Hotplug`.
  - The application supplies a millisecond clock through `tick(now_ms)`.
  - signwriter never sleeps.
- Attach runs detect, init, a full flush of the application's current framebuffer, and then display-on.
  - The first visible frame is therefore the one the application drew.
  - Detach keeps drawing into the framebuffer, and the next attach shows it.
- A swap between two probes is handled by a manual `redetect()` call, and there is no periodic re-detection in v0.1.
  - The application calls `redetect()` after a swap it knows about.
  - The next attach also detects the controller again.
- The layering follows [ADR-010](010-shared-logic-ladder.md).
  - The stateless bus functions come first, then the I/O-free `Framebuffer`, then the blocking `Oled` runner.
  - A rung-3 action machine is deferred until an async runner exists.
- The bus is an owned generic `embedded_hal::i2c::I2c`.
  - Sharing one bus between devices is the application's choice, made with `embedded-hal-bus`.
- Release as one wave: `tamer` 0.2.0 ships `hotplug` first, then `signwriter` 0.2.0 (lockstep with the workspace version) is published against it.
  - Its `tamer` dependency carries both `path` and `version = "0.2"`, so it is publish-ready and still builds in the workspace.

## Consequences
- The two `*_c3_oled` examples run on `signwriter`, so neither tier depends on the upstream `ssd1306` crate and `deny.toml` needs no ignore for RUSTSEC-2024-0370.
- `signwriter` cannot be published before `tamer` 0.2.0 is live on crates.io, so the release order is fixed.
- The OLED driver is a second graphics-side crate, not part of `tamer`.
  - `tamer` stays `embedded-graphics`-free.
  - The layer-1 reuse rule of ADR-008 applies to panels with a maintained `embedded-hal` 1.0 driver.
  - No such driver exists for 128x64 OLEDs under these constraints.
- A clone that fails the read-back is treated as an SSD1306.
  - The init and flush paths are the same for both controllers apart from the column offset.
  - The risk is therefore a wrong offset on the panel and not a crash.
- A swap that happens entirely between two probes is not detected unless the application calls `redetect()`.
  - The column-130 write used by the read-back is off-screen on the SH1106.
  - It is overwritten by the full flush on the SSD1306.
  - The probe therefore does not leave visible artefacts.
- The rule rests on bench evidence from one rig; the on-device checks in the feature document are required before the `c3/oled` gate ticks.
- The address stays a positional argument, with no auto-scan in v0.1.

## Alternatives Considered
|                                  Alternative | Pros                                                       | Cons                                                                                       | Why Rejected                                                                           |
|---------------------------------------------:|:-----------------------------------------------------------|:-------------------------------------------------------------------------------------------|:--------------------------------------------------------------------------------------|
|      Upstream `ssd1306` and `sh1106` drivers | Existing `DrawTarget` drivers, no code of our own          | See note 1                                                 | See note 1                                                                            |
|              Upstream `ssd1306` only (SSD1306 only) | Smallest code, mature crate                                | SH1106 modules (common 1.3" panels) would not work                                         | Does not meet the goal of one driver for both controllers                              |
|                  Adapt an existing SH1106 driver | Faster start on the SH1106 side                            | Provenance and licence of copied code would have to be settled first                       | Writing from scratch is cheap at this size and keeps the licence clean                 |
|                  A display driver in `tamer` | One crate for the pure and the hardware-facing logic       | Puts `embedded-graphics` into `tamer` against ADR-008      | Breaks the graphics-free rule of `tamer`                                               |
|             Periodic re-detection every N ms | Catches swaps between probes without an explicit call      | Extra bus traffic on a live panel; the rule is unproven beyond one rig                     | Deferred; `redetect()` is the v0.1 mechanism, revisit after bench data                |
|             Type-state or rung-3 action machine | Compile-time state; a pure transition table                | No async runner exists yet to drive it; the state is small                                  | Premature per ADR-010; revisit when an async runner needs it                          |
|        ID-register detection | Would be one read                                          | Neither controller has an ID register                                                       | Not possible on either controller                                                       |
|          Shared bus owned by the driver | Simpler application code                                   | Forces one bus ownership model on every consumer                                            | Sharing belongs to the application through `embedded-hal-bus`                          |

Notes:
1. Upstream `ssd1306` and `sh1106` drivers.
   - Cons: advisory on the `ssd1306` dependency tree; `sh1106` is on `embedded-hal` 0.2; bus errors are erased; no read path for detection.
   - Why rejected: blocks the detection rule and the `embedded-hal` 1.0 tier; two drivers to keep in step.
2. A display driver in `tamer`: the pure core would gain a graphics dependency.
