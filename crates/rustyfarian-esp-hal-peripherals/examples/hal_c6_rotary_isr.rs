//! ESP32-C6 — Interrupt-Driven Rotary Encoder (esp-hal)
//!
//! Demonstrates the interrupt-driven rotary-encoder driver [`InterruptEncoder`].
//! Quadrature decoding (A/B pins) runs entirely inside the GPIO interrupt, so
//! `position()` stays current even while the main loop is busy; button
//! debounce/click/long-press/double-click timing remains polled.
//!
//! Rotate the knob and watch the position increment (CW) / decrement (CCW) on the serial
//! monitor together with the edge and skipped-edge counters; press the button to see
//! Press / Release / Click / DoubleClick / LongPress events.
//!
//! The application owns the single esp-hal GPIO handler and forwards to
//! [`rotary::on_gpio_interrupt`] at `Priority3`.
//!
//! ## Components
//!
//! - ESP32-C6-DevKitC-1
//! - 1 × EC11 rotary encoder with integral push button
//!
//! ## Wiring
//!
//! ```text
//! EC11 pin      ESP32-C6
//! ────────      ────────
//! A / CLK       GPIO 18
//! B / DT        GPIO 19
//! Button / SW   GPIO 20
//! +             3V3
//! -             GND
//! ```
//!
//! All pins use internal pull-ups and are active-low.
//!
//! ## Build
//!
//! ```sh
//! just build-example hal_c6_rotary_isr
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash hal_c6_rotary_isr
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

/// Poll interval; also bounds the latency of click / long-press events.
const POLL_MS: u64 = 1;

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

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(gpio_handler);

    let mut encoder = match InterruptEncoder::new(
        peripherals.GPIO18, // A / CLK
        peripherals.GPIO19, // B / DT
        peripherals.GPIO20, // Button / SW
    ) {
        Ok(encoder) => encoder,
        Err(e) => {
            println!("Encoder init failed: {}", e);
            loop {
                core::hint::spin_loop();
            }
        }
    };
    println!("encoder ready (A=GPIO18 B=GPIO19 BTN=GPIO20)");

    let mut last_pos = encoder.position();

    loop {
        let now_ms = Instant::now().as_millis();
        if let Some(event) = encoder.update(now_ms) {
            match event {
                ButtonEvent::Press => println!("button: Press"),
                ButtonEvent::Release => println!("button: Release"),
                ButtonEvent::Click => println!("button: Click"),
                ButtonEvent::DoubleClick => println!("button: DoubleClick"),
                ButtonEvent::LongPress => println!("button: LongPress"),
            }
        }

        let pos = encoder.position();
        if pos != last_pos {
            let dir = if pos > last_pos { "CW " } else { "CCW" };
            println!(
                "rotated {}  position: {} -> {}  edges={} skipped={}",
                dir,
                last_pos,
                pos,
                encoder.edge_count(),
                encoder.skipped_count()
            );
            last_pos = pos;
        }

        Timer::after(Duration::from_millis(POLL_MS)).await;
    }
}
