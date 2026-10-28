// Emit chip-specific cfg flags from the Cargo target triple, so driver code can
// branch with #[cfg(esp32)] / #[cfg(esp32s3)] on the few ESP-IDF API
// differences between chips — without depending on the MCU env var or on cfg
// flags from another crate's build script (those do not propagate to
// dependents).
//
// The cargo:rustc-check-cfg lines register each key so Cargo's check-cfg lint
// does not warn about unexpected_cfgs.
//
// For ESP-IDF targets this build script also calls
// `embuild::espidf::sysenv::output()`, which re-emits esp-idf-sys's link args
// *and* its `esp_idf_*` sdkconfig cfgs for this crate — neither propagates from
// a dependency's build script on its own. That is how
// `cfg(esp_idf_freertos_unicore)` reaches `rotary` (verified: it appears in
// this crate's build-script output on the C3). If it ever stopped arriving, a
// single-core build fails to compile rather than misbehaving: the dual-core
// branch calls `esp_ipc_call_blocking`, which unicore bindings do not contain.
// See rustyfarian-esp-idf-power/build.rs.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(esp32)");
    println!("cargo:rustc-check-cfg=cfg(esp32s3)");
    // Emitted by esp-idf-sys for single-core FreeRTOS builds (C3, C6); gates
    // the dual-core-only IPC teardown barrier in `rotary`.
    println!("cargo:rustc-check-cfg=cfg(esp_idf_freertos_unicore)");

    let target = std::env::var("TARGET").unwrap_or_default();
    match target.as_str() {
        "xtensa-esp32-espidf" => println!("cargo:rustc-cfg=esp32"),
        "xtensa-esp32s3-espidf" => println!("cargo:rustc-cfg=esp32s3"),
        _ => {}
    }

    if target.ends_with("-espidf") {
        embuild::espidf::sysenv::output();
    }
}
