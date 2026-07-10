//! ESP32-C3 — Potentiometer-dimmed LED via `tamer::range_map::RangeMap`
//!
//! Reads a potentiometer on ADC1, observes a short startup calibration sweep
//! with [`AnalogCalibration`] to find the pot's real end-to-end raw range
//! (the ESP32-C3 SAR ADC clips before 0 and 4095 at 11 dB attenuation), and
//! maps the raw reading onto an 8-bit LEDC PWM duty with [`RangeMap`] built
//! from that calibrated range: turn the pot, the LED sweeps from fully dark
//! to fully bright.
//!
//! This exercises the pure [`tamer::range_map`] logic over a real esp-hal
//! ADC read and LEDC PWM output — the repo's first output/PWM example.
//!
//! ## Components
//!
//! - ESP32-C3 development board (e.g. ESP32-C3-DevKitM-1, ESP32-C3 SuperMini)
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
//! LED                ESP32-C3
//! ───                ────────
//! anode (+)          GPIO 6 (through ~330 Ω resistor)
//! cathode (-)         GND
//! ```
//!
//! GPIO 4 is ADC1-capable on ESP32-C3 and is a convenient non-strapping pin
//! on common ESP32-C3 development boards.
//! Keep the potentiometer signal between GND and 3V3.
//! Avoid feeding 5 V into the ADC pin.
//!
//! GPIO 6 drives the LED active-high through LEDC PWM (duty 0 = off, 255 =
//! full brightness) and is not a strapping pin, the on-board WS2812 (GPIO 8),
//! USB (GPIO 18/19), the UART console (GPIO 20/21), or in-package SPI flash
//! (GPIO 11-17).
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_poti_led
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_poti_led
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embedded_hal::pwm::SetDutyCycle;
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation},
    delay::Delay,
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    main,
    time::Rate,
};
use esp_println::println;
use tamer::{
    analog::{AnalogCalibration, AnalogRange},
    range_map::RangeMap,
};

const ADC_MAX: u16 = 4095;
const CALIBRATION_SAMPLES: u16 = 200;
const CALIBRATION_DELAY_MS: u32 = 25;
const MIN_CALIBRATION_SPAN: u16 = 512;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let mut adc1_config = AdcConfig::new();
    let mut poti_pin = adc1_config.enable_pin(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);
    let delay = Delay::new();

    // LEDC low-speed timer at 8-bit duty resolution, so RangeMap's u8 output
    // (0..=255) maps 1:1 onto the full PWM duty range.
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut led_timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    led_timer
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty8Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(5),
        })
        .expect("LEDC timer configuration failed");

    let mut led_channel = ledc.channel(channel::Number::Channel0, peripherals.GPIO6);
    led_channel
        .configure(channel::config::Config {
            timer: &led_timer,
            duty_pct: 0,
            drive_mode: esp_hal::gpio::DriveMode::PushPull,
        })
        .expect("LEDC channel configuration failed");

    let default_range = AnalogRange::zero_to(ADC_MAX);

    println!("Waiting for first ADC sample on GPIO 4...");

    let initial_raw = loop {
        match nb::block!(adc1.read_oneshot(&mut poti_pin)) {
            Ok(raw) => break raw,
            Err(_) => {
                println!("ADC read failed while initializing; retrying");
                delay.delay_millis(250u32);
            }
        }
    };

    println!(
        "Calibration: turn the potentiometer end-to-end for {} seconds.",
        (u32::from(CALIBRATION_SAMPLES) * CALIBRATION_DELAY_MS) / 1000
    );

    let mut calibration = AnalogCalibration::from_sample(initial_raw);
    let mut calibration_read_failed = false;

    for _ in 0..CALIBRATION_SAMPLES {
        match nb::block!(adc1.read_oneshot(&mut poti_pin)) {
            Ok(raw) => {
                if calibration_read_failed {
                    println!("ADC read recovered during calibration");
                    calibration_read_failed = false;
                }

                calibration.observe(raw);
            }
            Err(_) => {
                if !calibration_read_failed {
                    println!("ADC read failed during calibration; suppressing repeated failures");
                    calibration_read_failed = true;
                }
            }
        }

        delay.delay_millis(CALIBRATION_DELAY_MS);
    }

    let range = if let Some(range) = calibration.range_with_min_span(MIN_CALIBRATION_SPAN) {
        println!("Calibration accepted; using calibrated range.");
        range
    } else {
        println!(
            "Calibration span below {} counts; falling back to full ADC range.",
            MIN_CALIBRATION_SPAN
        );
        default_range
    };

    println!(
        "Calibration raw min={:?} max={:?} span={:?}; using range {}..{}",
        calibration.min(),
        calibration.max(),
        calibration.span(),
        range.min(),
        range.max()
    );

    let dimmer = RangeMap::new(range.min(), range.max(), 0, 255);

    let mut duty = dimmer.map(initial_raw);
    led_channel
        .set_duty_cycle(u16::from(duty))
        .expect("LEDC initial duty set failed");

    println!("LED ready on GPIO 6: raw={} duty={}", initial_raw, duty);

    let mut read_failed = false;

    loop {
        match nb::block!(adc1.read_oneshot(&mut poti_pin)) {
            Ok(raw) => {
                if read_failed {
                    println!("ADC read recovered");
                    read_failed = false;
                }

                let mapped = dimmer.map(raw);
                if mapped != duty {
                    duty = mapped;
                    led_channel
                        .set_duty_cycle(u16::from(duty))
                        .expect("LEDC duty set failed");
                    println!("raw={} duty={}", raw, duty);
                }
            }
            Err(_) => {
                if !read_failed {
                    println!("ADC read failed; suppressing repeated failures");
                    read_failed = true;
                }
            }
        }

        delay.delay_millis(25u32);
    }
}
