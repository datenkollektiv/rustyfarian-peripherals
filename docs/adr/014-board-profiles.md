# ADR-014: Board profiles for per-board pin choice

## Status
Accepted (2026-11-08); it extends the I2C pin choice of [ADR-012](012-esp32-c3-pin-map.md) and the bus pattern of [ADR-004](004-i2c-bus-pattern.md) without changing either's probe methodology.

## Context
Examples hard-code their pins, so every example names GPIO numbers for the board it was written on.
The I2C bus is the first place where one chip has more than one board in use.
An ESP32 example has to run unedited on two boards.

- An Adafruit Feather V2 has a STEMMA QT socket on SDA 22 / SCL 20, and the socket is powered through GPIO 2, which must be driven high.
- A plain WROOM-32 board uses SDA 21 / SCL 22, which is the de-facto ESP32 default.

The C6 DevKitC-1 uses SDA 21 / SCL 22, which is a hardware-tested convention from a sibling project.
The S3 CrowPanel has no free I2C connector, because its I2C pins 6 and 7 are the touch bus, so it gets no OLED example.

Pin choice cannot be a plain constant in the examples.
esp-hal pins are distinct types that are moved out of `Peripherals`, so a helper function cannot take "GPIO 21" as a value and return a typed pin.
A macro can name the pin in the expansion, move it out of `Peripherals`, and keep the rest of the example unchanged.
The same holds for the esp-idf tier, where pins are taken from `peripherals.pins.gpioN`.

The constraints are these.

- Examples must not carry board-specific GPIO numbers once they are portable.
- Library crates must build without a profile, so that a downstream consumer can depend on a tier without choosing a board.
- No `unsafe` and no `steal()` may be used to reach a pin.

## Decision
Each hardware tier gains one Cargo feature per board, and a public `board` module that is compiled only with exactly one `board-*` feature.

| Profile (Cargo feature) | Chip   | SDA / SCL | Bus power  | Default for |
|:------------------------|:-------|:----------|:-----------|:------------|
| `board-c3-supermini`    | C3     | 6 / 7     | none       | `c3`        |
| `board-c6-devkitc`      | C6     | 21 / 22   | none       | `c6`        |
| `board-esp32-wroom`     | ESP32  | 21 / 22   | none       | `esp`       |
| `board-feather-v2`      | ESP32  | 22 / 20   | GPIO 2 high | opt-in      |

The `board` module of each tier exposes the following.

- `board_i2c!(peripherals)` moves the profile's SDA and SCL pins out of `Peripherals`.
- `BOARD_NAME` and `I2C_PINS: (u8, u8)` are for log lines and carry no pin ownership.
- `board_i2c_power!(peripherals)` switches the bus power on where the board has a switch, as the Feather V2 does on GPIO 2, and expands to nothing on the other boards.

The default profile for each chip is added by `scripts/lib.sh` when a build passes no profile.
`just build-example <ex> <profile>` (the profile is a positional argument; `just flash` and `just run` take it too) overrides the default, so `just build-example hal_esp_oled feather-v2` builds the same example for the Feather V2.
The `board` module emits a `compile_error!` when two `board-*` features are enabled, and when a profile does not match the chip of the target.

The scope today is the I2C bus only.
The profile is extended with further pins (for example a button or a buzzer) when a second example needs them, and not before.

The OLED examples (the C3 pair, and the new C6 and ESP32 pairs) take their pins from the profile, and the other C3 examples keep their ADR-012 constants until they need it.

## Consequences
- The ESP32 OLED examples run unedited on the Feather V2 and on a WROOM-32, and the C6 and C3 OLED examples run on their own boards.
  The examples are portable across the boards of one chip.
- The bench gains a `feather-v2` board beside `esp32`, with the same chip and per-board build and flash commands that pass the `feather-v2` profile.
- The example names stay `{driver}_{chip}_{name}`, so no example file is duplicated per board.
- Other C3 examples keep their ADR-012 constants for now, so the C3 pin map is the same whether a profile is named or not.
- Libraries build without a profile, so a consumer that depends on a tier does not have to pick a board.
  Only examples and the `board` module need one.
- A profile is a compile-time choice.
  Moving one physical board to another profile is a rebuild, not a runtime switch.
- The `board` module is public API of each tier crate.
  Adding a profile or a pin to a profile is additive, but renaming a profile feature or a `board_i2c!` argument is a breaking change for the tier.
- Each profile is a claim about one board's wiring.
  A board that differs from the profile needs a new profile, and the wrong profile fails at the pin level, not at compile time.

## Alternatives Considered

|                                                        Alternative | Pros                                                                 | Cons                                                                                  | Why Rejected                                                                                  |
|-------------------------------------------------------------------:|:---------------------------------------------------------------------|:--------------------------------------------------------------------------------------|:----------------------------------------------------------------------------------------------|
|                          Per-board example files (`hal_esp_oled_feather_v2`) | No feature flags; each file is plain and obvious                     | Duplicates each example per board; the same logic is copied and drifts                | Multiplies examples with each board, against the demand-driven and single-example rule         |
|                        Runtime configuration (pin choice read at boot) | One binary for every board                                           | esp-hal pins are distinct types, so a runtime choice cannot return a typed pin without `unsafe` or `steal()` | Cannot meet the no-`unsafe` and no-`steal()` constraints                                      |
|                                  A board support crate (BSP) per board | Hides every pin behind a named type                                  | A new crate per board is heavy for a two-line pin choice; the repo is demand-driven     | Too much structure for the I2C bus alone; revisit if the profiles grow past pins              |

## Verification
- `just build-example hal_esp_oled feather-v2` and `just build-example idf_esp_oled feather-v2` succeed, and so do the default-profile builds of the other OLED examples.
- `just build-matrix` links the OLED examples for each chip, with `esp` built for both ESP32 profiles.
- Building with two `board-*` features, or with a profile for another chip, fails with the `compile_error!` message.
- `RUSTYFARIAN_BENCH=../rustyfarian-bench just bench-list` lists the `feather-v2` board and its `oled` scenario.
