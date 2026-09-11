//! Timeout and test-group scheduling primitives for rust-mutant.
//!
//! This crate owns the pure adaptive timeout policy and the duration scaling
//! used for routed test groups. Mutation discovery, coverage routing, test
//! grouping, process execution, caching, and resource governance remain in
//! `rust-mutant-core`; core re-exports these primitives to preserve its public
//! surface.

use std::time::Duration;

/// Multiplier applied to a baseline duration when deriving an adaptive timeout.
pub const ADAPTIVE_TIMEOUT_MULTIPLIER: u128 = 3;

/// Minimum adaptive timeout, in milliseconds.
pub const ADAPTIVE_TIMEOUT_FLOOR_MS: u128 = 5_000;

/// Maximum adaptive timeout, in milliseconds.
///
/// The design record specifies a 300-second ceiling: without it, a slow
/// baseline multiplied by the routed-group test count could stall a campaign
/// for an unbounded wall clock.
pub const ADAPTIVE_TIMEOUT_CEILING_MS: u128 = 300_000;

/// Derive an adaptive timeout from a baseline duration in milliseconds.
///
/// The policy applies [`ADAPTIVE_TIMEOUT_MULTIPLIER`], adds the
/// [`ADAPTIVE_TIMEOUT_FLOOR_MS`] safety margin, and clamps the result to the
/// documented floor and [`ADAPTIVE_TIMEOUT_CEILING_MS`].
pub fn adaptive_timeout(baseline_ms: u128) -> Duration {
    let timeout_ms = baseline_ms
        .saturating_mul(ADAPTIVE_TIMEOUT_MULTIPLIER)
        .saturating_add(ADAPTIVE_TIMEOUT_FLOOR_MS)
        .clamp(ADAPTIVE_TIMEOUT_FLOOR_MS, ADAPTIVE_TIMEOUT_CEILING_MS);
    Duration::from_millis(timeout_ms as u64)
}

/// Scale a timeout for a routed group containing `test_count` tests.
///
/// Counts larger than `u32::MAX` use the largest multiplier supported by
/// [`Duration::saturating_mul`]. In adaptive mode the scaled result is capped
/// at [`ADAPTIVE_TIMEOUT_CEILING_MS`] so a wide group cannot exceed the
/// documented ceiling; an explicit `--timeout` value remains uncapped because
/// it is an intentional user choice.
pub fn group_timeout(timeout: Duration, test_count: usize, adaptive: bool) -> Duration {
    let scaled = timeout.saturating_mul(u32::try_from(test_count).unwrap_or(u32::MAX));
    if adaptive {
        scaled.min(Duration::from_millis(ADAPTIVE_TIMEOUT_CEILING_MS as u64))
    } else {
        scaled
    }
}

#[cfg(test)]
mod tests {
    use super::{ADAPTIVE_TIMEOUT_CEILING_MS, adaptive_timeout, group_timeout};
    use std::time::Duration;

    #[test]
    fn adaptive_timeout_applies_floor_and_baseline_multiplier() {
        assert_eq!(adaptive_timeout(0), Duration::from_secs(5));
        assert_eq!(adaptive_timeout(1_000), Duration::from_millis(8_000));
        assert_eq!(adaptive_timeout(10_000), Duration::from_millis(35_000));
    }

    #[test]
    fn adaptive_timeout_respects_the_documented_ceiling() {
        assert_eq!(adaptive_timeout(200_000), Duration::from_secs(300));
        assert_eq!(
            adaptive_timeout(u128::MAX),
            Duration::from_millis(ADAPTIVE_TIMEOUT_CEILING_MS as u64)
        );
    }

    #[test]
    fn group_timeout_handles_zero_and_scales_by_test_count() {
        assert_eq!(
            group_timeout(Duration::from_secs(2), 0, false),
            Duration::ZERO
        );
        assert_eq!(
            group_timeout(Duration::from_millis(250), 3, false),
            Duration::from_millis(750)
        );
    }

    #[test]
    fn group_timeout_clamps_huge_counts_and_saturates_duration() {
        assert_eq!(
            group_timeout(Duration::from_secs(2), usize::MAX, false),
            Duration::from_secs(2).saturating_mul(u32::MAX)
        );
        assert_eq!(
            group_timeout(Duration::MAX, usize::MAX, false),
            Duration::MAX
        );
    }

    #[test]
    fn adaptive_group_timeout_is_capped_at_the_ceiling() {
        assert_eq!(
            group_timeout(Duration::from_secs(200), 2, true),
            Duration::from_secs(300)
        );
        assert_eq!(
            group_timeout(Duration::from_secs(200), 100, true),
            Duration::from_secs(300)
        );
    }
}
