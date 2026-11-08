//! ESP32-C3 — Rig check for the fixed ADR-012 pin map
//!
//! Bring-up diagnostic that verifies every connector of the C3 SuperMini
//! bench rig, one part at a time, by watching for changes and reporting only
//! transitions.
//! A bench scenario (or a human with a serial monitor) attaches and removes
//! parts, presses the button, and reads the log.
//! This is not a peripheral driver; it only reuses [`AdcReader`] and
//! [`EdgeDetector`] and holds no decode logic of its own.
//!
//! ## Pins (ADR-012)
//!
//! | Part                | Pin(s)          | How it is sensed                                                    |
//! |:--------------------|:----------------|:--------------------------------------------------------------------|
//! | I2C device          | SDA 6 / SCL 7   | 100 kHz bus kept up; full `0x08..=0x77` scan every 500 ms           |
//! | 3-pin pogo (analog) | GPIO 4          | pull-and-ADC test every 250 ms (raw = mean)                        |
//! | Button              | GPIO 3          | pull-up + 20 ms debounce                                            |
//! | Passive buzzer      | GPIO 10 (NPN)   | LEDC 2 kHz, 50 %, driven only after the first button press          |
//!
//! Nothing else is touched: only GPIO 4's internal pulls are toggled, only
//! GPIO 10 is driven (the documented buzzer), GPIO 6/7 are used only through
//! the I2C driver.
//!
//! ## Log lines
//!
//! ```text
//! RIG ready (button=3 pogo=4 i2c=6/7 buzzer=10)
//! I2C attached sda=6 scl=7 addr=0x3C (OLED)      / I2C detached sda=6 scl=7 addr=0x3C
//! POGO attached GPIO4 raw=2048                   / POGO detached GPIO4
//! BUTTON GPIO3 pressed
//! Listen now: press the button when you hear a tone
//! TONE GPIO10
//! BUZZER GPIO10 confirmed
//! ```
//!
//! I2C addresses are debounced over 2 consecutive scans.
//! A press while not listening starts a round: a 300 ms tone every 2 s.
//! A press within 1.5 s of a tone start confirms the buzzer and ends the round;
//! an unconfirmed round stops by itself after 15 tones (`Listen stopped`).
//! The next press starts a new round.
//! Parts already present at boot are reported right after `RIG ready`.
//!
//! ## Pull test (pogo GPIO 4)
//!
//! The pin gets the internal pull-up (about 45 k), a 2 ms settle, then an ADC1
//! reading (median of 4); then the same with the pull-down.
//! esp-hal's analog setup clears the pulls, but a floating pad keeps the
//! charge for the few ms of the read, so an empty pogo tracks the pull (about
//! 4095 up, 0 down).
//! A part that drives the line (potentiometer wiper, 49E Hall sensor, PIR
//! output) reads its own level both times; it counts as attached when the two
//! reads differ by less than 1500, after 3 equal results.
//! The reported raw is the mean of both reads (a digital read would miss a
//! mid-rail wiper that looks like both pulls).
//! A WS2812 on the pogo has a high-impedance data input and cannot be sensed
//! this way.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c3_rig_check
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c3_rig_check
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embedded_hal::pwm::SetDutyCycle;
use esp_hal::{
    delay::Delay,
    gpio::{DriveMode, Input, InputConfig, Pull},
    i2c::master::{Config as I2cConfig, Error as I2cError, I2c},
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    main,
    time::{Instant, Rate},
};
use esp_println::{println, Printer};
use rustyfarian_esp_hal_peripherals::analog::{AdcReader, AnalogRead};
use tamer::debounce::{Edge, EdgeDetector};

/// Fixed-capacity line buffer; overlong lines are truncated, never panic.
struct Line {
    buf: [u8; 128],
    len: usize,
}

impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - 1 - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// Emits one log line with a single `Printer::write_bytes` call, so the last
/// segment cannot stay stuck in the USB-JTAG TX FIFO (project lore,
/// "esp-hal — console output").
fn log_line(args: core::fmt::Arguments<'_>) {
    use core::fmt::Write;
    let mut line = Line {
        buf: [0; 128],
        len: 0,
    };
    let _ = line.write_fmt(args);
    line.buf[line.len] = b'\n';
    Printer::write_bytes(&line.buf[..=line.len]);
}

macro_rules! logln {
    ($($arg:tt)*) => {
        log_line(format_args!($($arg)*))
    };
}

const TICK_MS: u32 = 10;
const DEBOUNCE_MS: u64 = 20;
const SCAN_PERIOD_MS: u64 = 500;
const POGO_PERIOD_MS: u64 = 250;
const TONE_PERIOD_MS: u64 = 2000;
const TONE_LEN_MS: u64 = 300;
const CONFIRM_WINDOW_MS: u64 = 1500;
const FIRST_ADDR: u8 = 0x08;
const ADDRS: usize = 0x70; // 0x08..=0x77
/// Consecutive disagreeing scans before an address flips state.
const I2C_DEBOUNCE: u8 = 2;
/// Equal consecutive pull-test results before the pogo flips state.
const POGO_DEBOUNCE: u8 = 3;
const MAX_DUTY_PCT: u8 = 50;
/// Pogo counts as attached when the pull-up and pull-down ADC reads differ by
/// less than this (an empty pin tracks the pull: about 4095 vs 0).
const POGO_TRACK_SPAN: u16 = 1500;
/// Tones per listening round before it stops by itself (~30 s).
const MAX_TONES: u8 = 15;

fn known_device(address: u8) -> &'static str {
    match address {
        0x76 | 0x77 => " (BME680)",
        0x68 | 0x69 => " (MPU6050)",
        0x3C | 0x3D => " (OLED)",
        _ => "",
    }
}

fn now_ms() -> u64 {
    Instant::now().duration_since_epoch().as_millis()
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    let mut gpio4 = peripherals.GPIO4;
    let mut adc1 = peripherals.ADC1;

    let button = Input::new(
        peripherals.GPIO3,
        InputConfig::default().with_pull(Pull::Up),
    );
    let mut button_edge = EdgeDetector::new(button.is_high(), DEBOUNCE_MS);

    let mut i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(100)),
    )
    .expect("valid I2C configuration")
    .with_sda(peripherals.GPIO6)
    .with_scl(peripherals.GPIO7);

    // Buzzer: the channel owns GPIO 10 for the whole run and sits at duty 0
    // (pin low, NPN off) until a tone is requested; the timer is configured
    // once at 2 kHz and never retuned.
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut lstimer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    let timer_ok = lstimer
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty10Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_hz(2000),
        })
        .is_ok();
    let mut buzzer = ledc.channel::<LowSpeed>(channel::Number::Channel0, peripherals.GPIO10);
    let buzzer_ok = timer_ok
        && buzzer
            .configure(channel::config::Config {
                timer: &lstimer,
                duty_pct: 0,
                drive_mode: DriveMode::PushPull,
            })
            .is_ok();
    if !buzzer_ok {
        logln!("BUZZER GPIO10 setup failed");
    }

    // I2C state: debounced presence plus the count of scans that disagree.
    let mut present = [false; ADDRS];
    let mut disagree = [0u8; ADDRS];
    let mut bus_error_reported = false;

    // Pogo state: debounced attached flag plus the current equal-result streak.
    let mut pogo_attached = false;
    let mut pogo_last = false;
    let mut pogo_last_raw = 0u16;
    let mut pogo_streak: u8;

    // One I2C scan; with `verbose` the debounced transitions are logged.
    let mut scan = |verbose: bool, present: &mut [bool; ADDRS], disagree: &mut [u8; ADDRS]| {
        let mut seen = [false; ADDRS];
        let mut error: Option<I2cError> = None;
        for (i, slot) in seen.iter_mut().enumerate() {
            match i2c.write(FIRST_ADDR + i as u8, &[]) {
                Ok(()) => *slot = true,
                Err(I2cError::AcknowledgeCheckFailed(_)) => {}
                Err(err) => {
                    error = Some(err);
                    break;
                }
            }
        }
        if let Some(err) = error {
            // Bus fault (shorted line, missing pull-ups): do not change any
            // address state, report once per fault episode.
            if !bus_error_reported {
                bus_error_reported = true;
                logln!("I2C error sda=6 scl=7 err={:?}", err);
            }
            return;
        }
        bus_error_reported = false;
        for i in 0..ADDRS {
            if seen[i] == present[i] {
                disagree[i] = 0;
                continue;
            }
            disagree[i] += 1;
            if disagree[i] >= I2C_DEBOUNCE {
                present[i] = seen[i];
                disagree[i] = 0;
                if verbose {
                    let address = FIRST_ADDR + i as u8;
                    if seen[i] {
                        logln!(
                            "I2C attached sda=6 scl=7 addr=0x{:02X}{}",
                            address,
                            known_device(address)
                        );
                    } else {
                        logln!("I2C detached sda=6 scl=7 addr=0x{:02X}", address);
                    }
                }
            }
        }
    };

    // Median of 4 ADC1 samples on GPIO 4 (no pull is applied by the ADC setup);
    // None if a conversion failed.
    let adc_median = |adc1: &mut esp_hal::peripherals::ADC1<'_>,
                      gpio4: &mut esp_hal::peripherals::GPIO4<'_>|
     -> Option<u16> {
        let mut reader = AdcReader::new(adc1.reborrow(), gpio4.reborrow());
        let mut samples = [0u16; 4];
        for s in samples.iter_mut() {
            *s = reader.read_raw().ok()?;
        }
        samples.sort_unstable();
        Some(samples[2])
    };

    // Pogo test on GPIO 4: apply the internal pull-up, wait 2 ms, hand the pad
    // to the ADC (esp-hal's analog setup on the C3 clears the pulls, but a
    // floating pad keeps the pull's charge for the few ms of the read), then
    // the same with the pull-down. An empty pogo tracks the pull (raw_up near
    // 4095, raw_down near 0); a driven part (poti wiper, 49E, PIR) reads its
    // own level both times. Returns (attached, mean raw), None on ADC failure.
    let pogo_test = |adc1: &mut esp_hal::peripherals::ADC1<'_>,
                     gpio4: &mut esp_hal::peripherals::GPIO4<'_>|
     -> Option<(bool, u16)> {
        let mut read_with = |pull: Pull| {
            let _input = Input::new(gpio4.reborrow(), InputConfig::default().with_pull(pull));
            delay.delay_millis(2u32);
            adc_median(adc1, gpio4)
        };
        let raw_up = read_with(Pull::Up)?;
        let raw_down = read_with(Pull::Down)?;
        let attached = raw_up.abs_diff(raw_down) < POGO_TRACK_SPAN;
        Some((
            attached,
            ((u32::from(raw_up) + u32::from(raw_down)) / 2) as u16,
        ))
    };

    // Boot state: two scans / three pogo tests settle the debounce silently,
    // then `RIG ready` is printed followed by what is already attached.
    for _ in 0..I2C_DEBOUNCE {
        scan(false, &mut present, &mut disagree);
        delay.delay_millis(TICK_MS);
    }
    let mut boot_valid = false;
    for _ in 0..POGO_DEBOUNCE {
        if let Some((attached, raw)) = pogo_test(&mut adc1, &mut gpio4) {
            pogo_last = attached;
            pogo_last_raw = raw;
            boot_valid = true;
        }
        delay.delay_millis(TICK_MS);
    }
    logln!("RIG ready (button=3 pogo=4 i2c=6/7 buzzer=10)");
    for (i, &p) in present.iter().enumerate() {
        if p {
            let address = FIRST_ADDR + i as u8;
            logln!(
                "I2C attached sda=6 scl=7 addr=0x{:02X}{}",
                address,
                known_device(address)
            );
        }
    }
    if boot_valid && pogo_last {
        pogo_attached = true;
        logln!("POGO attached GPIO4 raw={}", pogo_last_raw);
    }
    pogo_streak = 1;

    let start = now_ms();
    let mut next_scan = start + SCAN_PERIOD_MS;
    let mut next_pogo = start + POGO_PERIOD_MS;
    // Buzzer round state.
    let mut listening = false;
    let mut tones_played = 0u8;
    let mut next_tone = 0u64;
    let mut tone_start: Option<u64> = None;
    let mut tone_end: Option<u64> = None;

    loop {
        let now = now_ms();

        if now >= next_scan {
            next_scan = now + SCAN_PERIOD_MS;
            scan(true, &mut present, &mut disagree);
        }

        if now >= next_pogo {
            next_pogo = now + POGO_PERIOD_MS;
            // A failed ADC read skips this round (no streak change).
            if let Some((result, raw)) = pogo_test(&mut adc1, &mut gpio4) {
                if result == pogo_last {
                    pogo_streak = pogo_streak.saturating_add(1);
                } else {
                    pogo_last = result;
                    pogo_streak = 1;
                }
                if pogo_streak >= POGO_DEBOUNCE && result != pogo_attached {
                    pogo_attached = result;
                    if result {
                        logln!("POGO attached GPIO4 raw={}", raw);
                    } else {
                        logln!("POGO detached GPIO4");
                    }
                }
            }
        }

        // Fresh timestamp: the scan and pogo test above can block ~35 ms.
        let now = now_ms();
        if let Some(Edge::Falling) = button_edge.update(button.is_high(), now) {
            logln!("BUTTON GPIO3 pressed");
            if !listening {
                // A press outside a round starts a new one.
                listening = true;
                tones_played = 0;
                tone_start = None;
                next_tone = now + TONE_PERIOD_MS;
                logln!("Listen now: press the button when you hear a tone");
            } else if tone_start.is_some_and(|t| now.saturating_sub(t) <= CONFIRM_WINDOW_MS) {
                listening = false;
                tone_start = None;
                tone_end = None;
                if buzzer.set_duty_cycle(0).is_err() {
                    logln!("BUZZER GPIO10 duty error");
                }
                logln!("BUZZER GPIO10 confirmed");
            }
            // Listening but outside a confirm window: BUTTON line only.
        }

        if listening && now >= next_tone {
            next_tone = now + TONE_PERIOD_MS;
            if tones_played >= MAX_TONES || !buzzer_ok {
                listening = false;
                tone_start = None;
                logln!("Listen stopped");
            } else {
                tones_played += 1;
                tone_start = Some(now);
                tone_end = Some(now + TONE_LEN_MS);
                logln!("TONE GPIO10");
                if buzzer.set_duty_cycle_percent(MAX_DUTY_PCT).is_err() {
                    logln!("BUZZER GPIO10 duty error");
                }
            }
        }
        if let Some(end) = tone_end {
            if now >= end {
                tone_end = None;
                if buzzer.set_duty_cycle(0).is_err() {
                    logln!("BUZZER GPIO10 duty error");
                }
            }
        }

        delay.delay_millis(TICK_MS);
    }
}
