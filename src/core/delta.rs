// Delta-over-elapsed tracking shared by the rate-based monitors (network
// byte counters, powercap energy counters). Callers interpret
// DeltaSample::First/Invalid differently (zero rates vs skipped
// accumulation), which matches their pre-refactor behavior exactly.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeltaSample {
    /// No previous record for this key.
    First,
    /// Elapsed time <= 0 or the counter rolled back; the caller decides
    /// whether that means "zero rate" (network) or "skip" (power).
    Invalid,
    /// Counter delta (monotonic increase: value > previous) and elapsed
    /// seconds, computed for the caller to scale.
    Changed(u64, f64),
}

pub struct DeltaTracker<K> {
    entries: HashMap<K, (u64, Instant)>,
}

impl<K: Hash + Eq> DeltaTracker<K> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn record(&mut self, key: K, value: u64, now: Instant) -> DeltaSample {
        let Some((previous_value, previous_time)) = self.entries.insert(key, (value, now)) else {
            return DeltaSample::First;
        };

        let elapsed = now
            .checked_duration_since(previous_time)
            .map(|elapsed| elapsed.as_secs_f64())
            .unwrap_or(0.0);

        let Some(elapsed) = (elapsed > 0.0).then_some(elapsed) else {
            return DeltaSample::Invalid;
        };
        if value < previous_value {
            return DeltaSample::Invalid;
        }

        DeltaSample::Changed(value.saturating_sub(previous_value), elapsed)
    }
}

impl<K: Hash + Eq> Default for DeltaTracker<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // Characterization tests: pin the tracker semantics the two monitors
    // relied on. See docs/baseline-history.md (formerly BASELINE.md). Network treats First and Invalid as zero
    // rate; CPU treats both as skip.

    #[test]
    fn first_record_reports_first() {
        let mut tracker = DeltaTracker::new();
        let now = Instant::now();
        assert_eq!(tracker.record("k", 100, now), DeltaSample::First);
        assert_eq!(
            tracker.record("k", 200, now + Duration::from_secs(1)),
            DeltaSample::Changed(100, 1.0)
        );
    }

    #[test]
    fn rolled_back_counter_is_invalid() {
        let mut tracker = DeltaTracker::new();
        let now = Instant::now();
        tracker.record("k", 200, now);
        // Backwards counter (ops after reboot/reset): Invalid, not negative.
        assert_eq!(
            tracker.record("k", 100, now + Duration::from_secs(1)),
            DeltaSample::Invalid
        );
        // It still overwrites the stored value so the next tick can resume.
        assert_eq!(
            tracker.record("k", 250, now + Duration::from_secs(2)),
            DeltaSample::Changed(150, 1.0)
        );
    }

    #[test]
    fn no_elapsed_time_is_invalid() {
        let mut tracker = DeltaTracker::new();
        let now = Instant::now();
        tracker.record("k", 100, now);
        // Same instant: elapsed == 0 -> Invalid.
        assert_eq!(tracker.record("k", 150, now), DeltaSample::Invalid);
    }

    #[test]
    fn rollback_after_extreme_values_is_invalid() {
        let mut tracker = DeltaTracker::new();
        let now = Instant::now();
        tracker.record("k", u64::MAX, now);
        // Reboot-style reset: value < previous -> Invalid, never a negative
        // or wrapping delta.
        assert_eq!(
            tracker.record("k", 500_000, now + Duration::from_secs(3)),
            DeltaSample::Invalid
        );
    }

    #[test]
    fn zero_delta_is_changed_not_invalid() {
        // Same value on two ticks is a real sample with delta 0, matching
        // the pre-refactor `saturating_sub` behavior: accumulate 0 and still
        // set `found`.
        let mut tracker: DeltaTracker<i32> = DeltaTracker::new();
        let now = Instant::now();
        tracker.record(1, 42, now);
        assert_eq!(
            tracker.record(1, 42, now + Duration::from_secs(1)),
            DeltaSample::Changed(0, 1.0)
        );
    }

    #[test]
    fn keys_are_independent() {
        let mut tracker = DeltaTracker::new();
        let now = Instant::now();
        assert_eq!(tracker.record("a", 1, now), DeltaSample::First);
        assert_eq!(tracker.record("b", 9, now), DeltaSample::First);
        assert_eq!(
            tracker.record("a", 4, now + Duration::from_secs(2)),
            DeltaSample::Changed(3, 2.0)
        );
        assert_eq!(
            tracker.record("b", 9, now + Duration::from_secs(2)),
            DeltaSample::Changed(0, 2.0)
        );
    }
}
