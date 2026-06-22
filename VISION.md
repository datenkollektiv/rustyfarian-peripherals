# Project Vision

## North Star

One home for every hardware peripheral the rustyfarian ecosystem touches —
buttons, encoders, buzzers, displays, LEDs — each built as pure, host-testable
logic behind a thin hardware wrapper, so a new device never means a new repo.

## Long-Term Goals

- **Pure, host-testable peripheral logic.**
  Debounce, rotary quadrature decoding, button-event timing, tone/duration
  sequencing, text layout and framebuffer diffing — all live in `tamer` as plain
  `no_std` Rust with no hardware dependency, fully unit-testable on a laptop. No
  peripheral lands without its decode/render logic in the pure core.

- **Thin, trait-first hardware tiers.**
  Keep `rustyfarian-esp-hal-peripherals` (bare-metal) and
  `rustyfarian-esp-idf-peripherals` (std) thin: they read pins and push bytes,
  and delegate all logic to `tamer`. Every hardware interaction sits behind a
  trait, and every trait ships with a `Noop*` mock.

- **One repo, many peripherals — input *and* output.**
  Buttons, switches, rotary encoders, piezo buzzers, 7-segment and OLED
  displays, addressable LEDs all live here. New device types are added in place
  rather than as new repos. `rustyfarian-ws2812` is a candidate to fold in.

- **A catalogue that grows on demand.**
  The set of peripherals is not predefined — it grows in response to real
  downstream requests. "Done" is the state where rustyfarian apps consistently
  find the peripheral driver they need without forking or copying code.

- **Ecosystem currency.**
  Timely adoption of new ESP32 chip variants and HAL releases on both the
  esp-hal and esp-idf tiers, in step with the sibling rustyfarian repos.

## Target Beneficiaries

Developers building battery-powered ESP32 applications in the rustyfarian
ecosystem — for example, a remote field sensor with a config knob, a button, a
buzzer, and a small status display — who want clean, tested peripheral drivers
rather than re-writing debounce, quadrature, tone, and framebuffer code per
project.

Primary today: the maintainer's own downstream project(s).
Secondary: any embedded developer who discovers and adopts the crates.

## Supported Platforms

- **ESP32 (RISC-V and Xtensa)** via `rustyfarian-esp-idf-peripherals` (std,
  ESP-IDF) and `rustyfarian-esp-hal-peripherals` (no_std, esp-hal).

Additional MCU families are not pursued proactively, but remain open if a
genuine use case arises and the pure-logic discipline can be preserved.

## Non-Goals

- **Network and radio.**
  Wi-Fi, MQTT, LoRaWAN, and ESP-NOW belong in `rustyfarian-network`. Network
  chips are not peripherals in this repo.
- **Application-level business logic.**
  Deciding *what* a button press means, or *what* a display should show, is the
  application's job, not the library's.
- **Hardware-only drivers with no pure core.**
  A peripheral shipped as a thin esp wrapper with no host-testable logic in
  `tamer` violates the discipline. The pure-core / thin-tier split is
  non-negotiable, output peripherals included.
- **Predefined exhaustive peripheral catalogues.**
  Peripherals are added on demand, not speculatively.
- **Proactive expansion to additional MCU families** beyond ESP32.

## Success Signals

- A new rustyfarian application can wire up a debounced button, a rotary knob, a
  buzzer, or a status display in minutes, against traits, with the library's own
  `Noop*` mocks in its tests.
- All decode and render logic remains fully unit-testable on a laptop without an
  ESP toolchain or hardware.
- When a new peripheral is needed, it is added here — no new repo is spun up for
  it.
- The hardware tiers stay current with their HAL release cadences, and
  downstream crates rarely break.

## Open Questions

- **`ws2812` merge:** should the WS2812 / NeoPixel effects fold into this repo as
  just-another-output-peripheral, or stay a sibling? It was the first peripheral
  and predates this repo's broadened scope. Decide when the boundary is next
  tested by real use.
- **Power / charging grey zone:** do battery-charging and power-management
  devices count as peripherals here, or stay in `rustyfarian-power`? Currently
  leaning `rustyfarian-power`; revisit if a charging IC needs a driver.
- **Interrupt vs. polling boundary:** should `tamer` expose its state machines as
  both poll-driven (tick with a sampled level) and event-driven (fed from a
  pin-change interrupt), or standardise on one? Resolve when the first
  interrupt-driven consumer appears.
- **Where generic pin/bus abstractions stop and `embedded-hal` begins.** The pure
  core should lean on `embedded-hal` traits rather than reinventing them; revisit
  if a downstream need exposes a gap.

## Vision History

- 2026-06-22 — Initial onboarding vision established the repo as **input-only**
  peripherals (pins, debounce, rotary, button events), with all output pushed to
  `rustyfarian-ws2812` and a deliberate input/output split.
- 2026-06-22 — Vision **re-derived from scratch** (AI onboarding draft dropped).
  Scope broadened from "input peripherals" to **all hardware peripherals — input
  and output** (piezo buzzers, 7-segment / OLED displays, addressable LEDs now
  included), motivated by not wanting to spawn a new repo per device type;
  `ws2812` may fold in. Network chips remain out of scope (`rustyfarian-network`);
  battery / power is recorded as a named grey zone. The pure-`tamer`-core plus
  thin-hardware-tier discipline is retained and reaffirmed as non-negotiable —
  now the project's true spine, in place of "input".
