//! ESP32-S3 CrowPanel Rotary Display — Interrupt-Driven Rotary Encoder (esp-hal)
//!
//! Reads the CrowPanel 1.28" HMI ESP32-S3 Rotary Display knob with the
//! [`InterruptEncoder`]: quadrature decoding runs inside the GPIO interrupt, so
//! `position()` stays current while the main loop is blocked.
//! The loop simulates a display redraw with a 90 ms blocking busy-wait on every
//! position change (the async `Encoder` loses about half the detents at that
//! load), polls the push button with a timestamp, and prints one `WINDOW` line
//! per burst of motion plus a `HB` heartbeat every 10 s.
//!
//! The application owns the single esp-hal GPIO handler and forwards to
//! [`rotary::on_gpio_interrupt`] at `Priority3`.
//!
//! Pass criteria for a round trip of 30 detents out and 30 back: every window
//! shows `delta` equal to the detents turned and `skipped=0`.
//!
//! ## Components
//!
//! - CrowPanel 1.28" HMI ESP32-S3 Rotary Display (on-board EC11-style knob)
//!
//! ## Wiring
//!
//! The knob is wired on the board; no external parts are needed.
//! These pins are this board's wiring only: `InterruptEncoder::new` takes any
//! three input-capable GPIOs, so adapt them for another board.
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
//! just build-example hal_s3_rotary_isr
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_s3_rotary_isr
//! ```

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Io;
use esp_hal::handler;
use esp_hal::interrupt::Priority;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use rustyfarian_esp_hal_peripherals::rotary::{self, ButtonEvent, InterruptEncoder};

/// Simulated blocking redraw per position change.
const WORK_MS: u64 = 90;
/// A window closes after this much motion-free time.
const IDLE_CLOSE_MS: u64 = 1500;
/// Idle poll interval; also bounds the latency of click / long-press events.
const POLL_MS: u64 = 5;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("PANIC: {}", info);
    loop {}
}

/// The application's single GPIO handler. It clears no other pins because it
/// listens on none.
#[handler(priority = Priority::Priority3)]
fn gpio_handler() {
    rotary::on_gpio_interrupt();
}

fn busy_wait_ms(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        core::hint::spin_loop();
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(gpio_handler);

    let mut encoder =
        match InterruptEncoder::new(peripherals.GPIO45, peripherals.GPIO42, peripherals.GPIO41) {
            Ok(encoder) => encoder,
            Err(e) => {
                println!("Encoder init failed: {}", e);
                loop {
                    core::hint::spin_loop();
                }
            }
        };

    println!(
        "ISR encoder work_ms={} — ready, turn round trips (30 out, pause, 30 back).",
        WORK_MS
    );

    let mut drawn = 0i32;
    let mut window_open = false;
    let mut last_motion = Instant::now();
    // Counter snapshots taken at the start of the current window.
    let (mut w_edges, mut w_skipped, mut w_isrs, mut w_pos) = (0u32, 0u32, 0u32, 0i32);
    // Detents seen per direction, summed from per-redraw position changes;
    // a CW/CCW pair inside one redraw nets out, so these are a lower bound.
    let (mut cw, mut ccw) = (0u32, 0u32);
    let mut redraws = 0u32;
    let mut window_no = 0u32;
    let mut heartbeat = Instant::now();

    loop {
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

        let pos = encoder.position();
        if pos != drawn {
            if !window_open {
                window_open = true;
                w_pos = drawn;
                redraws = 0;
                cw = 0;
                ccw = 0;
            }
            if pos > drawn {
                cw += pos.abs_diff(drawn);
            } else {
                ccw += pos.abs_diff(drawn);
            }
            drawn = pos;
            // Simulated blocking redraw (does not yield).
            busy_wait_ms(WORK_MS);
            redraws += 1;
            last_motion = Instant::now();
            continue;
        }

        if window_open && (Instant::now() - last_motion).as_millis() >= IDLE_CLOSE_MS {
            window_open = false;
            window_no += 1;
            println!(
                "WINDOW {} isr work_ms={} cw={} ccw={} delta={} pos={} edges={} skipped={} isrs={} redraws={}",
                window_no,
                WORK_MS,
                cw,
                ccw,
                drawn - w_pos,
                drawn,
                encoder.edge_count().wrapping_sub(w_edges),
                encoder.skipped_count().wrapping_sub(w_skipped),
                encoder.isr_count().wrapping_sub(w_isrs),
                redraws
            );
            // Next window starts from this window's end, so its first detent counts.
            w_edges = encoder.edge_count();
            w_skipped = encoder.skipped_count();
            w_isrs = encoder.isr_count();
        }
        if (Instant::now() - heartbeat).as_millis() >= 10_000 {
            heartbeat = Instant::now();
            println!(
                "HB pos={} edges={} skipped={} isrs={}",
                encoder.position(),
                encoder.edge_count(),
                encoder.skipped_count(),
                encoder.isr_count()
            );
        }
        Timer::after(Duration::from_millis(POLL_MS)).await;
    }
}
