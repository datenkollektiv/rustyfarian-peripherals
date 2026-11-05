//! Transition-burst detection over a stream of event flags.
//!
//! [`BurstDetector`] counts transitions inside a time window and reports a
//! [`BurstEvent::Burst`] when enough arrive, then a [`BurstEvent::Quiet`]
//! after a period without any.
//! It is a pure state machine: time is an input and no clock is read.

/// An event emitted by [`BurstDetector::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurstEvent {
    /// The transition count reached the threshold inside one window.
    Burst,
    /// The detector was bursting and no transition arrived for the quiet period.
    Quiet,
}

/// Burst detector over a stream of transition flags.
///
/// Counts transitions inside a window (for example from a chattering
/// ball-in-cylinder tilt switch).
/// Reaching `edge_threshold` transitions within `window_ms` emits
/// [`BurstEvent::Burst`] once; after `quiet_ms` without a transition it
/// emits [`BurstEvent::Quiet`] and starts over.
///
/// Feed it `true` whenever the input changed, for example when
/// [`DigitalPresence::update`](crate::presence::DigitalPresence::update) returned `Some`.
/// Time is an input (`now_ms`); the detector never reads a clock.
/// A threshold of `0` or `1` emits `Burst` on the first transition.
///
/// # Examples
///
/// ```
/// use tamer::burst::{BurstDetector, BurstEvent};
///
/// let mut burst = BurstDetector::new(1_000, 3, 500);
/// assert_eq!(burst.update(true, 0), None);
/// assert_eq!(burst.update(true, 100), None);
/// assert_eq!(burst.update(true, 200), Some(BurstEvent::Burst));
/// assert!(burst.is_bursting());
/// assert_eq!(burst.update(false, 699), None);
/// assert_eq!(burst.update(false, 700), Some(BurstEvent::Quiet));
/// assert!(!burst.is_bursting());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BurstDetector {
    window_ms: u64,
    edge_threshold: u8,
    quiet_ms: u64,
    window_started_ms: u64,
    count: u8,
    last_transition_ms: u64,
    bursting: bool,
}

impl BurstDetector {
    /// Creates a detector.
    ///
    /// A transition more than `window_ms` after the window start opens a new
    /// window.
    /// `quiet_ms` is the time without transitions after which a bursting
    /// detector reports [`BurstEvent::Quiet`].
    #[must_use]
    pub const fn new(window_ms: u64, edge_threshold: u8, quiet_ms: u64) -> Self {
        Self {
            window_ms,
            edge_threshold,
            quiet_ms,
            window_started_ms: 0,
            count: 0,
            last_transition_ms: 0,
            bursting: false,
        }
    }

    /// Advances the detector.
    ///
    /// `transition` is `true` when the debounced presence input changed on
    /// this pass.
    /// If both events would fire (only possible with `quiet_ms == 0`),
    /// [`BurstEvent::Burst`] wins.
    pub fn update(&mut self, transition: bool, now_ms: u64) -> Option<BurstEvent> {
        let mut event = None;
        if transition {
            if now_ms.saturating_sub(self.window_started_ms) > self.window_ms {
                self.window_started_ms = now_ms;
                self.count = 0;
            }
            self.count = self.count.saturating_add(1);
            self.last_transition_ms = now_ms;
            if !self.bursting && self.count >= self.edge_threshold {
                self.bursting = true;
                event = Some(BurstEvent::Burst);
            }
        }
        if event.is_none()
            && self.bursting
            && now_ms.saturating_sub(self.last_transition_ms) >= self.quiet_ms
        {
            self.bursting = false;
            self.count = 0;
            self.window_started_ms = now_ms;
            event = Some(BurstEvent::Quiet);
        }
        event
    }

    /// Transitions counted in the current window.
    #[must_use]
    pub const fn transition_count(&self) -> u8 {
        self.count
    }

    /// Returns `true` between a [`BurstEvent::Burst`] and the following
    /// [`BurstEvent::Quiet`].
    #[must_use]
    pub const fn is_bursting(&self) -> bool {
        self.bursting
    }

    /// Clears all state; the configuration is kept.
    pub fn reset(&mut self) {
        self.window_started_ms = 0;
        self.count = 0;
        self.last_transition_ms = 0;
        self.bursting = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presence::{DigitalPresence, Polarity};

    fn burst(m: &mut BurstDetector, n: u8, start: u64) -> Option<BurstEvent> {
        let mut last = None;
        for i in 0..n {
            last = m.update(true, start + u64::from(i));
        }
        last
    }

    #[test]
    fn burst_below_threshold_then_at_threshold() {
        let mut m = BurstDetector::new(1_000, 4, 1_200);
        assert_eq!(burst(&mut m, 3, 0), None);
        assert!(!m.is_bursting());
        assert_eq!(m.update(true, 3), Some(BurstEvent::Burst));
        assert!(m.is_bursting());
    }

    #[test]
    fn window_edge_is_strict() {
        let mut m = BurstDetector::new(1_000, 4, 1_200);
        m.update(true, 0);
        m.update(true, 1_000);
        assert_eq!(m.transition_count(), 2);
        let mut m = BurstDetector::new(1_000, 4, 1_200);
        m.update(true, 0);
        m.update(true, 1_001);
        assert_eq!(m.transition_count(), 1);
    }

    #[test]
    fn burst_emitted_once() {
        let mut m = BurstDetector::new(1_000, 2, 1_200);
        assert_eq!(burst(&mut m, 2, 0), Some(BurstEvent::Burst));
        assert_eq!(m.update(true, 10), None);
        assert_eq!(m.update(true, 11), None);
    }

    #[test]
    fn quiet_boundary() {
        let mut m = BurstDetector::new(1_000, 2, 500);
        burst(&mut m, 2, 0); // last transition at 1
        assert_eq!(m.update(false, 500), None);
        assert_eq!(m.update(false, 501), Some(BurstEvent::Quiet));
    }

    #[test]
    fn transition_refreshes_quiet_timer() {
        let mut m = BurstDetector::new(10_000, 2, 500);
        burst(&mut m, 2, 0);
        assert_eq!(m.update(true, 400), None);
        assert_eq!(m.update(false, 899), None);
        assert_eq!(m.update(false, 900), Some(BurstEvent::Quiet));
    }

    #[test]
    fn quiet_resets_and_needs_full_threshold() {
        let mut m = BurstDetector::new(1_000, 3, 100);
        burst(&mut m, 3, 0);
        assert_eq!(m.update(false, 200), Some(BurstEvent::Quiet));
        assert_eq!(m.transition_count(), 0);
        assert!(!m.is_bursting());
        assert_eq!(burst(&mut m, 2, 300), None);
        assert_eq!(m.update(true, 302), Some(BurstEvent::Burst));
    }

    #[test]
    fn count_saturates() {
        let mut m = BurstDetector::new(u64::MAX, u8::MAX, 100);
        for i in 0..300u64 {
            m.update(true, i);
        }
        assert_eq!(m.transition_count(), u8::MAX);
    }

    #[test]
    fn no_quiet_when_never_bursting() {
        let mut m = BurstDetector::new(1_000, 4, 100);
        m.update(true, 0);
        assert_eq!(m.update(false, 10_000), None);
    }

    #[test]
    fn reset_clears_state() {
        let mut m = BurstDetector::new(1_000, 2, 100);
        burst(&mut m, 2, 0);
        m.reset();
        assert!(!m.is_bursting());
        assert_eq!(m.transition_count(), 0);
        assert_eq!(m.update(false, 10_000), None);
    }

    #[test]
    fn large_now_does_not_underflow() {
        let mut m = BurstDetector::new(1_000, 2, 100);
        let base = u64::MAX - 10;
        assert_eq!(m.update(true, base), None);
        assert_eq!(m.update(true, base + 1), Some(BurstEvent::Burst));
    }

    #[test]
    fn zero_quiet_burst_wins() {
        let mut m = BurstDetector::new(1_000, 1, 0);
        assert_eq!(m.update(true, 5), Some(BurstEvent::Burst));
        assert_eq!(m.update(false, 5), Some(BurstEvent::Quiet));
    }

    #[test]
    fn chattering_presence_drives_burst() {
        let mut p = DigitalPresence::new(true, Polarity::ActiveLow, 2);
        let mut m = BurstDetector::new(1_000, 3, 200);
        let mut events = [None; 1];
        // Each level is held for 3 ms so it passes the 2 ms debounce.
        let mut level = true;
        let mut now = 0u64;
        for _ in 0..4 {
            level = !level;
            for _ in 0..4 {
                let t = p.update(level, now).is_some();
                if let Some(e) = m.update(t, now) {
                    events[0] = Some(e);
                }
                now += 1;
            }
        }
        assert_eq!(events[0], Some(BurstEvent::Burst));
        assert_eq!(m.update(false, now + 300), Some(BurstEvent::Quiet));
    }
}
