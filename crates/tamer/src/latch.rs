//! Report-once latch — [`OnceLatch`].
//!
//! A one-bit latch for suppressing repeated reports of a persistent
//! condition: [`trip`](OnceLatch::trip) answers `true` only for the first
//! call of a run, and [`rearm`](OnceLatch::rearm) answers `true` only when it
//! ends a run, so the next run is reported again.
//!
//! # Example
//!
//! Report a failing sensor read once, and its recovery once.
//!
//! ```
//! use tamer::latch::OnceLatch;
//!
//! let mut failed = OnceLatch::new();
//! let reads = [Ok(1), Err(()), Err(()), Ok(2), Err(())];
//! let mut reports = 0;
//! for read in reads {
//!     match read {
//!         Ok(_) => {
//!             if failed.rearm() {
//!                 reports += 1; // "recovered"
//!             }
//!         }
//!         Err(()) => {
//!             if failed.trip() {
//!                 reports += 1; // "failed" (first of the run only)
//!             }
//!         }
//!     }
//! }
//! assert_eq!(reports, 3);
//! ```

/// A latch that reports the first event of a run and the end of that run.
///
/// The latch starts disarmed (no run in progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OnceLatch {
    tripped: bool,
}

impl OnceLatch {
    /// Creates a latch with no run in progress.
    #[must_use]
    pub const fn new() -> Self {
        Self { tripped: false }
    }

    /// Records an occurrence of the condition.
    ///
    /// Returns `true` only if this starts a new run, that is, the latch was
    /// not already tripped.
    #[must_use]
    pub fn trip(&mut self) -> bool {
        let first = !self.tripped;
        self.tripped = true;
        first
    }

    /// Records that the condition has cleared.
    ///
    /// Returns `true` only if a run was in progress, which also re-arms the
    /// latch so the next [`trip`](Self::trip) reports again.
    #[must_use]
    pub fn rearm(&mut self) -> bool {
        let was = self.tripped;
        self.tripped = false;
        was
    }

    /// Returns `true` while a run is in progress.
    #[must_use]
    pub const fn is_tripped(&self) -> bool {
        self.tripped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_trip_reports_repeats_do_not() {
        let mut l = OnceLatch::new();
        assert!(!l.is_tripped());
        assert!(l.trip());
        assert!(!l.trip());
        assert!(!l.trip());
        assert!(l.is_tripped());
    }

    #[test]
    fn rearm_without_trip_is_silent() {
        let mut l = OnceLatch::new();
        assert!(!l.rearm());
        assert!(!l.is_tripped());
    }

    #[test]
    fn rearm_reports_once_then_trip_reports_again() {
        let mut l = OnceLatch::new();
        assert!(l.trip());
        assert!(l.rearm());
        assert!(!l.rearm());
        assert!(l.trip());
        assert!(!l.trip());
    }
}
