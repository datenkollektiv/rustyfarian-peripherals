//! Attach/detach decisions for a removable device.
//!
//! A device on a connector that can be unplugged while the node runs — an
//! SSD1306 OLED on a magnetic pogo connector, for example — needs one pure
//! answer: has it attached, or has it detached?
//! [`Hotplug`] decides that from the outcome of each probe or transfer.
//! It never touches a bus; the caller feeds `ok: bool`, such as the result of a
//! zero-length-write probe where an address NACK is `false`.
//!
//! The machine is deliberately asymmetric.
//! Attaching needs `ok` to hold for `attach_debounce` ticks without a gap,
//! because contacts bounce while a connector seats.
//! Detaching needs `detach_errors` consecutive failures, because transfer
//! errors are discrete events rather than a sampled level.
//! Ticks are unit-free `u64` values chosen by the caller.
//!
//! # Probe cadence
//!
//! Only an observed failure restarts the attach window.
//! Silence cannot prove continuity: two successful probes far apart attach on
//! the second one, whatever happened in between.
//! Probe at a steady cadence well below `attach_debounce`.
//!
//! # What to observe
//!
//! Feed one deliberate stream of health observations, not every bus operation.
//! A successful address probe between failed transfers resets the failure
//! count, so mixing the two can keep an unusable device attached.
//! Decide what "attached" means for your application — the device answers its
//! address, or it completes the transfers you need — and report that outcome.
//!
//! # Example
//!
//! ```
//! use tamer::hotplug::{Hotplug, Transition};
//!
//! // Attach after 50 ticks of continuous success; detach after 3 failures.
//! let mut oled = Hotplug::new(50, 3);
//! assert!(!oled.is_attached());
//!
//! // The connector seats: probes succeed for the whole window.
//! assert_eq!(oled.observe(true, 0), None);
//! assert_eq!(oled.observe(true, 25), None);
//! assert_eq!(oled.observe(true, 50), Some(Transition::Attached));
//! assert!(oled.is_attached());
//!
//! // One failed transfer is tolerated, and a success resets the count.
//! assert_eq!(oled.observe(false, 60), None);
//! assert_eq!(oled.observe(true, 70), None);
//!
//! // The connector is pulled: three consecutive failures detach.
//! assert_eq!(oled.observe(false, 80), None);
//! assert_eq!(oled.observe(false, 90), None);
//! assert_eq!(oled.observe(false, 100), Some(Transition::Detached));
//! assert!(!oled.is_attached());
//! ```

use crate::presence::{DigitalPresence, Polarity, Presence};

/// A change in attachment reported by [`Hotplug::observe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transition {
    /// The device has been present for the full attach window.
    Attached,
    /// The device has failed the configured number of consecutive probes.
    Detached,
}

/// Attach/detach state machine for a removable device.
///
/// `Hotplug` starts detached.
/// Feed it one `ok` outcome per probe or transfer through
/// [`observe`](Hotplug::observe); it returns a [`Transition`] only when the
/// attachment state changes.
///
/// While detached, the attach window is a
/// [`DigitalPresence`] debounce: an unbroken run of `ok == true` samples
/// attaches on the first `true` at least `attach_debounce` ticks after the
/// start of the run, and any `false` restarts the window.
/// A gap between samples does not restart it; see
/// [Probe cadence](crate::hotplug#probe-cadence).
/// While attached, `detach_errors` consecutive `false` samples detach, and any
/// `true` resets the failure count.
/// Detaching ignores time entirely.
///
/// The caller controls the clock by passing monotonic tick values (`u64`) in a
/// consistent unit.
/// Non-monotonic timestamps never cause a spurious transition; they can only
/// delay an attach.
#[derive(Debug, Clone, Copy)]
pub struct Hotplug {
    presence: DigitalPresence,
    attach_debounce: u64,
    detach_errors: u32,
    failures: u32,
}

impl Hotplug {
    /// Creates a detached state machine.
    ///
    /// `attach_debounce` is the number of ticks `ok` must hold without a gap
    /// before the device counts as attached; `0` attaches on the first `true`.
    /// `detach_errors` is the number of consecutive failures that detach an
    /// attached device; `1` detaches on the first failure.
    ///
    /// # Panics
    ///
    /// Panics if `detach_errors == 0`.
    ///
    /// ```should_panic
    /// use tamer::hotplug::Hotplug;
    ///
    /// let _ = Hotplug::new(0, 0);
    /// ```
    #[must_use]
    pub fn new(attach_debounce: u64, detach_errors: u32) -> Self {
        assert!(detach_errors > 0, "hotplug detach_errors must be non-zero");
        Self {
            presence: Self::detached_presence(attach_debounce),
            attach_debounce,
            detach_errors,
            failures: 0,
        }
    }

    fn detached_presence(attach_debounce: u64) -> DigitalPresence {
        DigitalPresence::new(false, Polarity::ActiveHigh, attach_debounce)
    }

    /// Drops to the detached state, discarding any pending attach window and
    /// failure count.
    fn drop_to_detached(&mut self) {
        self.presence = Self::detached_presence(self.attach_debounce);
        self.failures = 0;
    }

    /// Feeds the outcome of one probe or transfer at the given timestamp.
    ///
    /// `ok` is `true` when the device answered and `false` when it did not
    /// (for example an address NACK on a zero-length write).
    ///
    /// Returns `Some(Transition::Attached)` or `Some(Transition::Detached)`
    /// when the state changes, and `None` otherwise.
    /// A transition is reported exactly once.
    pub fn observe(&mut self, ok: bool, now: u64) -> Option<Transition> {
        if self.is_attached() {
            if ok {
                self.failures = 0;
                return None;
            }
            self.failures = self.failures.saturating_add(1);
            if self.failures >= self.detach_errors {
                self.drop_to_detached();
                return Some(Transition::Detached);
            }
            return None;
        }

        match self.presence.update(ok, now) {
            Some(Presence::Present) => Some(Transition::Attached),
            _ => None,
        }
    }

    /// Drops to the detached state without reporting a transition.
    ///
    /// Any pending attach window and failure count are discarded.
    /// Use this after the caller has already handled the loss, for example
    /// after rebuilding the driver.
    pub fn reset(&mut self) {
        self.drop_to_detached();
    }

    /// Returns `true` if the device is currently attached.
    ///
    /// Useful for callers that poll state instead of consuming
    /// [`Transition`] values.
    #[must_use]
    pub fn is_attached(&self) -> bool {
        self.presence.stable_state().is_present()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_detached() {
        assert!(!Hotplug::new(50, 3).is_attached());
    }

    #[test]
    #[should_panic(expected = "detach_errors must be non-zero")]
    fn new_zero_detach_errors_panics() {
        let _ = Hotplug::new(50, 0);
    }

    #[test]
    fn attach_after_continuous_window() {
        let mut h = Hotplug::new(50, 3);
        assert_eq!(h.observe(true, 0), None);
        assert_eq!(h.observe(true, 30), None);
        assert_eq!(h.observe(true, 70), Some(Transition::Attached));
        assert!(h.is_attached());
    }

    #[test]
    fn attach_exactly_at_window_edge() {
        let mut h = Hotplug::new(50, 3);
        assert_eq!(h.observe(true, 10), None);
        assert_eq!(h.observe(true, 59), None);
        assert_eq!(h.observe(true, 60), Some(Transition::Attached));
    }

    #[test]
    fn single_true_does_not_attach_with_positive_debounce() {
        let mut h = Hotplug::new(50, 3);
        assert_eq!(h.observe(true, 0), None);
        assert!(!h.is_attached());
    }

    #[test]
    fn attach_zero_debounce_first_true_attaches() {
        let mut h = Hotplug::new(0, 3);
        assert_eq!(h.observe(true, 0), Some(Transition::Attached));
    }

    #[test]
    fn bounce_restarts_attach_window() {
        let mut h = Hotplug::new(50, 3);
        assert_eq!(h.observe(true, 0), None);
        assert_eq!(h.observe(false, 30), None);
        assert_eq!(h.observe(true, 40), None);
        assert_eq!(h.observe(true, 80), None);
        assert_eq!(h.observe(true, 90), Some(Transition::Attached));
    }

    #[test]
    fn false_while_detached_is_silent() {
        let mut h = Hotplug::new(50, 3);
        for t in 0..10 {
            assert_eq!(h.observe(false, t * 10), None);
        }
        assert!(!h.is_attached());
    }

    #[test]
    fn attach_emits_only_once() {
        let mut h = Hotplug::new(10, 3);
        h.observe(true, 0);
        assert_eq!(h.observe(true, 10), Some(Transition::Attached));
        assert_eq!(h.observe(true, 20), None);
        assert_eq!(h.observe(true, 30), None);
    }

    #[test]
    fn one_failure_tolerated_while_attached() {
        let mut h = Hotplug::new(0, 3);
        h.observe(true, 0);
        assert_eq!(h.observe(false, 1), None);
        assert!(h.is_attached());
    }

    #[test]
    fn ok_between_failures_resets_count() {
        let mut h = Hotplug::new(0, 3);
        h.observe(true, 0);
        assert_eq!(h.observe(false, 1), None);
        assert_eq!(h.observe(false, 2), None);
        assert_eq!(h.observe(true, 3), None);
        assert_eq!(h.observe(false, 4), None);
        assert_eq!(h.observe(false, 5), None);
        assert!(h.is_attached());
        assert_eq!(h.observe(false, 6), Some(Transition::Detached));
    }

    #[test]
    fn consecutive_failures_detach() {
        let mut h = Hotplug::new(0, 3);
        h.observe(true, 0);
        assert_eq!(h.observe(false, 1), None);
        assert_eq!(h.observe(false, 2), None);
        assert_eq!(h.observe(false, 3), Some(Transition::Detached));
        assert!(!h.is_attached());
    }

    #[test]
    fn detach_errors_one_detaches_on_first_failure() {
        let mut h = Hotplug::new(0, 1);
        h.observe(true, 0);
        assert_eq!(h.observe(false, 1), Some(Transition::Detached));
    }

    #[test]
    fn detach_emits_only_once() {
        let mut h = Hotplug::new(0, 1);
        h.observe(true, 0);
        assert_eq!(h.observe(false, 1), Some(Transition::Detached));
        assert_eq!(h.observe(false, 2), None);
        assert_eq!(h.observe(false, 3), None);
    }

    #[test]
    fn reattach_after_detach_needs_full_window() {
        let mut h = Hotplug::new(50, 1);
        h.observe(true, 0);
        assert_eq!(h.observe(true, 50), Some(Transition::Attached));
        assert_eq!(h.observe(false, 60), Some(Transition::Detached));
        assert_eq!(h.observe(true, 70), None);
        assert_eq!(h.observe(true, 119), None);
        assert_eq!(h.observe(true, 120), Some(Transition::Attached));
    }

    #[test]
    fn failure_count_cleared_across_automatic_reattach() {
        let mut h = Hotplug::new(10, 3);
        h.observe(true, 0);
        assert_eq!(h.observe(true, 10), Some(Transition::Attached));
        assert_eq!(h.observe(false, 11), None);
        assert_eq!(h.observe(false, 12), None);
        assert_eq!(h.observe(false, 13), Some(Transition::Detached));
        h.observe(true, 20);
        assert_eq!(h.observe(true, 30), Some(Transition::Attached));
        assert_eq!(h.observe(false, 31), None);
        assert_eq!(h.observe(false, 32), None);
        assert!(h.is_attached());
        assert_eq!(h.observe(false, 33), Some(Transition::Detached));
    }

    #[test]
    fn sparse_probes_attach_without_proving_continuity() {
        let mut h = Hotplug::new(50, 3);
        assert_eq!(h.observe(true, 0), None);
        assert_eq!(h.observe(true, 1_000), Some(Transition::Attached));
    }

    #[test]
    fn reset_while_attached_is_silent_and_detaches() {
        let mut h = Hotplug::new(0, 3);
        h.observe(true, 0);
        assert!(h.is_attached());
        h.reset();
        assert!(!h.is_attached());
        assert_eq!(h.observe(false, 1), None);
    }

    #[test]
    fn reset_while_detached_clears_pending_window() {
        let mut h = Hotplug::new(50, 3);
        h.observe(true, 0);
        h.reset();
        assert_eq!(h.observe(true, 50), None);
        assert_eq!(h.observe(true, 100), Some(Transition::Attached));
    }

    #[test]
    fn reset_clears_failure_count() {
        let mut h = Hotplug::new(0, 3);
        h.observe(true, 0);
        h.observe(false, 1);
        h.observe(false, 2);
        h.reset();
        assert_eq!(h.observe(true, 3), Some(Transition::Attached));
        assert_eq!(h.observe(false, 4), None);
        assert_eq!(h.observe(false, 5), None);
        assert!(h.is_attached());
    }

    #[test]
    fn is_attached_tracks_transitions() {
        let mut h = Hotplug::new(0, 1);
        assert!(!h.is_attached());
        h.observe(true, 0);
        assert!(h.is_attached());
        h.observe(false, 1);
        assert!(!h.is_attached());
    }

    #[test]
    fn non_monotonic_now_delays_attach() {
        let mut h = Hotplug::new(50, 3);
        assert_eq!(h.observe(true, 100), None);
        assert_eq!(h.observe(true, 60), None);
        assert!(!h.is_attached());
        assert_eq!(h.observe(true, 150), Some(Transition::Attached));
    }

    #[test]
    fn detach_ignores_time() {
        let mut h = Hotplug::new(0, 2);
        h.observe(true, 1000);
        assert_eq!(h.observe(false, 500), None);
        assert_eq!(h.observe(false, 0), Some(Transition::Detached));
    }

    #[test]
    fn ticks_near_u64_max_attach() {
        let mut h = Hotplug::new(50, 3);
        assert_eq!(h.observe(true, u64::MAX - 50), None);
        assert_eq!(h.observe(true, u64::MAX - 1), None);
        assert_eq!(h.observe(true, u64::MAX), Some(Transition::Attached));
    }
}
