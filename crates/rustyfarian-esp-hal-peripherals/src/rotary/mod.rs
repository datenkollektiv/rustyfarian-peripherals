//! Async rotary-encoder driver — [`Encoder`].
//!
//! Contract: rotation is awaited asynchronously, button gestures are polled
//! synchronously, and all decoding lives in [`tamer`].
//!
//! Wraps two quadrature GPIO pins (A/B) and a push-button pin, and delegates
//! all decode/timing logic to the pure [`tamer`] state machines:
//! [`tamer::rotary::QuadratureDecoder`] for the A/B quadrature signal and
//! [`tamer::button::ButtonDecoder`] for the button's debounce/click/
//! long-press/double-click gestures. This module only reads GPIO and feeds
//! those state machines — no decoding happens here.
//!
//! This is the bare-metal (esp-hal) twin of the `Encoder` in
//! `rustyfarian-esp-idf-peripherals`. The IDF tier decodes inside a GPIO ISR;
//! this tier decodes in task context, woken by esp-hal's async GPIO edge
//! futures. The sync-vs-async question and the resulting API shape are
//! recorded in ADR-006 (`docs/adr/006-interrupt-encoder-instance-and-api-shape.md`).
//!
//! # How it works: read, arm, re-read
//!
//! [`Encoder::wait_for_step`] loops over three phases:
//!
//! 1. **Read** both A and B. If the pair differs from the last-seen pair, feed
//!    it to the quadrature decoder and return when a detent completes.
//! 2. **Arm** `wait_for_any_edge` on both pins.
//! 3. **Re-read** the levels once both futures have been polled (and are thus
//!    armed). If they changed between step 1 and the arming, finish the wait
//!    immediately and go back to step 1.
//!
//! Step 3 closes a read-to-arm race. esp-hal's edge futures are one-shot: the
//! GPIO handler un-listens the pin when it fires, and arming a new wait clears
//! any pending edge. Contact bounce lands exactly in that window, so an edge
//! between "read" and "armed" would otherwise be lost and the decoder would
//! silently skip a transition. The re-read uses read-only
//! [`InputSignal`] handles, because
//! the wait futures hold `&mut` borrows on the pins.
//!
//! # Cancellation
//!
//! [`Encoder::wait_for_step`] is cancel-safe at the state level: all decoder
//! state, the last-seen levels and the edge counter live in the [`Encoder`],
//! never in the future. Dropping the future mid-await (for example because the
//! other arm of a `select` won) loses no transitions — the next call re-reads
//! the pins and feeds the decoder any change it missed. Edges that arrive
//! while no future is armed are not individually counted, but the level
//! difference is picked up by the next read.
//!
//! # Blocking-work budget
//!
//! Decoding runs only while `wait_for_step` is being polled.
//! An edge pair that lands entirely in the gap between two calls skips a Gray
//! state, which the decoder drops as bounce, and can cost a detent.
//! Keep the work between two `wait_for_step` calls under **~10 ms per step**
//! (a blocking `println!` or SPI transfer included).
//! Measured on an ESP32-S3 with a 30-detent encoder: lossless up to ~13 ms of
//! work per step; a ~90 ms blocking display redraw lost about 50–63 % of
//! detents. The ~10 ms budget keeps a margin below the measured limit.
//! Moving rotation into its own task on the same cooperative executor does
//! not help — blocking work stalls that task too.
//! Firmware with heavier blocking work should use [`InterruptEncoder`], which
//! decodes in the GPIO interrupt and has no such budget.
//! [`Encoder::skipped_count`] counts samples that skipped a Gray state, so a
//! consumer can detect that it is likely over budget.
//!
//! # Recommended loop
//!
//! Rotation is awaited; the button needs a timestamped poll. Combine them with
//! `select` against a periodic `Ticker`. Use a `Ticker`, not a fresh `Timer`
//! per iteration: a fast-spinning knob wakes the loop on every detent, and a
//! freshly created `Timer` would restart each time and starve the button
//! poll.
//!
//! ```ignore
//! use embassy_futures::select::{select, Either};
//! use embassy_time::{Duration, Instant, Ticker};
//!
//! let mut encoder = Encoder::new(p.GPIO45, p.GPIO42, p.GPIO41)?;
//! let mut ticker = Ticker::every(Duration::from_millis(5));
//! loop {
//!     if let Either::First(direction) =
//!         select(encoder.wait_for_step(), ticker.next()).await
//!     {
//!         // rotated: encoder.position() has already been updated
//!         let _ = direction;
//!     }
//!     if let Some(event) = encoder.update(Instant::now().as_millis()) {
//!         // button gesture
//!         let _ = event;
//!     }
//! }
//! ```
//!
//! # Synchronous users
//!
//! Firmware without an async executor should use [`InterruptEncoder`], which
//! needs no executor and decodes in the GPIO interrupt.
//! Firmware that cannot install a GPIO interrupt handler can fall back to the
//! polled adapters in `tamer` directly: `tamer::rotary::QuadratureInput`
//! (behind `tamer`'s `hal` feature) for A/B and
//! [`tamer::button::ButtonDecoder`] for the button, both fed from esp-hal
//! `Input` pins. A slow poll cadence can miss quadrature edges, which is the
//! reason the async and interrupt drivers exist.
//!
//! # Runtime
//!
//! The driver itself has no executor or timer dependency and uses only
//! `core::future`. It needs the esp-hal GPIO interrupt handler, which
//! `esp_hal::init` binds by default. The caller supplies the executor
//! (for example `esp-rtos` with `embassy-executor`).

use core::fmt;
use core::future::{poll_fn, Future};
use core::pin::pin;
use core::task::Poll;

use esp_hal::gpio::interconnect::InputSignal;
use esp_hal::gpio::{Input, InputConfig, InputPin, Pull};

pub mod interrupt;

pub use interrupt::{on_gpio_interrupt, InterruptEncoder};

use tamer::button::ButtonDecoder;
pub use tamer::button::ButtonEvent;
// This module shadows the crate-root glob re-export of `tamer::rotary`; the
// glob keeps `rustyfarian_esp_hal_peripherals::rotary::QuadratureDecoder` and
// friends reachable, as they were in 0.1.0.
pub use tamer::rotary::*;

/// Errors returned by [`Encoder::new`] / [`Encoder::new_with_config`] and the
/// [`InterruptEncoder`] constructors.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderError {
    /// `steps_per_detent` was `0`. [`tamer::rotary::QuadratureDecoder::new`]
    /// panics on this value, so it is rejected here before construction.
    InvalidConfig,
    /// Another [`InterruptEncoder`] is already alive.
    ///
    /// The interrupt-driven encoder keeps its decoder in a single static
    /// slot, so only one instance may exist at a time.
    AlreadyInUse,
}

impl fmt::Display for EncoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => write!(
                f,
                "invalid encoder configuration: steps_per_detent must be greater than zero"
            ),
            Self::AlreadyInUse => write!(f, "an interrupt encoder is already in use"),
        }
    }
}

impl core::error::Error for EncoderError {}

/// Encoder configuration.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct EncoderConfig {
    /// Valid quadrature transitions per physical detent.
    ///
    /// `4` for full-step EC11 encoders (most common, one count per click).
    /// `2` for half-step encoders (two counts per click — feels sluggish on
    /// a full-step encoder, so only use if the encoder has detents every
    /// 2 electrical transitions). Must be non-zero; `0` is rejected with
    /// [`EncoderError::InvalidConfig`].
    pub steps_per_detent: u8,
    /// Button debounce window in milliseconds.
    pub debounce_ms: u64,
    /// Minimum button hold duration in milliseconds to fire a long-press event.
    pub long_press_ms: u64,
    /// Maximum gap in milliseconds between two clicks to detect a double click.
    pub double_click_ms: u64,
}

impl Default for EncoderConfig {
    /// EC11 full-step defaults: `steps_per_detent = 4`, `debounce_ms = 50`,
    /// `long_press_ms = 1000`, `double_click_ms = 300`.
    fn default() -> Self {
        Self {
            steps_per_detent: 4,
            debounce_ms: 50,
            long_press_ms: 1000,
            double_click_ms: 300,
        }
    }
}

/// Async rotary encoder with a button.
///
/// Quadrature decoding runs in task context inside
/// [`wait_for_step`](Encoder::wait_for_step); [`Encoder::update`] only polls
/// the button for debounce/click/long-press/double-click timing. See the
/// [module docs](self) for the read, arm, re-read scheme and the recommended
/// loop.
///
/// This type intentionally does **not** implement
/// [`RotaryEncoder`].
/// Its `position()` only advances while `wait_for_step` is being polled, so
/// behind the trait it would look like a drop-in for an interrupt-decoded
/// encoder but silently lose detents under load.
/// Use [`InterruptEncoder`] for tier-agnostic code.
pub struct Encoder<'d> {
    pin_a: Input<'d>,
    pin_b: Input<'d>,
    button: Input<'d>,
    /// Read-only level handles for A/B, usable while the edge futures hold
    /// `&mut` borrows on `pin_a` / `pin_b`.
    level_a: InputSignal<'d>,
    level_b: InputSignal<'d>,
    decoder: QuadratureDecoder,
    button_sm: ButtonDecoder,
    /// Last (A, B) pair fed to the decoder.
    last: (bool, bool),
    edge_count: u32,
}

impl<'d> Encoder<'d> {
    /// Creates a new encoder with the default [`EncoderConfig`] (EC11
    /// full-step).
    ///
    /// # Errors
    ///
    /// See [`new_with_config`](Encoder::new_with_config).
    pub fn new(
        pin_a: impl InputPin + 'd,
        pin_b: impl InputPin + 'd,
        button: impl InputPin + 'd,
    ) -> Result<Self, EncoderError> {
        Self::new_with_config(pin_a, pin_b, button, EncoderConfig::default())
    }

    /// Creates a new encoder with a custom [`EncoderConfig`].
    ///
    /// Configures all three pins as inputs with an internal pull-up and seeds
    /// the [`QuadratureDecoder`] and [`ButtonDecoder`] from a *live* read of
    /// each pin (so a button already held at construction does not register
    /// a phantom press). No interrupt is armed until the first
    /// [`wait_for_step`](Encoder::wait_for_step) poll.
    ///
    /// # Errors
    ///
    /// Returns [`EncoderError::InvalidConfig`] if
    /// `config.steps_per_detent == 0`.
    ///
    /// # Panics
    ///
    /// Never panics: `steps_per_detent == 0` is validated and rejected
    /// before [`QuadratureDecoder::new`] (which would otherwise panic on
    /// that input) is called.
    pub fn new_with_config(
        pin_a: impl InputPin + 'd,
        pin_b: impl InputPin + 'd,
        button: impl InputPin + 'd,
        config: EncoderConfig,
    ) -> Result<Self, EncoderError> {
        if config.steps_per_detent == 0 {
            return Err(EncoderError::InvalidConfig);
        }

        let input_config = InputConfig::default().with_pull(Pull::Up);
        let pin_a = Input::new(pin_a, input_config);
        let pin_b = Input::new(pin_b, input_config);
        let button = Input::new(button, input_config);

        let level_a = pin_a.peripheral_input();
        let level_b = pin_b.peripheral_input();

        let initial_a = pin_a.is_high();
        let initial_b = pin_b.is_high();
        let button_pressed = button.is_low();

        Ok(Self {
            pin_a,
            pin_b,
            button,
            level_a,
            level_b,
            decoder: QuadratureDecoder::new(initial_a, initial_b, config.steps_per_detent),
            button_sm: ButtonDecoder::new(
                button_pressed,
                config.debounce_ms,
                config.long_press_ms,
                config.double_click_ms,
            ),
            last: (initial_a, initial_b),
            edge_count: 0,
        })
    }

    /// Waits until the encoder completes one detent and returns its direction.
    ///
    /// The new position is available from [`position`](Encoder::position)
    /// when this returns. See the [module docs](self) for the algorithm and
    /// cancellation semantics: the future may be dropped at any await point
    /// without losing quadrature state.
    pub async fn wait_for_step(&mut self) -> EncoderDirection {
        // Disjoint field borrows: the edge futures hold `&mut` on the pins
        // while the level handles and decoder state stay usable.
        let Self {
            pin_a,
            pin_b,
            level_a,
            level_b,
            decoder,
            last,
            edge_count,
            ..
        } = self;

        loop {
            // 1. Read: feed any change to the decoder.
            let now = (level_a.is_input_high(), level_b.is_input_high());
            if now != *last {
                *last = now;
                *edge_count = edge_count.wrapping_add(1);
                if let Some(direction) = decoder.update(now.0, now.1) {
                    return direction;
                }
            }

            // 2. Arm both edge futures and wait for either.
            let mut wait_a = pin!(pin_a.wait_for_any_edge());
            let mut wait_b = pin!(pin_b.wait_for_any_edge());
            let mut first_poll = true;
            let seen = *last;

            poll_fn(|cx| {
                let a_ready = wait_a.as_mut().poll(cx).is_ready();
                let b_ready = wait_b.as_mut().poll(cx).is_ready();
                if a_ready || b_ready {
                    return Poll::Ready(());
                }
                // 3. Re-read: both futures are armed now. Anything that moved
                // since step 1 may have had its edge cleared by arming.
                if first_poll {
                    first_poll = false;
                    if (level_a.is_input_high(), level_b.is_input_high()) != seen {
                        return Poll::Ready(());
                    }
                }
                Poll::Pending
            })
            .await;
        }
    }

    /// Polls the button for debounce/click/long-press/double-click timing.
    ///
    /// Quadrature decoding is handled by
    /// [`wait_for_step`](Encoder::wait_for_step); this method only reads the
    /// button pin and ticks the internal [`ButtonDecoder`]. `now_ms` is a
    /// monotonic millisecond timestamp — pass the same clock consistently
    /// across calls. Call this after every wake of the main loop (the
    /// debounce/long-press/double-click windows are measured against
    /// `now_ms`, not against call frequency, but a gesture is only ever
    /// detected on a call to `update`).
    pub fn update(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        let pressed = self.button.is_low();
        self.button_sm.update(pressed, now_ms)
    }

    /// Returns the current encoder position.
    #[must_use]
    pub fn position(&self) -> i32 {
        self.decoder.position()
    }

    /// Overwrites the encoder position and clears the decoder's internal
    /// accumulator.
    pub fn set_position(&mut self, position: i32) {
        self.decoder.set_position(position);
    }

    /// Resets the encoder position to zero.
    pub fn reset(&mut self) {
        self.set_position(0);
    }

    /// Returns the current debounced (stable) button-pressed state.
    #[must_use]
    pub fn is_button_pressed(&self) -> bool {
        self.button_sm.is_pressed()
    }

    /// Number of A/B level changes fed to the decoder since construction.
    ///
    /// **Diagnostic only** — intended for bring-up (e.g. confirming edges are
    /// arriving) and not part of the semantic contract. It counts observed
    /// level changes, not detents; edges that coincide or bounce between
    /// reads are not counted individually; it wraps (`u32`), and its exact
    /// value may change between versions. Use [`position`](Encoder::position)
    /// for motion.
    #[must_use]
    pub fn edge_count(&self) -> u32 {
        self.edge_count
    }

    /// Number of samples that skipped a Gray-code state since construction.
    ///
    /// **Diagnostic only** — a growing count suggests the work between two
    /// [`wait_for_step`](Encoder::wait_for_step) calls exceeds the budget in
    /// the module docs and detents may be lost; contact bounce can also add
    /// to it. It wraps (`u32`); take deltas with `wrapping_sub`. See
    /// [`QuadratureDecoder::skipped_count`].
    #[must_use]
    pub fn skipped_count(&self) -> u32 {
        self.decoder.skipped_count()
    }

    /// Current level of the A pin (diagnostics).
    #[must_use]
    pub fn pin_a_is_high(&self) -> bool {
        self.pin_a.is_high()
    }

    /// Current level of the B pin (diagnostics).
    #[must_use]
    pub fn pin_b_is_high(&self) -> bool {
        self.pin_b.is_high()
    }
}
