//! Interrupt-driven rotary encoder — [`InterruptEncoder`].
//!
//! Contract: quadrature decoding runs inside the GPIO interrupt, so
//! [`position`](InterruptEncoder::position) stays current while the main
//! context is busy; button gestures are polled synchronously.
//!
//! This mirrors the method names of the `Encoder` in
//! `rustyfarian-esp-idf-peripherals`, so switching tiers is mostly a change of
//! import and constructor.
//! Method-name parity is not full parity: this tier allows a single instance
//! (see below), the IDF `Encoder` does not.
//! Unlike the async [`Encoder`](super::Encoder), it has no task-level
//! blocking-work budget — a ~90 ms blocking display redraw loses no detents,
//! as long as the GPIO interrupt itself stays serviceable.
//! Long critical sections, higher-priority handlers and flash operations
//! still delay or coalesce edges.
//! A `skipped_count` of zero does not prove that capture was lossless.
//!
//! # Button latency
//!
//! Rotation survives a long block, the button does not: it is polled in
//! [`update`](InterruptEncoder::update), so a press and release that both fall
//! within one blocked interval are missed.
//! Keep the interval between `update` calls shorter than the shortest press
//! you need to see.
//! All decoding lives in [`tamer::rotary::QuadratureDecoder`] and
//! [`tamer::button::ButtonDecoder`]; this module only reads GPIO and feeds
//! them.
//!
//! # Installing the handler
//!
//! esp-hal allows a single user GPIO handler, and it belongs to the
//! application.
//! The application installs it and calls [`on_gpio_interrupt`] from it;
//! the hook is a cheap no-op while no [`InterruptEncoder`] exists.
//! `Priority3` is recommended: it preempts task-context work yet leaves the
//! higher levels free for time-critical peripherals.
//!
//! ```ignore
//! use esp_hal::gpio::Io;
//! use esp_hal::handler;
//! use esp_hal::interrupt::Priority;
//! use rustyfarian_esp_hal_peripherals::rotary::{self, InterruptEncoder};
//!
//! #[handler(priority = Priority::Priority3)]
//! fn gpio_handler() {
//!     rotary::on_gpio_interrupt();
//!     // ...then clear and service any other GPIO interrupt the app listens on.
//! }
//!
//! let mut io = Io::new(peripherals.IO_MUX);
//! io.set_interrupt_handler(gpio_handler);
//! let mut encoder = InterruptEncoder::new(peripherals.GPIO45, peripherals.GPIO42, peripherals.GPIO41)?;
//!
//! loop {
//!     let position = encoder.position(); // current even during long blocking work
//!     if let Some(event) = encoder.update(now_ms) {
//!         // button gesture
//!         let _ = event;
//!     }
//! }
//! ```
//!
//! # Shared-handler duty
//!
//! The hook clears only the encoder's own A/B status bits.
//! Every other GPIO interrupt the application listens on must be cleared (or
//! `unlisten`ed) by the application's own handler code, otherwise it re-fires
//! continuously.
//! The encoder pins must not be used with esp-hal's `wait_for_*` futures: the
//! esp-hal handler would race the hook for the same status bits.
//! The button pin is only polled and may be shared freely.
//!
//! # Single instance
//!
//! The decoder lives in one static slot, so only one [`InterruptEncoder`] can
//! exist.
//! This is an intentional limitation of the esp-hal tier (the IDF `Encoder`
//! supports several instances); a second [`new`](InterruptEncoder::new) fails with
//! [`EncoderError::AlreadyInUse`] until the first is dropped.
//! The handle is `!Send`: create and drop it on the core that runs the
//! handler.
//! Leaking the handle with `core::mem::forget` keeps the slot claimed, so a
//! later `new` returns [`EncoderError::AlreadyInUse`].
//!
//! # Interrupt-latency caveat
//!
//! The hook and the decoder are ordinary flash-resident code.
//! While a flash write or erase has the cache disabled, the interrupt cannot
//! run and edges coalesce, which can cost a detent (same caveat as the IDF
//! tier without IRAM placement).
//!
//! # Concurrency
//!
//! The hook feeds the decoder inside a `critical_section`; the getters and
//! setters take the same short critical section.
//! The `listen` / `unlisten` / `clear_interrupt` calls are never made inside
//! it: esp-hal takes its own GPIO lock around them, and the hook takes the two
//! locks in the opposite order.
//! Atomics are limited to load/store because the ESP32-C3 has no atomic
//! read-modify-write.
//!
//! The hook clears the encoder's status bits *before* sampling the levels; an
//! edge after the clear re-raises the interrupt by design, so it is not wiped
//! unseen.

use core::cell::RefCell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, Ordering};

use critical_section::Mutex;
use esp_hal::gpio::{Event, Input, InputConfig, InputPin, Pull};
use esp_hal::peripherals::GPIO;

use tamer::button::ButtonDecoder;
use tamer::rotary::QuadratureDecoder;

use super::{ButtonEvent, EncoderConfig, EncoderError};

// Memory ordering of `PIN_A` / `PIN_B`: every write happens inside a critical
// section (`new` publishes, `drop` zeroes), and the hook re-checks the cells
// inside its own critical section before it touches `SLOT`. That critical
// section is what keeps the cells and the slot consistent; do not move the
// re-check or the stores out of it. The hook's first, lock-free load only
// picks which status bits to clear (a stale value clears a bit that is
// re-sampled under the lock, see `on_gpio_interrupt`). `Acquire`/`Release`
// keep that early load ordered with the slot accesses around it and cost
// nothing on these cores; they do not order the MMIO pin configuration,
// which is why `listen` runs only after the cells are published.
// Load/store only — the ESP32-C3 has no atomic read-modify-write.

/// GPIO number of pin A plus one; `0` means "no encoder".
static PIN_A: AtomicU32 = AtomicU32::new(0);
/// GPIO number of pin B plus one; `0` means "no encoder".
static PIN_B: AtomicU32 = AtomicU32::new(0);

/// Decoder state shared between the handle and the hook. Holds no `Input`s.
static SLOT: Mutex<RefCell<Option<IsrSlot>>> = Mutex::new(RefCell::new(None));

struct IsrSlot {
    decoder: QuadratureDecoder,
    last: (bool, bool),
    edge_count: u32,
    isr_count: u32,
}

impl IsrSlot {
    fn new(a: bool, b: bool, steps_per_detent: u8) -> Self {
        Self {
            decoder: QuadratureDecoder::new(a, b, steps_per_detent),
            last: (a, b),
            edge_count: 0,
            isr_count: 0,
        }
    }
}

// Lifecycle interleavings (host tests cannot cover register code, so the
// reasoning is recorded here).
//
// - Every sample of the input register and its application to the slot happen
//   inside one critical section, and so do the constructor's seed and
//   reconcile. Samples therefore apply in the order they were read; none can
//   overwrite a newer level with an older one.
// - `new` publishes PIN_A/PIN_B in the same critical section that installs the
//   seeded slot; `drop` zeroes them in the same critical section that takes
//   the slot. A `new` on another core can only claim the slot after that
//   section, so a late `drop` can never zero a successor's pins.
// - The hook loads the pin cells before its critical section and re-checks
//   them inside; if they changed, the slot belongs to another instance and the
//   hook skips decoding (it has only cleared status bits of the old pins).
// - `listen` clears pending status, so a transition between the seed read and
//   arming is invisible to the interrupt; `new` reconciles the live levels
//   after arming.
// - A hook running during `new`/`drop` sees either zero pin cells (no-op) or a
//   consistent slot; `listen`/`unlisten`/`clear_interrupt` stay outside the
//   critical section to keep the lock order GPIO_LOCK -> critical section.

/// Splits a pin-number-plus-one cell into per-bank masks `(bank0, bank1)`.
///
/// Returns `(0, 0)` for an empty cell. Bank 1 (GPIO32 and up) exists only on
/// the ESP32 and ESP32-S3; elsewhere a pin number cannot reach it.
fn bank_masks(cell: u32) -> (u32, u32) {
    let Some(pin) = cell.checked_sub(1) else {
        return (0, 0);
    };
    #[cfg(any(feature = "esp32", feature = "esp32s3"))]
    debug_assert!(pin < 64, "GPIO number out of range");
    #[cfg(not(any(feature = "esp32", feature = "esp32s3")))]
    debug_assert!(pin < 32, "GPIO number out of range");
    let bit = 1u32 << (pin % 32);
    if pin < 32 {
        (bit, 0)
    } else {
        (0, bit)
    }
}

/// Raw interrupt status `(bank0, bank1)`.
fn read_status() -> (u32, u32) {
    let gpio = GPIO::regs();
    let bank0 = gpio.status().read().bits();
    #[cfg(any(feature = "esp32", feature = "esp32s3"))]
    let bank1 = gpio.status1().read().bits();
    #[cfg(not(any(feature = "esp32", feature = "esp32s3")))]
    let bank1 = 0;
    (bank0, bank1)
}

/// Clears the given status bits (write-one-to-clear).
fn clear_status(bank0: u32, bank1: u32) {
    let gpio = GPIO::regs();
    if bank0 != 0 {
        // SAFETY: STATUS_W1TC only clears the status bits written as one; it
        // has no other side effect, and the bits belong to our own pins.
        gpio.status_w1tc().write(|w| unsafe { w.bits(bank0) });
    }
    #[cfg(any(feature = "esp32", feature = "esp32s3"))]
    if bank1 != 0 {
        // SAFETY: as above, for GPIO32 and up.
        gpio.status1_w1tc().write(|w| unsafe { w.bits(bank1) });
    }
    #[cfg(not(any(feature = "esp32", feature = "esp32s3")))]
    let _ = bank1;
}

/// Input levels `(bank0, bank1)`, one register read per bank.
fn read_input() -> (u32, u32) {
    let gpio = GPIO::regs();
    let bank0 = gpio.in_().read().bits();
    #[cfg(any(feature = "esp32", feature = "esp32s3"))]
    let bank1 = gpio.in1().read().bits();
    #[cfg(not(any(feature = "esp32", feature = "esp32s3")))]
    let bank1 = 0;
    (bank0, bank1)
}

/// Current `(A, B)` levels from one register read. Takes no GPIO lock.
fn read_levels(cell_a: u32, cell_b: u32) -> (bool, bool) {
    let (a0, a1) = bank_masks(cell_a);
    let (b0, b1) = bank_masks(cell_b);
    let (in0, in1) = read_input();
    (
        in0 & a0 != 0 || in1 & a1 != 0,
        in0 & b0 != 0 || in1 & b1 != 0,
    )
}

/// Feeds one sampled pair to the decoder if it changed. Shared by the hook and
/// the constructor's reconcile so they cannot diverge.
fn apply_sample(slot: &mut IsrSlot, now: (bool, bool)) {
    if now != slot.last {
        slot.last = now;
        slot.edge_count = slot.edge_count.wrapping_add(1);
        let _ = slot.decoder.update(now.0, now.1);
    }
}

/// Feeds the interrupt-driven encoder from the application's GPIO handler.
///
/// Call this from the handler installed with
/// `Io::set_interrupt_handler`.
/// It clears the encoder's A/B status bits before sampling the levels, so a
/// level change after the clear re-raises the interrupt (by design) instead of
/// being wiped unseen; as long as the handler calls this hook on every GPIO
/// interrupt, the encoder's bits cannot cause an interrupt storm.
/// It is a cheap no-op when no [`InterruptEncoder`] exists.
/// It does not touch any other pin's status bits; see the
/// [module docs](crate::rotary::interrupt) for the shared-handler duty.
///
/// # Panics
///
/// Never panics.
pub fn on_gpio_interrupt() {
    let cell_a = PIN_A.load(Ordering::Acquire);
    let cell_b = PIN_B.load(Ordering::Acquire);
    if cell_a == 0 || cell_b == 0 {
        return;
    }
    let (a0, a1) = bank_masks(cell_a);
    let (b0, b1) = bank_masks(cell_b);
    let (ours0, ours1) = (a0 | b0, a1 | b1);

    // Read the raw status first (only to gate the diagnostic count), then
    // clear unconditionally. The levels are sampled afterwards, inside the
    // critical section: an edge after the clear re-raises the interrupt, one
    // before it (including between the status read and the clear) is already
    // visible in the sample, so no edge is wiped unseen.
    let (status0, status1) = read_status();
    clear_status(ours0, ours1);
    let fired = (status0 & ours0 != 0) || (status1 & ours1 != 0);

    critical_section::with(|cs| {
        // The slot may have changed hands since the cells were loaded. Then the
        // clear above may have wiped a status bit of the new owner's pins, so
        // sample the new owner's pins instead of dropping that edge; its own
        // status bits, if set, re-raise the interrupt as usual.
        let (cur_a, cur_b) = (PIN_A.load(Ordering::Acquire), PIN_B.load(Ordering::Acquire));
        if cur_a == 0 || cur_b == 0 {
            return;
        }
        let mut slot = SLOT.borrow_ref_mut(cs);
        let Some(slot) = slot.as_mut() else { return };
        let same_owner = cur_a == cell_a && cur_b == cell_b;
        if fired && same_owner {
            slot.isr_count = slot.isr_count.wrapping_add(1);
        }
        apply_sample(slot, read_levels(cur_a, cur_b));
    });
}

/// Interrupt-driven rotary encoder with a button.
///
/// Quadrature decoding runs in the application's GPIO handler via
/// [`on_gpio_interrupt`]; [`InterruptEncoder::update`] only polls the button
/// for debounce/click/long-press/double-click timing.
/// See the [module docs](crate::rotary::interrupt) for the handler setup and the single-instance
/// rule.
pub struct InterruptEncoder<'d> {
    pin_a: Input<'d>,
    pin_b: Input<'d>,
    button: Input<'d>,
    button_sm: ButtonDecoder,
    /// Keeps the handle `!Send` so it is created and dropped on the core that
    /// runs the handler.
    _not_send: PhantomData<*const ()>,
}

impl<'d> InterruptEncoder<'d> {
    /// Creates a new encoder with the default [`EncoderConfig`] (EC11
    /// full-step).
    ///
    /// # Errors
    ///
    /// See [`new_with_config`](InterruptEncoder::new_with_config).
    pub fn new(
        pin_a: impl InputPin + 'd,
        pin_b: impl InputPin + 'd,
        button: impl InputPin + 'd,
    ) -> Result<Self, EncoderError> {
        Self::new_with_config(pin_a, pin_b, button, EncoderConfig::default())
    }

    /// Creates a new encoder with a custom [`EncoderConfig`].
    ///
    /// Configures all three pins as inputs with an internal pull-up, seeds the
    /// decoders from a *live* read of each pin (so a button already held at
    /// construction does not register a phantom press), and finally arms the
    /// any-edge interrupt on A and B.
    /// The application must already have installed its GPIO handler (see the
    /// [module docs](crate::rotary::interrupt)), otherwise nothing feeds the decoder.
    ///
    /// # Errors
    ///
    /// Returns [`EncoderError::InvalidConfig`] if
    /// `config.steps_per_detent == 0`, and [`EncoderError::AlreadyInUse`] if
    /// another [`InterruptEncoder`] is alive.
    ///
    /// # Panics
    ///
    /// Never panics: `steps_per_detent == 0` is rejected before
    /// [`QuadratureDecoder::new`] (which would panic on it) is called.
    pub fn new_with_config(
        pin_a: impl InputPin + 'd,
        pin_b: impl InputPin + 'd,
        button: impl InputPin + 'd,
        config: EncoderConfig,
    ) -> Result<Self, EncoderError> {
        if config.steps_per_detent == 0 {
            return Err(EncoderError::InvalidConfig);
        }

        // Claim the slot first so a second instance fails before it touches
        // any pin. The hook ignores the placeholder: the masks are still zero.
        let claimed = critical_section::with(|cs| {
            let mut slot = SLOT.borrow_ref_mut(cs);
            if slot.is_some() {
                false
            } else {
                *slot = Some(IsrSlot::new(true, true, config.steps_per_detent));
                true
            }
        });
        if !claimed {
            return Err(EncoderError::AlreadyInUse);
        }

        let cell_a = u32::from(pin_a.number()) + 1;
        let cell_b = u32::from(pin_b.number()) + 1;

        let input_config = InputConfig::default().with_pull(Pull::Up);
        let mut pin_a = Input::new(pin_a, input_config);
        let mut pin_b = Input::new(pin_b, input_config);
        let button = Input::new(button, input_config);
        let button_pressed = button.is_low();

        // Seed from the live levels and publish the pins in one critical
        // section (see the lifecycle notes above).
        critical_section::with(|cs| {
            let (a, b) = read_levels(cell_a, cell_b);
            *SLOT.borrow_ref_mut(cs) = Some(IsrSlot::new(a, b, config.steps_per_detent));
            PIN_A.store(cell_a, Ordering::Release);
            PIN_B.store(cell_b, Ordering::Release);
        });

        // Arm last and outside the critical section: `listen` takes esp-hal's
        // GPIO lock, which the hook holds while it takes ours.
        pin_a.listen(Event::AnyEdge);
        pin_b.listen(Event::AnyEdge);

        // `listen` clears pending status, so a transition between the seed and
        // the arming raised no interrupt. Reconcile with the live levels. The
        // read and its application share one critical section, exactly like
        // the hook's, so samples cannot apply out of order.
        critical_section::with(|cs| {
            if let Some(slot) = SLOT.borrow_ref_mut(cs).as_mut() {
                apply_sample(slot, read_levels(cell_a, cell_b));
            }
        });

        Ok(Self {
            pin_a,
            pin_b,
            button,
            button_sm: ButtonDecoder::new(
                button_pressed,
                config.debounce_ms,
                config.long_press_ms,
                config.double_click_ms,
            ),
            _not_send: PhantomData,
        })
    }

    /// Polls the button for debounce/click/long-press/double-click timing.
    ///
    /// Quadrature decoding happens in the interrupt; this method only reads
    /// the button pin and ticks the internal [`ButtonDecoder`], without taking
    /// any lock.
    /// `now_ms` is a monotonic millisecond timestamp — pass the same clock
    /// consistently across calls.
    /// A gesture is only ever detected on a call to `update`.
    pub fn update(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        let pressed = self.button.is_low();
        self.button_sm.update(pressed, now_ms)
    }

    /// Returns the current encoder position.
    #[must_use]
    pub fn position(&self) -> i32 {
        self.with_slot(|slot| slot.decoder.position()).unwrap_or(0)
    }

    /// Overwrites the encoder position and clears the decoder's internal
    /// accumulator.
    pub fn set_position(&mut self, position: i32) {
        let _ = self.with_slot(|slot| slot.decoder.set_position(position));
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

    /// Number of GPIO interrupts that carried an A/B edge since construction.
    ///
    /// **Diagnostic only** — it wraps (`u32`); take deltas with
    /// `wrapping_sub`.
    #[must_use]
    pub fn isr_count(&self) -> u32 {
        self.with_slot(|slot| slot.isr_count).unwrap_or(0)
    }

    /// Number of A/B level changes fed to the decoder since construction.
    ///
    /// **Diagnostic only** — it counts observed level changes, not detents;
    /// edges that coincide between interrupts are not counted individually; it
    /// wraps (`u32`).
    #[must_use]
    pub fn edge_count(&self) -> u32 {
        self.with_slot(|slot| slot.edge_count).unwrap_or(0)
    }

    /// Number of samples that skipped a Gray-code state since construction.
    ///
    /// **Diagnostic only** — contact bounce can also add to it. It wraps
    /// (`u32`); take deltas with `wrapping_sub`. See
    /// [`QuadratureDecoder::skipped_count`].
    #[must_use]
    pub fn skipped_count(&self) -> u32 {
        self.with_slot(|slot| slot.decoder.skipped_count())
            .unwrap_or(0)
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

    /// Runs `f` on the shared slot inside a short critical section.
    ///
    /// The slot is `Some` for the whole life of a handle (`new` fills it,
    /// `drop` empties it), so `None` is unreachable in practice.
    fn with_slot<R>(&self, f: impl FnOnce(&mut IsrSlot) -> R) -> Option<R> {
        critical_section::with(|cs| SLOT.borrow_ref_mut(cs).as_mut().map(f))
    }
}

impl Drop for InterruptEncoder<'_> {
    fn drop(&mut self) {
        // Outside the critical section, for the same lock-order reason as in
        // `new_with_config`.
        self.pin_a.unlisten();
        self.pin_b.unlisten();
        self.pin_a.clear_interrupt();
        self.pin_b.clear_interrupt();
        // Zero the pin cells and free the slot in one critical section, so a
        // `new` on another core cannot publish into the gap.
        critical_section::with(|cs| {
            PIN_A.store(0, Ordering::Release);
            PIN_B.store(0, Ordering::Release);
            SLOT.borrow_ref_mut(cs).take();
        });
    }
}
