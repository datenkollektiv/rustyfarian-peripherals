//! ESP32-S3 CrowPanel Rotary Display — Async Rotary Encoder (esp-hal + Embassy)
//!
//! Reads the CrowPanel 1.28" HMI ESP32-S3 Rotary Display knob with the async
//! [`Encoder`] driver: quadrature rotation is awaited through
//! [`Encoder::wait_for_step`], and the push button is polled with a timestamp
//! through [`Encoder::update`]. Rotation direction, the running position and
//! button gestures (press, release, click, double click, long press) are
//! printed via `esp-println` using its auto console transport.
//! On C3/C6/S3 boards this follows the active USB Serial/JTAG monitor when
//! present, and otherwise falls back to UART.
//!
//! All decoding lives in the pure [`tamer`] state machines; the driver only
//! reads GPIO and feeds them. The runtime is `esp-rtos` with
//! `embassy-executor`; the main loop selects between the next detent and a
//! 5 ms [`embassy_time::Ticker`] so the button is polled even while the knob
//! is idle.
//!
//! ## Components
//!
//! - CrowPanel 1.28" HMI ESP32-S3 Rotary Display (on-board EC11-style knob)
//!
//! ## Wiring
//!
//! The knob is wired on the board; no external parts are needed.
//! These pins are this board's wiring only: `Encoder::new` takes any three
//! input-capable GPIOs, so adapt them for another board.
//!
//! ```text
//! Knob signal   CrowPanel ESP32-S3
//! ───────────   ──────────────────
//! A             GPIO 45
//! B             GPIO 42
//! Push button   GPIO 41
//! ```
//!
//! All three pins use the internal pull-up and are active-low.
//! GPIO 45 is a strapping pin (VDD_SPI voltage); it is fine as an input after
//! boot as long as the knob does not hold it low during reset.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_s3_rotary
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_s3_rotary
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Ticker};
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use rustyfarian_esp_hal_peripherals::rotary::{ButtonEvent, Encoder, EncoderDirection};

/// Button poll interval; also bounds the latency of click / long-press events.
const POLL_MS: u64 = 5;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let mut encoder = match Encoder::new(peripherals.GPIO45, peripherals.GPIO42, peripherals.GPIO41)
    {
        Ok(encoder) => encoder,
        Err(e) => {
            println!("Encoder init failed: {}", e);
            loop {
                core::hint::spin_loop();
            }
        }
    };

    println!("Rotary encoder ready (A=GPIO45 B=GPIO42 button=GPIO41) — turn and press.");

    let mut ticker = Ticker::every(Duration::from_millis(POLL_MS));

    loop {
        // Keep the work between two `wait_for_step` calls short: edges that land
        // entirely inside a long blocking print can cost a detent on fast spins.
        if let Either::First(direction) = select(encoder.wait_for_step(), ticker.next()).await {
            let label = match direction {
                EncoderDirection::Clockwise => "CW",
                EncoderDirection::CounterClockwise => "CCW",
            };
            println!(
                "rotated {} position {} (edges {}, skipped {})",
                label,
                encoder.position(),
                encoder.edge_count(),
                encoder.skipped_count()
            );
        }

        let now_ms = Instant::now().as_millis();
        if let Some(event) = encoder.update(now_ms) {
            match event {
                ButtonEvent::Press => println!("  t={} ms  button pressed", now_ms),
                ButtonEvent::Release => println!("  t={} ms  button released", now_ms),
                ButtonEvent::Click => println!("  t={} ms  click", now_ms),
                ButtonEvent::DoubleClick => println!("  t={} ms  double click", now_ms),
                ButtonEvent::LongPress => println!("  t={} ms  long press", now_ms),
            }
        }
    }
}
