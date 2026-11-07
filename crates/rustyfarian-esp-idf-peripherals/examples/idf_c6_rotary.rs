//! ESP32-C6 — Interrupt-Driven Rotary Encoder (ESP-IDF / std)
//!
//! Demonstrates the full interrupt-driven rotary-encoder driver [`Encoder`].
//! Quadrature decoding (A/B pins) runs entirely inside GPIO interrupts via persistent
//! AnyEdge handlers; button debounce/click/long-press/double-click timing remains polled.
//!
//! Rotate the knob and watch the position increment (CW) / decrement (CCW) on the serial
//! monitor; press the button to see Click / DoubleClick / LongPress events.
//!
//! ## Components
//!
//! - ESP32-C6 development board (e.g. ESP32-C6-DevKitC-1)
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
//! All pins use internal pull-ups.
//! GPIO 18/19/20 are adjacent on J3 and clear of strapping, USB and UART pins.
//! The C6 is single-core.
//! The A/B quadrature channels are monitored via persistent AnyEdge GPIO interrupts;
//! the button is polled for debounce/timing.
//!
//! ## Build
//!
//! Requires espup + device target configuration.
//!
//! ```sh
//! just build-example idf_c6_rotary
//! ```
//!
//! ## Flash
//!
//! ```sh
//! just flash idf_c6_rotary
//! ```

#![deny(clippy::unwrap_used)]

use anyhow::Context;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::log::EspLogger;
use log::info;
use rustyfarian_esp_idf_peripherals::rotary::{ButtonEvent, Encoder};

/// Uptime in milliseconds from the ESP-IDF high-resolution timer.
fn now_millis() -> u64 {
    // SAFETY: FFI read of the esp_timer subsystem, which the ESP-IDF runtime
    // initializes before `main` and keeps alive for the program's lifetime.
    let micros = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
    (micros / 1000).max(0) as u64
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    info!("rotary encoder example — rotate the knob and press the button");

    let peripherals = Peripherals::take()?;

    let mut encoder = Encoder::new(
        peripherals.pins.gpio18, // A / CLK
        peripherals.pins.gpio19, // B / DT
        peripherals.pins.gpio20, // Button / SW
    )
    .context("Failed to initialize encoder")?;
    info!("encoder ready (A=GPIO18 B=GPIO19 BTN=GPIO20)");

    let mut last_pos = encoder.position();

    loop {
        let now = now_millis();
        if let Some(event) = encoder.update(now) {
            match event {
                ButtonEvent::Press => info!("button: Press"),
                ButtonEvent::Release => info!("button: Release"),
                ButtonEvent::Click => info!("button: Click"),
                ButtonEvent::DoubleClick => info!("button: DoubleClick"),
                ButtonEvent::LongPress => info!("button: LongPress"),
            }
        }

        let pos = encoder.position();
        if pos != last_pos {
            let dir = if pos > last_pos { "CW" } else { "CCW" };
            info!(
                "rotated {dir} position {pos} (edges {}, skipped {})",
                encoder.edge_count(),
                encoder.skipped_count()
            );
            last_pos = pos;
        }

        // Poll at 1 ms — use FreeRtos::delay_ms so we yield to the scheduler.
        // Busy-wait style sleeps can starve IDLE and trigger the watchdog.
        esp_idf_hal::delay::FreeRtos::delay_ms(1);
    }
}
