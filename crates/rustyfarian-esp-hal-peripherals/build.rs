// Emit chip-specific cfg flags from the Cargo target triple, so driver code can
// branch with #[cfg(esp32)] / #[cfg(esp32s3)] on the handful of bare-metal GPIO
// and RTC differences between chips — without depending on the MCU env var or
// on cfg flags from another crate's build script (those do not propagate to
// dependents).
//
// The cargo:rustc-check-cfg lines register each key, so Cargo's check-cfg lint
// does not warn about unexpected_cfgs.
//
// This crate is a skeleton: there is no esp-hal dependency and no linker glue
// yet. When the first driver lands and pulls in esp-hal, no change is needed
// here — the cfg seam is already in place.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(esp32)");
    println!("cargo:rustc-check-cfg=cfg(esp32s3)");

    let target = std::env::var("TARGET").unwrap_or_default();
    match target.as_str() {
        "xtensa-esp32-none-elf" => println!("cargo:rustc-cfg=esp32"),
        "xtensa-esp32s3-none-elf" => println!("cargo:rustc-cfg=esp32s3"),
        _ => {}
    }
}
