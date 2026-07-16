// Emit chip-specific cfg flags from the Cargo target triple, so driver code can
// branch with #[cfg(esp32)] / #[cfg(esp32s3)] on the handful of bare-metal GPIO
// and RTC differences between chips — without depending on the MCU env var or
// on cfg flags from another crate's build script (those do not propagate to
// dependents).
//
// The cargo:rustc-check-cfg lines register each key, so Cargo's check-cfg lint
// does not warn about unexpected_cfgs.
//
// esp-hal is wired in behind the chip features and the crate ships device
// examples; the cfg seam here stays independent of it, emitting the chip flags
// straight from the target triple so driver code can branch without relying on
// esp-hal's own cfgs (which do not propagate to dependents).

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
