//! ESP32-C3 — HX711 Load-Cell Amplifier
//!
//! Reads an HX711 24-bit load-cell amplifier and prints the tared reading once a second.
//! The scale is uncalibrated, so the value is net counts, not grams.
//!
//! `tamer` owns everything that can be host-tested: the bit-banged read
//! ([`Hx711::read_blocking`]), the sign extension, and the tare / averaging
//! pipeline ([`LoadCellScale`]).
//! This example owns only the hardware: the two pins, the delay providers, the
//! interrupt mask and the console output.
//!
//! The pulse train runs inside [`MaskedSection`](rustyfarian_esp_hal_peripherals::hx711::MaskedSection), which masks interrupts so
//! an interrupt cannot hold PD_SCK high past the 60 µs power-down limit and power the chip down mid-read.
//! The masked pulse train takes about 100 to 135 µs, an estimate until the rig run measures it.
//! The driver waits with esp-hal's busy-wait `Delay`, which is quantised to whole microseconds,
//! so it clocks with `PULSE_US` (2 µs) phases that are each at least 1 µs.
//! Between readings the example sleeps with `delay_millis`, every 100 ms, the HX711's 10 Hz output rate.
//! Every sample feeds the ten-sample window of the scale, one second at 10 Hz, so it settles within a second,
//! and one line is printed per ten samples.
//! The first sample after power-up is discarded and the tare is the average of the next ten,
//! so keep the load cell unloaded at start.
//! A reading pinned at the converter's rail prints a one-time hint to check the bridge wiring.
//!
//! Values are printed as integer counts, so no floating-point formatting is linked in.
//! Bring-up failures print a hint and park the board; read failures in the loop are
//! printed once per run via `OnceLatch`.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 1 × HX711 breakout module with a load cell attached
//! - jumper wires
//!
//! ## Wiring
//!
//! ```text
//! HX711      ESP32-C3
//! -----      --------
//! GND        GND
//! VCC        3V3
//! DT         GPIO 6
//! SCK        GPIO 7
//! ```
//!
//! The module runs from 3V3: a 5 V supply would put 5 V on DOUT, which the ESP32-C3 does not tolerate.
//! The lower excitation voltage at 3V3 gives a proportionally smaller signal, so counts per gram are lower than at 5 V.
//! DT is read as a floating input and SCK is a push-pull output held low, which keeps the chip powered.
//! GPIO 6 and GPIO 7 are the shared pogo header pins, as for the I2C sensors.
//!
//! ## Expected output
//!
//! ```text
//! HX711 ready: DT=GPIO6 SCK=GPIO7 gain A128, tare -8130
//! HX711 raw=-8127 net=3 counts
//! HX711 raw=-8131 net=-1 counts
//! ```
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_hx711
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_hx711
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    main,
};
use esp_println::println;
use rustyfarian_esp_hal_peripherals::hx711::{
    is_saturated, Calibration, Gain, LoadCellScale, MaskedSection, SETTLE_MS_10HZ,
};
use tamer::hx711::{Error, Hx711};
use tamer::latch::OnceLatch;

/// Attempts at 100 µs apart per read: 200 ms, twice the HX711's 100 ms conversion period.
const MAX_POLLS: u32 = 2_000;
/// Pause between samples in milliseconds: the HX711's 10 Hz output rate.
const SAMPLE_MS: u32 = 100;
/// One line per this many samples, so the console shows one line per second.
const PRINT_EVERY: u32 = 10;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

/// Idle forever so a failed bring-up stays visible instead of reboot-looping.
fn park(delay: &Delay) -> ! {
    loop {
        delay.delay_millis(1000u32);
    }
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    // Busy-wait delay; the driver clocks with `PULSE_US` (2 µs) because esp-hal's `Delay` is
    // quantised to whole microseconds, so each phase is at least 1 µs.
    let mut delay = Delay::new();
    let mask = MaskedSection::new();

    let dout = Input::new(peripherals.GPIO6, InputConfig::default());
    let sck = Output::new(peripherals.GPIO7, Level::Low, OutputConfig::default());
    let mut adc = Hx711::new(dout, sck);

    if let Err(e) = adc.power_up() {
        println!("HX711 pin fault: {}", e);
        park(&delay);
    }
    delay.delay_millis(SETTLE_MS_10HZ);

    // The first sample after power-up is discarded.
    if let Err(e) = adc.read_blocking(&mut delay, &mask, Gain::A128, MAX_POLLS) {
        match e {
            Error::NotReady => println!(
                "no HX711 answered: DOUT stays high (check VCC/GND, DT=GPIO6, SCK=GPIO7, 3V3 supply)"
            ),
            _ => println!("HX711 pin fault: {}", e),
        }
        park(&delay);
    }

    let mut scale = LoadCellScale::<10>::new(Calibration::identity());
    // Weigh a full window of samples, then tare on their average.
    while !scale.window_is_full() {
        match adc.read_blocking(&mut delay, &mask, Gain::A128, MAX_POLLS) {
            Ok(raw) => {
                let _ = scale.weigh(raw, None);
            }
            Err(e) => {
                match e {
                    Error::NotReady => println!(
                        "no HX711 answered: DOUT stays high (check VCC/GND, DT=GPIO6, SCK=GPIO7, 3V3 supply)"
                    ),
                    _ => println!("HX711 pin fault: {}", e),
                }
                park(&delay);
            }
        }
        if !scale.window_is_full() {
            delay.delay_millis(SAMPLE_MS);
        }
    }
    // The window is full, so the average exists; park rather than unwrap if it does not.
    let Some(avg) = scale.averaged_raw() else {
        println!("HX711 pin fault: no samples");
        park(&delay);
    };
    scale.tare(avg);
    println!("HX711 ready: DT=GPIO6 SCK=GPIO7 gain A128, tare {}", avg);

    let mut read_failed = OnceLatch::new();
    let mut saturated = OnceLatch::new();
    let mut samples: u32 = 0;
    loop {
        match adc.read_blocking(&mut delay, &mask, Gain::A128, MAX_POLLS) {
            Ok(raw) => {
                if read_failed.rearm() {
                    println!("reading recovered");
                }
                if is_saturated(raw) {
                    if saturated.trip() {
                        println!("HX711 saturated: check the bridge wiring");
                    }
                } else {
                    let _ = saturated.rearm();
                }
                // Every sample feeds the window; no float formatting is linked in.
                let net = scale.weigh(raw, None) as i32;
                samples += 1;
                if samples.is_multiple_of(PRINT_EVERY) {
                    println!("HX711 raw={} net={} counts", raw, net);
                }
            }
            Err(e) => {
                if read_failed.trip() {
                    println!("reading failed: {}", e);
                }
            }
        }
        // Sleep, never busy-wait, between reads.
        delay.delay_millis(SAMPLE_MS);
    }
}
