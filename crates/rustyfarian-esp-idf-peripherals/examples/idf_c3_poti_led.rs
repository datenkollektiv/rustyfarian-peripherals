//! ESP32-C3 — Potentiometer-dimmed LED via RangeMap (ESP-IDF / std)
//!
//! ESP-IDF (`std`) example demonstrating [`RangeMap`](tamer::range_map::RangeMap)
//! as the pure mapping step between an analog reading and a PWM duty cycle.
//! It samples a potentiometer wiper through ADC1, observes a short startup
//! calibration sweep with [`AnalogCalibration`] to find the pot's real
//! end-to-end raw range (the ESP32-C3 SAR ADC clips before 0 and 4095 at
//! 12 dB attenuation), and maps the raw reading onto an 8-bit LEDC duty
//! cycle from that calibrated range: turning the pot sweeps the LED from
//! fully dark to fully bright.
//!
//! Output goes through [`EspLogger`](esp_idf_svc::log::EspLogger), matching the
//! other ESP-IDF examples in this crate.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3 SuperMini)
//! - 1 × potentiometer, 10 kΩ is a good default
//! - 1 × LED (any color) + 1 × ~330 Ω series resistor
//!
//! ## Wiring
//!
//! ```text
//! Potentiometer     ESP32-C3
//! ─────────────     ────────
//! outer leg         3V3
//! wiper/middle      GPIO 4
//! outer leg         GND
//!
//! LED (active-high)         ESP32-C3
//! ──────────────────        ────────
//! anode -- 330 Ω resistor    GPIO 6
//! cathode                    GND
//! ```
//!
//! GPIO 4 is ADC1-capable on ESP32-C3 and is a convenient non-strapping pin on
//! common ESP32-C3 development boards.
//! Keep the potentiometer signal between GND and 3V3.
//! Avoid feeding 5 V into the ADC pin.
//!
//! GPIO 6 is not a strapping pin (2/8/9), not the on-board WS2812 (8), not
//! USB (18/19), not the UART console (20/21), and not in-package SPI flash
//! (11-17) — a safe general-purpose PWM output on common ESP32-C3 boards.
//!
//! ## Build
//!
//! ```sh
//! just build-example idf_c3_poti_led
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c3_poti_led
//! ```

use esp_idf_hal::{
    adc::{
        attenuation::DB_12,
        oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
    },
    delay::FreeRtos,
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver, Resolution},
    peripherals::Peripherals,
    units::Hertz,
};
use tamer::{
    analog::{AnalogCalibration, AnalogRange},
    range_map::RangeMap,
};

const ADC_MAX: u16 = 4095;
const LEDC_FREQUENCY_HZ: u32 = 5_000;
const CALIBRATION_SAMPLES: u16 = 200;
const CALIBRATION_DELAY_MS: u32 = 25;
const MIN_CALIBRATION_SPAN: u16 = 512;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let adc = AdcDriver::new(peripherals.adc1)?;
    let config = AdcChannelConfig {
        attenuation: DB_12,
        ..Default::default()
    };
    let mut poti_pin = AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &config)?;

    let timer_config = TimerConfig::new()
        .frequency(Hertz(LEDC_FREQUENCY_HZ))
        .resolution(Resolution::Bits8);
    let timer_driver = LedcTimerDriver::new(peripherals.ledc.timer0, &timer_config)?;
    let mut led = LedcDriver::new(
        peripherals.ledc.channel0,
        timer_driver,
        peripherals.pins.gpio6,
    )?;

    let default_range = AnalogRange::zero_to(ADC_MAX);

    log::info!("Waiting for first ADC sample on GPIO 4...");

    let initial_raw = loop {
        match adc.read_raw(&mut poti_pin) {
            Ok(raw) => break raw,
            Err(err) => {
                log::warn!("ADC read failed while initializing; retrying: {:?}", err);
                FreeRtos::delay_ms(250);
            }
        }
    };

    log::info!(
        "Calibration: turn the potentiometer end-to-end for {} seconds.",
        (u32::from(CALIBRATION_SAMPLES) * CALIBRATION_DELAY_MS) / 1000
    );

    let mut calibration = AnalogCalibration::from_sample(initial_raw);
    let mut calibration_read_failed = false;

    for _ in 0..CALIBRATION_SAMPLES {
        match adc.read_raw(&mut poti_pin) {
            Ok(raw) => {
                if calibration_read_failed {
                    log::info!("ADC read recovered during calibration");
                    calibration_read_failed = false;
                }

                calibration.observe(raw);
            }
            Err(err) => {
                if !calibration_read_failed {
                    log::warn!(
                        "ADC read failed during calibration; suppressing repeated failures: {:?}",
                        err
                    );
                    calibration_read_failed = true;
                }
            }
        }

        FreeRtos::delay_ms(CALIBRATION_DELAY_MS);
    }

    let range = if let Some(range) = calibration.range_with_min_span(MIN_CALIBRATION_SPAN) {
        log::info!("Calibration accepted; using calibrated range.");
        range
    } else {
        log::info!(
            "Calibration span below {} counts; falling back to full ADC range.",
            MIN_CALIBRATION_SPAN
        );
        default_range
    };

    log::info!(
        "Calibration raw min={:?} max={:?} span={:?}; using range {}..{}",
        calibration.min(),
        calibration.max(),
        calibration.span(),
        range.min(),
        range.max()
    );

    // Calibrated raw ADC range -> 8-bit LEDC duty. The LEDC timer above is
    // configured for 8-bit resolution, so `dimmer`'s u8 output (0..=255)
    // maps 1:1 onto the full duty range.
    let dimmer = RangeMap::new(range.min(), range.max(), 0, 255);

    let mut last_duty: Option<u8> = None;
    let mut read_failed = false;

    loop {
        match adc.read_raw(&mut poti_pin) {
            Ok(raw) => {
                if read_failed {
                    log::info!("ADC read recovered");
                    read_failed = false;
                }

                let duty = dimmer.map(raw);

                if last_duty != Some(duty) {
                    led.set_duty(u32::from(duty))?;
                    log::info!("raw={} duty={}", raw, duty);
                    last_duty = Some(duty);
                }
            }
            Err(err) => {
                if !read_failed {
                    log::warn!("ADC read failed; suppressing repeated failures: {:?}", err);
                    read_failed = true;
                }
            }
        }

        FreeRtos::delay_ms(25);
    }
}
