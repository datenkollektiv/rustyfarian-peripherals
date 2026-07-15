//! Interrupt-driven rotary-encoder driver — [`Encoder`].
//!
//! Wraps two quadrature GPIO pins (A/B) and a push-button pin, and delegates
//! all decode/timing logic to the pure [`tamer`] state machines:
//! [`tamer::rotary::QuadratureDecoder`] for the A/B quadrature signal and
//! [`tamer::button::ButtonDecoder`] for the button's debounce/click/
//! long-press/double-click gestures. This module only reads GPIO and feeds
//! those state machines — no decoding happens here.
//!
//! # Polled vs. interrupt-driven quadrature
//!
//! [`tamer::rotary::QuadratureInput`] (behind `tamer`'s `hal` feature) is the
//! polled complement to this driver: it reads both pins and ticks
//! [`tamer::rotary::QuadratureDecoder`] once per call to `update`, from
//! whatever cadence the caller's main loop runs at. That is simple and
//! sufficient for a main loop with headroom, but a slow tick (a long display
//! DMA transfer, a blocking I/O call) can miss quadrature edges between
//! polls: a fast-turned encoder can produce more edges than fit in one
//! polling interval.
//!
//! [`Encoder`] instead arms both A and B pins for `AnyEdge` GPIO interrupts
//! via raw ESP-IDF FFI (`gpio_isr_handler_add`), so every quadrature edge is
//! captured regardless of main-loop latency — the same [`QuadratureDecoder`]
//! runs inside the ISR instead of inside `update`. Button timing (debounce,
//! long-press, double-click) inherently needs a timestamp from the caller, so
//! it remains polled: call [`Encoder::update`] regularly to drive it.
//!
//! `esp-idf-hal`'s `PinDriver::subscribe` + `enable_interrupt` is a one-shot
//! pattern that auto-disables after the first fire, which is unsuitable for
//! continuous quadrature decoding; this driver bypasses it in favor of
//! persistent, non-one-shot edge interrupts registered directly against the
//! ESP-IDF C API.
//!
//! # IRAM caveat (known v1 limitation)
//!
//! [`encoder_isr_trampoline`] is *not* IRAM-resident. An edge that arrives
//! while the flash cache is disabled (for example, during an NVS or OTA
//! write) cannot be serviced and can crash the device. Placing the ISR in
//! IRAM is roadmapped as a follow-up; until then, avoid flash-cache-disabling
//! operations while relying on encoder interrupts, or accept the risk on
//! hardware that never performs them.

use core::cell::RefCell;
use core::ffi::c_void;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use critical_section::Mutex;
use esp_idf_hal::gpio::{Input, InputPin, OutputPin, PinDriver, Pull};
use esp_idf_sys::{esp_err_t, EspError};

use tamer::button::ButtonDecoder;
pub use tamer::button::ButtonEvent;
pub use tamer::rotary::EncoderDirection;
use tamer::rotary::QuadratureDecoder;

/// Per-instance state shared between the GPIO ISR and the owning [`Encoder`].
///
/// Allocated once on the heap (`Box<IsrContext>`) so its address is stable
/// for the lifetime of the armed interrupts: a `*mut c_void` pointer to this
/// struct is registered with `gpio_isr_handler_add` for both the A and B
/// pins. It must never be embedded by value in [`Encoder`], because
/// `Encoder::new_with_config` returning `Self` by value would move it and
/// invalidate the pointer already handed to ESP-IDF.
struct IsrContext {
    /// The quadrature decoder, guarded by a critical section so the ISR and
    /// (via [`Encoder::set_position`] / [`Encoder::reset`]) the main task
    /// never observe a torn update.
    decoder: Mutex<RefCell<QuadratureDecoder>>,
    /// Accumulated encoder position, mirrored out of `decoder` on every
    /// confirmed detent so [`Encoder::position`] is a lock-free atomic load.
    position: AtomicI32,
    /// Total ISR invocations since the interrupts were armed (diagnostics).
    isr_count: AtomicU32,
    /// Tombstone flag: `false` before both pins are fully armed and after
    /// [`Encoder::drop`] begins teardown. The ISR checks this first and
    /// returns immediately when `false`, so it never touches `decoder` /
    /// `position` / `isr_count` outside the window in which they are known
    /// to be alive and consistent.
    armed: AtomicBool,
    /// GPIO number of the A pin, captured at construction.
    pin_a: i32,
    /// GPIO number of the B pin, captured at construction.
    pin_b: i32,
}

/// ISR body — shared by both the A and B pin interrupts.
///
/// Reads current GPIO levels via `gpio_get_level` (safe in ISR context),
/// feeds the quadrature decoder, and mirrors any confirmed detent onto the
/// atomic position counter. Performs no allocation, no clock reads, and no
/// blocking calls.
///
/// The entire body — the tombstone check *and* every subsequent touch of
/// `ctx` — runs inside a single [`critical_section::with`] call. This is
/// load-bearing, not just a locking convenience for `decoder`: [`Drop`]'s
/// teardown barrier is itself a `critical_section::with(|_| {})` call, and a
/// critical section is mutually exclusive across cores. Keeping every access
/// to `ctx` inside the same critical section this function uses means the
/// `Drop` barrier cannot return — and therefore cannot let `ctx` be freed —
/// while any invocation of this function that has already entered its
/// critical section is still running. Splitting `ctx` accesses across
/// multiple critical sections (or leaving any of them outside one entirely)
/// would reopen a window in which `Drop` observes no contention, frees
/// `ctx`, and a not-yet-finished ISR invocation then touches freed memory.
fn encoder_isr(ctx: &IsrContext) {
    critical_section::with(|cs| {
        // Tombstone check: bail out before touching anything else if the
        // `Encoder` is not yet fully armed or is mid-teardown.
        if !ctx.armed.load(Ordering::Acquire) {
            return;
        }

        // SAFETY: gpio_get_level reads a GPIO input-level hardware register
        // directly; this is safe to call from interrupt context. `ctx.pin_a`
        // / `ctx.pin_b` are valid GPIO numbers captured from live
        // `PinDriver`s in `Encoder::new_with_config`, before either
        // interrupt was armed.
        let a = unsafe { esp_idf_svc::sys::gpio_get_level(ctx.pin_a) } != 0;
        let b = unsafe { esp_idf_svc::sys::gpio_get_level(ctx.pin_b) } != 0;

        if let Some(direction) = ctx.decoder.borrow_ref_mut(cs).update(a, b) {
            match direction {
                EncoderDirection::Clockwise => {
                    ctx.position.fetch_add(1, Ordering::Relaxed);
                }
                EncoderDirection::CounterClockwise => {
                    ctx.position.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }

        ctx.isr_count.fetch_add(1, Ordering::Relaxed);
    });
}

/// C-ABI trampoline registered with `gpio_isr_handler_add` for both the A and
/// B pins.
///
/// # Safety
///
/// Called directly by the ESP-IDF GPIO ISR dispatcher on a hardware edge.
/// `arg` must be the `*mut c_void` produced from a live `&IsrContext` — the
/// same pointer is registered for both pins in `Encoder::new_with_config`.
/// The referenced `IsrContext` must remain valid for every call to this
/// function until both pins have been disarmed and the `Drop`
/// critical-section barrier has completed; `Encoder::drop` upholds this
/// ordering. The function body only reaches into `IsrContext` through
/// atomics and a `critical_section`, allocates nothing, and calls no
/// blocking or non-reentrant ESP-IDF API, so it is safe to execute in
/// interrupt context.
unsafe extern "C" fn encoder_isr_trampoline(arg: *mut c_void) {
    // SAFETY: see the function's safety contract above.
    let ctx = unsafe { &*arg.cast::<IsrContext>() };
    encoder_isr(ctx);
}

/// Installs the shared ESP-IDF GPIO ISR service, if not already installed.
///
/// `ESP_ERR_INVALID_STATE` is treated as success: it means another driver
/// (or a previous `Encoder`) already installed the process-wide service.
fn install_isr_service() -> Result<(), EncoderError> {
    // SAFETY: `gpio_install_isr_service` takes an interrupt-allocation flags
    // bitmask (`0` selects the default flags). It has no pointer arguments
    // and no lifetime requirements beyond being called before
    // `gpio_isr_handler_add`, which is upheld by only calling it here, ahead
    // of `arm_gpio_isr`.
    let ret = unsafe { esp_idf_svc::sys::gpio_install_isr_service(0) };
    if ret != esp_idf_svc::sys::ESP_OK && ret != esp_idf_svc::sys::ESP_ERR_INVALID_STATE {
        return Err(EncoderError::Isr {
            call: "gpio_install_isr_service",
            pin: -1,
            code: ret,
        });
    }
    Ok(())
}

/// Arms a persistent `AnyEdge` GPIO interrupt on `pin`, routing it to
/// `trampoline` with `ctx_ptr` as its `void*` argument.
///
/// Executes the ESP-IDF sequence `gpio_set_intr_type` →
/// `gpio_isr_handler_add` → `gpio_intr_enable`, returning on the first
/// failure.
///
/// # Safety
///
/// `ctx_ptr` must point to a live `IsrContext` that remains valid for as long
/// as this pin's interrupt stays armed, i.e. until [`disarm_gpio_isr`] is
/// called for the same pin. `trampoline` must be a valid, ISR-safe
/// `extern "C"` function pointer.
unsafe fn arm_gpio_isr(
    pin: i32,
    trampoline: unsafe extern "C" fn(*mut c_void),
    ctx_ptr: *mut c_void,
) -> Result<(), EncoderError> {
    // SAFETY: `pin` is a valid GPIO number (captured from a live `PinDriver`
    // by the caller); this only writes hardware interrupt-configuration
    // registers for that pin.
    let ret = unsafe {
        esp_idf_svc::sys::gpio_set_intr_type(
            pin,
            esp_idf_svc::sys::gpio_int_type_t_GPIO_INTR_ANYEDGE,
        )
    };
    if ret != esp_idf_svc::sys::ESP_OK {
        return Err(EncoderError::Isr {
            call: "gpio_set_intr_type",
            pin,
            code: ret,
        });
    }

    // SAFETY: `trampoline` is a `'static` `extern "C"` fn satisfying the ISR
    // ABI; `ctx_ptr` is valid for the duration the caller's safety contract
    // guarantees (upheld by `Encoder::new_with_config` / `Encoder::drop`).
    let ret = unsafe { esp_idf_svc::sys::gpio_isr_handler_add(pin, Some(trampoline), ctx_ptr) };
    if ret != esp_idf_svc::sys::ESP_OK {
        return Err(EncoderError::Isr {
            call: "gpio_isr_handler_add",
            pin,
            code: ret,
        });
    }

    // SAFETY: enables the interrupt line just registered above on a valid
    // GPIO number; no aliasing or lifetime hazard.
    let ret = unsafe { esp_idf_svc::sys::gpio_intr_enable(pin) };
    if ret != esp_idf_svc::sys::ESP_OK {
        return Err(EncoderError::Isr {
            call: "gpio_intr_enable",
            pin,
            code: ret,
        });
    }

    log::info!("GPIO {pin}: AnyEdge persistent ISR armed");
    Ok(())
}

/// Best-effort teardown of a single pin's `AnyEdge` ISR — the inverse of
/// [`arm_gpio_isr`]. Disables the interrupt, then removes the handler.
///
/// Both ESP-IDF calls are idempotent, so this is safe to invoke on a pin that
/// was only *partially* armed (or never armed at all): if `arm_gpio_isr`
/// failed at its final `gpio_intr_enable` step, `gpio_isr_handler_add` had
/// already committed the handler, and this removes it. Failures are logged,
/// never masked — a warning on a never-armed pin is harmless diagnostic
/// noise.
///
/// # Safety
///
/// `pin` must be a valid GPIO number, or one that was previously passed to
/// [`arm_gpio_isr`].
unsafe fn disarm_gpio_isr(pin: i32) {
    // SAFETY: disabling and removing an ISR handler on a pin that may or may
    // not currently have one registered is well-defined and idempotent in
    // ESP-IDF; both calls are safe to make from ordinary (non-ISR) context.
    let r1 = unsafe { esp_idf_svc::sys::gpio_intr_disable(pin) };
    let r2 = unsafe { esp_idf_svc::sys::gpio_isr_handler_remove(pin) };
    if r1 != esp_idf_svc::sys::ESP_OK || r2 != esp_idf_svc::sys::ESP_OK {
        log::warn!(
            "Encoder teardown (GPIO {pin}): gpio_intr_disable={r1} gpio_isr_handler_remove={r2}"
        );
    }
}

/// Errors returned by [`Encoder::new`] / [`Encoder::new_with_config`].
#[non_exhaustive]
#[derive(Debug)]
pub enum EncoderError {
    /// `steps_per_detent` was `0`. [`tamer::rotary::QuadratureDecoder::new`]
    /// panics on this value, so it is rejected here before construction.
    InvalidConfig,
    /// Configuring a GPIO pin as an input (`PinDriver::input`) failed.
    Pin(EspError),
    /// A raw ESP-IDF ISR-management call failed.
    Isr {
        /// The name of the failing FFI call, e.g. `"gpio_isr_handler_add"`.
        call: &'static str,
        /// The GPIO number the call targeted, or `-1` for a call (such as
        /// `gpio_install_isr_service`) that is not specific to one pin.
        pin: i32,
        /// The raw `esp_err_t` status code ESP-IDF returned.
        code: esp_err_t,
    },
}

impl fmt::Display for EncoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => write!(
                f,
                "invalid encoder configuration: steps_per_detent must be greater than zero"
            ),
            Self::Pin(e) => write!(f, "GPIO pin configuration failed: {e}"),
            Self::Isr { call, pin, code } => {
                if *pin < 0 {
                    write!(f, "{call}() failed: esp_err_t {code}")
                } else {
                    write!(f, "{call}(GPIO{pin}) failed: esp_err_t {code}")
                }
            }
        }
    }
}

impl std::error::Error for EncoderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pin(e) => Some(e),
            Self::InvalidConfig | Self::Isr { .. } => None,
        }
    }
}

impl From<EspError> for EncoderError {
    fn from(e: EspError) -> Self {
        Self::Pin(e)
    }
}

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

/// Interrupt-driven rotary encoder with a button.
///
/// Quadrature decoding runs entirely inside a GPIO ISR shared by the A and B
/// pins (see the [module docs](self)); [`Encoder::update`] only polls the
/// button for debounce/click/long-press/double-click timing.
///
/// `pin_a` and `pin_b` are retained as fields to keep the GPIO pins
/// configured (input + pull-up) for the encoder's lifetime, and to serve the
/// [`pin_a_is_high`](Encoder::pin_a_is_high) /
/// [`pin_b_is_high`](Encoder::pin_b_is_high) diagnostic reads. Interrupts are
/// registered via raw ESP-IDF FFI, *not* `PinDriver::subscribe`, so these
/// `PinDriver`s must outlive the armed interrupts — [`Drop`] tears the
/// interrupts down before the pins (and the heap-allocated ISR context) are
/// released.
pub struct Encoder<'d> {
    pin_a: PinDriver<'d, Input>,
    pin_b: PinDriver<'d, Input>,
    button: PinDriver<'d, Input>,
    button_sm: ButtonDecoder,
    /// Heap-allocated so its address is stable across `new_with_config`
    /// returning `Self` by value — see [`IsrContext`].
    ctx: Box<IsrContext>,
}

impl<'d> Encoder<'d> {
    /// Creates a new encoder with the default [`EncoderConfig`] (EC11
    /// full-step).
    ///
    /// # Errors
    ///
    /// See [`new_with_config`](Encoder::new_with_config).
    pub fn new(
        pin_a: impl InputPin + OutputPin + 'd,
        pin_b: impl InputPin + OutputPin + 'd,
        button: impl InputPin + OutputPin + 'd,
    ) -> Result<Self, EncoderError> {
        Self::new_with_config(pin_a, pin_b, button, EncoderConfig::default())
    }

    /// Creates a new encoder with a custom [`EncoderConfig`].
    ///
    /// Configures all three pins as inputs with an internal pull-up, seeds
    /// the [`QuadratureDecoder`] and [`ButtonDecoder`] from a *live* read of
    /// each pin (so a button already held at construction does not register
    /// a phantom press), and arms persistent `AnyEdge` interrupts on the A
    /// and B pins.
    ///
    /// # Errors
    ///
    /// Returns [`EncoderError::InvalidConfig`] if
    /// `config.steps_per_detent == 0`. Returns [`EncoderError::Pin`] if
    /// configuring any of the three pins as an input fails. Returns
    /// [`EncoderError::Isr`] if installing the shared ESP-IDF GPIO ISR
    /// service, or arming the interrupt on either the A or B pin, fails; on
    /// this path any interrupt already armed for this instance is disarmed
    /// before returning, so no dangling ISR registration is left behind.
    ///
    /// # Panics
    ///
    /// Never panics: `steps_per_detent == 0` is validated and rejected
    /// before [`QuadratureDecoder::new`] (which would otherwise panic on
    /// that input) is called.
    pub fn new_with_config(
        pin_a: impl InputPin + OutputPin + 'd,
        pin_b: impl InputPin + OutputPin + 'd,
        button: impl InputPin + OutputPin + 'd,
        config: EncoderConfig,
    ) -> Result<Self, EncoderError> {
        if config.steps_per_detent == 0 {
            return Err(EncoderError::InvalidConfig);
        }

        let pin_a = PinDriver::input(pin_a, Pull::Up)?;
        let pin_b = PinDriver::input(pin_b, Pull::Up)?;
        let button = PinDriver::input(button, Pull::Up)?;

        // `PinDriver::pin()` returns `PinId` (`u8`) in esp-idf-hal 0.46;
        // widen to `i32` for both the `IsrContext` field and the raw
        // ESP-IDF FFI calls below, which expect `gpio_num_t`.
        let num_a = i32::from(pin_a.pin());
        let num_b = i32::from(pin_b.pin());

        // Read the initial A/B state before arming interrupts, to seed the
        // decoder from the live hardware state rather than an assumed one.
        let initial_a = pin_a.is_high();
        let initial_b = pin_b.is_high();
        log::info!(
            "Encoder init: A(GPIO{num_a})={initial_a} B(GPIO{num_b})={initial_b} steps_per_detent={}",
            config.steps_per_detent
        );

        let ctx = Box::new(IsrContext {
            decoder: Mutex::new(RefCell::new(QuadratureDecoder::new(
                initial_a,
                initial_b,
                config.steps_per_detent,
            ))),
            position: AtomicI32::new(0),
            isr_count: AtomicU32::new(0),
            armed: AtomicBool::new(false),
            pin_a: num_a,
            pin_b: num_b,
        });

        install_isr_service()?;

        // The pointer registered with ESP-IDF for both pins. Taken from the
        // `Box`'s referent (not `Box::into_raw`) — the `Box` field on
        // `Encoder` keeps ownership and its heap allocation stable, so this
        // pointer stays valid for as long as `ctx` (ultimately `self.ctx`)
        // is alive.
        let ctx_ptr = core::ptr::from_ref(ctx.as_ref()) as *mut c_void;

        log::info!("Configuring encoder A (GPIO {num_a})...");
        // SAFETY: `ctx_ptr` points at `ctx`, which outlives the armed
        // interrupt (`Encoder::drop` disarms both pins and waits out a
        // critical-section barrier before the `Box` is freed).
        // `encoder_isr_trampoline` is a `'static` ISR-safe `extern "C"` fn.
        if let Err(e) = unsafe { arm_gpio_isr(num_a, encoder_isr_trampoline, ctx_ptr) } {
            // SAFETY: pin A was, at most, partially armed by the failed call
            // above; `disarm_gpio_isr` is idempotent and safe to call
            // regardless of how far arming got.
            unsafe { disarm_gpio_isr(num_a) };
            return Err(e);
        }

        log::info!("Configuring encoder B (GPIO {num_b})...");
        if let Err(e) = unsafe { arm_gpio_isr(num_b, encoder_isr_trampoline, ctx_ptr) } {
            // SAFETY: pin A was fully armed above and must be disarmed so no
            // dangling ISR fires on it after this constructor returns `Err`.
            // Pin B is disarmed defensively in case it was partially armed
            // (e.g. `gpio_isr_handler_add` succeeded but `gpio_intr_enable`
            // failed) — `disarm_gpio_isr` is idempotent either way.
            unsafe {
                disarm_gpio_isr(num_a);
                disarm_gpio_isr(num_b);
            }
            return Err(e);
        }

        // Publish `armed = true` only now that BOTH pins are fully armed.
        // The ISR's tombstone check (`if !armed { return }`) must never
        // observe `true` before both interrupts are ready to be serviced;
        // any edge that lands on the already-armed A pin while B is still
        // being configured is safely dropped by that check.
        ctx.armed.store(true, Ordering::Release);

        // Seed the button decoder from a live read, so a button already held
        // at construction does not register an artificial press.
        let button_pressed = button.is_low();
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
            ctx,
        })
    }

    /// Polls the button for debounce/click/long-press/double-click timing.
    ///
    /// Quadrature decoding is handled entirely by the GPIO ISR; this method
    /// only reads the button pin and ticks the internal [`ButtonDecoder`].
    /// `now_ms` is a monotonic millisecond timestamp — pass the same clock
    /// consistently across calls. Call this regularly (the debounce/
    /// long-press/double-click windows are measured against `now_ms`, not
    /// against call frequency, but a gesture is only ever detected on a call
    /// to `update`).
    pub fn update(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        let pressed = self.button.is_low();
        self.button_sm.update(pressed, now_ms)
    }

    /// Returns the current encoder position (a lock-free atomic load).
    #[must_use]
    pub fn position(&self) -> i32 {
        self.ctx.position.load(Ordering::Relaxed)
    }

    /// Overwrites the encoder position and clears the decoder's internal
    /// accumulator.
    pub fn set_position(&mut self, position: i32) {
        self.ctx.position.store(position, Ordering::Relaxed);
        critical_section::with(|cs| {
            self.ctx.decoder.borrow_ref_mut(cs).set_position(position);
        });
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

    /// Total ISR invocation count since the interrupts were armed
    /// (diagnostics).
    #[must_use]
    pub fn isr_count(&self) -> u32 {
        self.ctx.isr_count.load(Ordering::Relaxed)
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

impl Drop for Encoder<'_> {
    /// Tears down the armed interrupts before the `PinDriver`s and the
    /// heap-allocated [`IsrContext`] are released.
    ///
    /// # Safety
    ///
    /// The teardown order below is load-bearing and closes a use-after-free
    /// race against an ISR that may be mid-flight on another core (e.g. a
    /// dual-core ESP32-S3) at the moment `drop` runs:
    ///
    /// 1. Disable both pins' interrupts (`gpio_intr_disable`) so no *new*
    ///    edge can trigger the ISR.
    /// 2. Remove both pins' handlers (`gpio_isr_handler_remove`) so ESP-IDF
    ///    no longer dispatches to [`encoder_isr_trampoline`] for either pin.
    /// 3. Publish `armed = false` (`Release`), then take a
    ///    `critical_section::with(|_| {})` barrier. Steps 1-2 only stop
    ///    *future* interrupts — an edge that fired just before them can
    ///    still be executing `encoder_isr` on another core. [`encoder_isr`]
    ///    wraps its *entire* body (tombstone check included) in one
    ///    `critical_section::with` call, and a critical section is mutually
    ///    exclusive across cores, so this barrier cannot return — and this
    ///    function cannot proceed to step 4 — until any such in-flight ISR
    ///    invocation that has already entered its critical section has
    ///    exited it, having finished touching `self.ctx`. Any ISR invocation
    ///    that instead enters its critical section *after* this barrier
    ///    (none should, but the tombstone check is defense in depth) sees
    ///    `armed == false` and returns immediately instead of touching
    ///    `self.ctx`.
    /// 4. `self.ctx` (the `Box<IsrContext>`) is then dropped by the compiler
    ///    after this function returns, freeing the heap allocation — safe
    ///    only because steps 1-3 have already proven nothing can still
    ///    observe it.
    ///
    /// Steps 1-2 are best-effort: `gpio_intr_disable` /
    /// `gpio_isr_handler_remove` failures are logged (`log::warn!`) inside
    /// [`disarm_gpio_isr`], never propagated — `Drop::drop` cannot return a
    /// `Result`.
    ///
    /// Residual risk (needs device confirmation): the barrier above only
    /// synchronizes with an ISR
    /// invocation that has *already entered* its critical section by the
    /// time this function's own `critical_section::with` call is made. It
    /// does not, by itself, prove that an invocation dispatched a few
    /// instructions earlier — after the hardware edge fired but before
    /// [`encoder_isr`] reaches its `critical_section::with` call — cannot
    /// still race a subsequent `Box` free. In practice this window is a
    /// handful of instructions (the trampoline's pointer cast plus the call
    /// into `encoder_isr`), and ESP-IDF's shared GPIO ISR service is
    /// expected to serialize `gpio_isr_handler_remove` against any in-flight
    /// dispatch of the handler being removed on another core — which would
    /// make step 2 alone sufficient and this barrier pure defense in depth.
    /// That expectation has not been confirmed against ESP-IDF's internal
    /// locking and should be verified (either by reading the ESP-IDF GPIO
    /// driver source for this IDF version, or by a hardware stress test that
    /// repeatedly constructs/drops an `Encoder` while another core spins the
    /// physical encoder) before relying on this driver in a
    /// safety-sensitive context.
    fn drop(&mut self) {
        // SAFETY: `pin_a` / `pin_b` were successfully armed together in
        // `new_with_config` — a construction failure tears itself down and
        // never produces an `Encoder` — so both are valid, previously-armed
        // GPIO numbers.
        unsafe {
            disarm_gpio_isr(self.ctx.pin_a);
            disarm_gpio_isr(self.ctx.pin_b);
        }

        // Tombstone: any ISR invocation still in flight (or, defensively,
        // one that fires despite step 1-2 above) now sees `armed == false`
        // and returns immediately without touching `self.ctx`.
        self.ctx.armed.store(false, Ordering::Release);

        // Barrier: cannot return until any in-flight ISR on another core has
        // exited its own `critical_section::with` call inside `encoder_isr`.
        // After this returns, no ISR can still be reading `self.ctx`, so it
        // is safe for the compiler to free the `Box<IsrContext>` next.
        critical_section::with(|_cs| {});
    }
}
