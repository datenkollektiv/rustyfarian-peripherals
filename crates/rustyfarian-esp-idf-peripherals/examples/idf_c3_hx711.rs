//! ESP32-C3 — HX711 Load-Cell Amplifier (ESP-IDF / std)
//!
//! Reads an HX711 24-bit load-cell amplifier and logs the tared reading once a second.
//! The scale is uncalibrated, so the value is net counts, not grams.
//!
//! `tamer` owns everything that can be host-tested: the bit-banged read
//! ([`Hx711::read_blocking`]), the sign extension, and the tare / averaging
//! pipeline ([`LoadCellScale`]).
//! This example owns only the hardware: the two pins, the delay providers, the
//! interrupt mask and the log output.
//!
//! The pulse train runs inside [`MaskedSection`](rustyfarian_esp_idf_peripherals::hx711::MaskedSection), which masks interrupts so
//! an interrupt cannot hold PD_SCK high past the 60 µs power-down limit and power the chip down mid-read.
//! The masked pulse train takes about 100 to 135 µs, with clock phases of `PULSE_US` (2 µs).
//! The driver waits with [`Ets`](esp_idf_hal::delay::Ets), a busy-wait that waits the full time, for its microsecond timing.
//! Between readings the example sleeps with [`FreeRtos`](esp_idf_hal::delay::FreeRtos), every 100 ms, the HX711's 10 Hz output rate.
//! The FreeRTOS sleep between reads lets the idle task feed the task watchdog.
//! Every sample feeds the ten-sample window of the scale, one second at 10 Hz, so it settles within a second,
//! and one line is logged per ten samples.
//! The first sample after power-up is discarded and the tare is the average of the next ten,
//! so keep the load cell unloaded at start.
//! A reading pinned at the converter's rail prints a one-time hint to check the bridge wiring.
//!
//! Values are logged as integer counts, so no floating-point formatting is linked in.
//! Bring-up failures print a hint and park the board; read failures in the loop are
//! logged once per run via `OnceLatch`.
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
//! just build-example idf_c3_hx711
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_hx711
//! ```

use esp_idf_hal::{
    delay::{Ets, FreeRtos},
    gpio::{PinDriver, Pull},
    peripherals::Peripherals,
};
use rustyfarian_esp_idf_peripherals::hx711::{
    is_saturated, Calibration, Gain, LoadCellScale, MaskedSection, SETTLE_MS_10HZ,
};
use tamer::hx711::{Error, Hx711};
use tamer::latch::OnceLatch;

/// Attempts at 100 µs apart per read: 200 ms, twice the HX711's 100 ms conversion period.
const MAX_POLLS: u32 = 2_000;
/// Pause between samples in milliseconds: the HX711's 10 Hz output rate.
const SAMPLE_MS: u32 = 100;
/// One line per this many samples, so the log shows one line per second.
const PRINT_EVERY: u32 = 10;

/// Parks the task forever so the watchdog-friendly idle loop keeps the log readable.
fn park() -> ! {
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let Some(peripherals) = Peripherals::take().ok() else {
        log::warn!("peripherals already taken");
        park();
    };
    let setup = (|| -> Result<_, esp_idf_hal::sys::EspError> {
        let dout = PinDriver::input(peripherals.pins.gpio6, Pull::Floating)?;
        let mut sck = PinDriver::output(peripherals.pins.gpio7)?;
        sck.set_low()?;
        Ok((dout, sck))
    })();
    let (dout, sck) = match setup {
        Ok(pins) => pins,
        Err(e) => {
            log::warn!("HX711 pin setup failed: {:?}", e);
            park();
        }
    };
    // Busy-wait delay (not `FreeRtos`): `Ets` waits the full time, so the `PULSE_US` (2 µs)
    // clock phases are never short.
    let mut delay = Ets;
    let mask = MaskedSection::new();
    let mut adc = Hx711::new(dout, sck);

    if let Err(e) = adc.power_up() {
        log::warn!("HX711 pin fault: {}", e);
        park();
    }
    FreeRtos::delay_ms(SETTLE_MS_10HZ);

    // The first sample after power-up is discarded.
    if let Err(e) = adc.read_blocking(&mut delay, &mask, Gain::A128, MAX_POLLS) {
        match e {
            Error::NotReady => log::warn!(
                "no HX711 answered: DOUT stays high (check VCC/GND, DT=GPIO6, SCK=GPIO7, 3V3 supply)"
            ),
            _ => log::warn!("HX711 pin fault: {}", e),
        }
        park();
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
                    Error::NotReady => log::warn!(
                        "no HX711 answered: DOUT stays high (check VCC/GND, DT=GPIO6, SCK=GPIO7, 3V3 supply)"
                    ),
                    _ => log::warn!("HX711 pin fault: {}", e),
                }
                park();
            }
        }
        if !scale.window_is_full() {
            FreeRtos::delay_ms(SAMPLE_MS);
        }
    }
    // The window is full, so the average exists; park rather than unwrap if it does not.
    let Some(avg) = scale.averaged_raw() else {
        log::warn!("HX711 pin fault: no samples");
        park();
    };
    scale.tare(avg);
    log::info!("HX711 ready: DT=GPIO6 SCK=GPIO7 gain A128, tare {}", avg);

    let mut read_failed = OnceLatch::new();
    let mut saturated = OnceLatch::new();
    let mut samples: u32 = 0;
    loop {
        match adc.read_blocking(&mut delay, &mask, Gain::A128, MAX_POLLS) {
            Ok(raw) => {
                if read_failed.rearm() {
                    log::info!("reading recovered");
                }
                if is_saturated(raw) {
                    if saturated.trip() {
                        log::info!("HX711 saturated: check the bridge wiring");
                    }
                } else {
                    let _ = saturated.rearm();
                }
                // Every sample feeds the window; no float formatting is linked in.
                let net = scale.weigh(raw, None) as i32;
                samples += 1;
                if samples.is_multiple_of(PRINT_EVERY) {
                    log::info!("HX711 raw={} net={} counts", raw, net);
                }
            }
            Err(e) => {
                if read_failed.trip() {
                    log::warn!("reading failed: {}", e);
                }
            }
        }
        // FreeRTOS sleep, never a busy-wait: the idle task feeds the task watchdog.
        FreeRtos::delay_ms(SAMPLE_MS);
    }
}
