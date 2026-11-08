//! Board profiles: the I2C bus (and its power switch) of a known dev board.
//!
//! Examples ask the profile instead of naming GPIOs, so one example runs on
//! several boards of the same chip without edits.
//! The module exists only when exactly one `board-*` feature is enabled
//! (`scripts/lib.sh` appends the chip's default profile; ADR-014).
//!
//! | Feature              | Chip   | SDA / SCL | Bus power    |
//! |:---------------------|:-------|:----------|:-------------|
//! | `board-c3-supermini` | ESP32-C3 | 6 / 7   | none         |
//! | `board-c6-devkitc`   | ESP32-C6 | 21 / 22 | none         |
//! | `board-esp32-wroom`  | ESP32    | 21 / 22 | none         |
//! | `board-feather-v2`   | ESP32    | 22 / 20 | GPIO 2 high  |
//!
//! Enabling two profiles, or a profile whose chip feature is not enabled, is a
//! compile error.
//!
//! The macros are exported at the crate root
//! (`rustyfarian_esp_hal_peripherals::board_i2c!`).
//!
//! ```ignore
//! let peripherals = esp_hal::init(esp_hal::Config::default());
//! let _bus_power = rustyfarian_esp_hal_peripherals::board_i2c_power!(peripherals);
//! let (sda, scl) = rustyfarian_esp_hal_peripherals::board_i2c!(peripherals);
//! let i2c = I2c::new(peripherals.I2C0, config)?.with_sda(sda).with_scl(scl);
//! ```

#[cfg(any(
    all(feature = "board-c3-supermini", feature = "board-c6-devkitc"),
    all(feature = "board-c3-supermini", feature = "board-esp32-wroom"),
    all(feature = "board-c3-supermini", feature = "board-feather-v2"),
    all(feature = "board-c6-devkitc", feature = "board-esp32-wroom"),
    all(feature = "board-c6-devkitc", feature = "board-feather-v2"),
    all(feature = "board-esp32-wroom", feature = "board-feather-v2"),
))]
compile_error!("enable at most one `board-*` feature");

#[cfg(all(feature = "board-c3-supermini", not(feature = "esp32c3")))]
compile_error!("`board-c3-supermini` needs the `esp32c3` chip feature");
#[cfg(all(feature = "board-c6-devkitc", not(feature = "esp32c6")))]
compile_error!("`board-c6-devkitc` needs the `esp32c6` chip feature");
#[cfg(all(
    any(feature = "board-esp32-wroom", feature = "board-feather-v2"),
    not(feature = "esp32")
))]
compile_error!("`board-esp32-wroom` and `board-feather-v2` need the `esp32` chip feature");

#[doc(hidden)]
pub use esp_hal as __esp_hal;

/// Short board name for log lines.
#[cfg(feature = "board-c3-supermini")]
pub const BOARD_NAME: &str = "c3-supermini";
/// Short board name for log lines.
#[cfg(feature = "board-c6-devkitc")]
pub const BOARD_NAME: &str = "c6-devkitc";
/// Short board name for log lines.
#[cfg(feature = "board-esp32-wroom")]
pub const BOARD_NAME: &str = "esp32-wroom";
/// Short board name for log lines.
#[cfg(feature = "board-feather-v2")]
pub const BOARD_NAME: &str = "feather-v2";

/// The profile's I2C `(SDA, SCL)` GPIO numbers, for log lines.
#[cfg(feature = "board-c3-supermini")]
pub const I2C_PINS: (u8, u8) = (6, 7);
/// The profile's I2C `(SDA, SCL)` GPIO numbers, for log lines.
#[cfg(any(feature = "board-c6-devkitc", feature = "board-esp32-wroom"))]
pub const I2C_PINS: (u8, u8) = (21, 22);
/// The profile's I2C `(SDA, SCL)` GPIO numbers, for log lines.
#[cfg(feature = "board-feather-v2")]
pub const I2C_PINS: (u8, u8) = (22, 20);

/// Moves the profile's `(sda, scl)` pins out of `Peripherals`.
///
/// Takes the `Peripherals` variable by name; only the two GPIO fields are
/// moved, so the rest stays usable.
#[cfg(feature = "board-c3-supermini")]
#[macro_export]
macro_rules! board_i2c {
    ($p:expr) => {
        ($p.GPIO6, $p.GPIO7)
    };
}
/// Moves the profile's `(sda, scl)` pins out of `Peripherals`.
///
/// Takes the `Peripherals` variable by name; only the two GPIO fields are
/// moved, so the rest stays usable.
#[cfg(any(feature = "board-c6-devkitc", feature = "board-esp32-wroom"))]
#[macro_export]
macro_rules! board_i2c {
    ($p:expr) => {
        ($p.GPIO21, $p.GPIO22)
    };
}
/// Moves the profile's `(sda, scl)` pins out of `Peripherals`.
///
/// Takes the `Peripherals` variable by name; only the two GPIO fields are
/// moved, so the rest stays usable.
#[cfg(feature = "board-feather-v2")]
#[macro_export]
macro_rules! board_i2c {
    ($p:expr) => {
        ($p.GPIO22, $p.GPIO20)
    };
}

/// Switches the profile's I2C bus power on; a no-op (`()`) on boards without
/// a switch.
///
/// On the Feather V2 this drives GPIO 2 high (STEMMA QT / I2C power) and
/// returns the `Output` that holds it: keep the returned value alive for as
/// long as the bus is used, e.g. `let _bus_power = board_i2c_power!(p);`
/// (a bare `let _ =` would drop it at once).
#[cfg(not(feature = "board-feather-v2"))]
#[macro_export]
macro_rules! board_i2c_power {
    ($p:expr) => {
        ()
    };
}
/// Switches the profile's I2C bus power on; a no-op (`()`) on boards without
/// a switch.
///
/// On the Feather V2 this drives GPIO 2 high (STEMMA QT / I2C power) and
/// returns the `Output` that holds it: keep the returned value alive for as
/// long as the bus is used, e.g. `let _bus_power = board_i2c_power!(p);`
/// (a bare `let _ =` would drop it at once).
#[cfg(feature = "board-feather-v2")]
#[macro_export]
macro_rules! board_i2c_power {
    ($p:expr) => {
        $crate::board::__esp_hal::gpio::Output::new(
            $p.GPIO2,
            $crate::board::__esp_hal::gpio::Level::High,
            $crate::board::__esp_hal::gpio::OutputConfig::default(),
        )
    };
}
