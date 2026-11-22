//! Saturating conversions for timestamps read from certificates or persisted state.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Time left until `deadline`, or zero when it has passed. A deadline that cannot be represented
/// is clamped to the platform's range instead of panicking.
///
/// # Examples
///
/// ```
/// use std::time::{Duration, UNIX_EPOCH};
/// use fw_serve::remaining;
///
/// let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
/// assert_eq!(remaining(now + Duration::from_secs(30), now), Duration::from_secs(30));
/// assert_eq!(remaining(now - Duration::from_secs(1), now), Duration::ZERO);
/// ```
pub fn remaining(deadline: SystemTime, now: SystemTime) -> Duration {
    deadline.duration_since(now).unwrap_or_default()
}

/// Signed unix seconds, clamped when the platform cannot hold them in an `i64`.
///
/// # Examples
///
/// ```
/// use std::time::UNIX_EPOCH;
/// use fw_serve::{from_unix_secs, unix_secs};
///
/// assert_eq!(unix_secs(UNIX_EPOCH), 0);
/// assert_eq!(unix_secs(from_unix_secs(1_700_000_000)), 1_700_000_000);
/// ```
pub fn unix_secs(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(delta) => i64::try_from(delta.as_secs()).unwrap_or(i64::MAX),
        Err(error) => i64::try_from(error.duration().as_secs())
            .map(|secs| secs.checked_neg().unwrap_or(i64::MIN))
            .unwrap_or(i64::MIN),
    }
}

/// A signed unix timestamp as a `SystemTime`, clamped to what the platform can represent.
///
/// # Examples
///
/// ```
/// use std::time::UNIX_EPOCH;
/// use fw_serve::from_unix_secs;
///
/// assert_eq!(from_unix_secs(0), UNIX_EPOCH);
/// assert!(from_unix_secs(-1) < UNIX_EPOCH);
/// ```
pub fn from_unix_secs(secs: i64) -> SystemTime {
    let magnitude = Duration::from_secs(secs.unsigned_abs());
    if secs < 0 {
        UNIX_EPOCH.checked_sub(magnitude).unwrap_or_else(floor)
    } else {
        UNIX_EPOCH.checked_add(magnitude).unwrap_or_else(ceiling)
    }
}

fn floor() -> SystemTime {
    let mut delta = Duration::from_secs(i64::MAX as u64);
    while UNIX_EPOCH.checked_sub(delta).is_none() {
        delta /= 2;
    }
    UNIX_EPOCH - delta
}

fn ceiling() -> SystemTime {
    let mut delta = Duration::from_secs(i64::MAX as u64);
    while UNIX_EPOCH.checked_add(delta).is_none() {
        delta /= 2;
    }
    UNIX_EPOCH + delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_is_zero_for_expired_equal_and_negative_looking_deadlines() {
        let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(remaining(now - Duration::from_secs(1), now), Duration::ZERO);
        assert_eq!(remaining(now, now), Duration::ZERO);
        assert_eq!(remaining(from_unix_secs(-1), now), Duration::ZERO);
        assert_eq!(remaining(from_unix_secs(0), UNIX_EPOCH), Duration::ZERO);
    }

    #[test]
    fn remaining_keeps_future_deadlines_including_huge_ones() {
        let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(
            remaining(now + Duration::from_secs(30), now),
            Duration::from_secs(30)
        );
        assert!(remaining(from_unix_secs(i64::MAX), now) > Duration::from_secs(30));
    }

    #[test]
    fn signed_timestamps_round_trip_and_saturate() {
        for secs in [0, 1, -1, 1_700_000_000, -1_700_000_000] {
            assert_eq!(unix_secs(from_unix_secs(secs)), secs);
        }
        assert!(unix_secs(from_unix_secs(i64::MAX)) > 0);
        assert!(unix_secs(from_unix_secs(i64::MIN)) < 0);
        assert_eq!(unix_secs(UNIX_EPOCH), 0);
    }

    #[test]
    fn extreme_timestamps_are_monotonic_and_never_panic() {
        let mut previous = from_unix_secs(i64::MIN);
        for secs in [
            i64::MIN + 1,
            -1_700_000_000,
            -1,
            0,
            1,
            1_700_000_000,
            i64::MAX - 1,
        ] {
            let time = from_unix_secs(secs);
            assert!(time >= previous, "{secs} moved the clock backwards");
            previous = time;
        }
        assert!(from_unix_secs(i64::MAX) >= previous);

        // Remaining time is zero at and before the deadline, and positive after it.
        let now = from_unix_secs(1_700_000_000);
        assert_eq!(remaining(from_unix_secs(i64::MIN), now), Duration::ZERO);
        assert_eq!(remaining(now, now), Duration::ZERO);
        assert!(remaining(from_unix_secs(i64::MAX), now) > Duration::ZERO);
    }

    #[test]
    fn very_large_durations_do_not_overflow_the_remaining_calculation() {
        let now = UNIX_EPOCH + Duration::from_secs(1);
        let deadline = now.checked_add(Duration::from_secs(i64::MAX as u64));
        if let Some(deadline) = deadline {
            assert_eq!(
                remaining(deadline, now),
                deadline.duration_since(now).unwrap()
            );
        }
        assert_eq!(remaining(UNIX_EPOCH, now), Duration::ZERO);
    }
}
