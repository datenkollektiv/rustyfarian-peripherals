// Emit chip-specific cfg flags from the Cargo target triple, so driver code can
// branch with #[cfg(esp32)] / #[cfg(esp32s3)] on the few ESP-IDF API
// differences between chips — without depending on the MCU env var or on cfg
// flags from another crate's build script (those do not propagate to
// dependents).
//
// The cargo:rustc-check-cfg lines register each key so Cargo's check-cfg lint
// does not warn about unexpected_cfgs.
//
// This crate is a skeleton: there is no esp-idf-hal dependency yet. When the
// first driver lands and pulls in esp-idf-hal, this build script must ALSO call
//
//     if target.ends_with("-espidf") { embuild::espidf::sysenv::output(); }
//
// (with `embuild` added under [build-dependencies]) so examples and tests link
// against ESP-IDF — link args from a dependency's build script do not propagate
// to dependents that build binaries. See rustyfarian-esp-idf-power/build.rs.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(esp32)");
    println!("cargo:rustc-check-cfg=cfg(esp32s3)");

    let target = std::env::var("TARGET").unwrap_or_default();
    match target.as_str() {
        "xtensa-esp32-espidf" => println!("cargo:rustc-cfg=esp32"),
        "xtensa-esp32s3-espidf" => println!("cargo:rustc-cfg=esp32s3"),
        _ => {}
    }
}
