//! Per-address attempt throttle. Checking the limit and reserving an attempt share one lock.

use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Addresses tracked at once. Past this, an expired entry is evicted first, so a flood of
/// addresses cannot grow the table without bound.
const MAX_TRACKED: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThrottleSettings {
    pub max_failures: u32,
    pub window: Duration,
}

/// How long an address must wait before it may try again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retry {
    pub after: Duration,
}

impl fmt::Display for Retry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "retry in {}s", self.after.as_secs())
    }
}

/// Counts attempts per address. Checking the limit and reserving an attempt happen under one
/// lock, so a concurrent burst cannot pass the check before the first failure is recorded.
pub struct LoginThrottle {
    settings: ThrottleSettings,
    seen: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

/// An IPv4 address is its own key. An IPv6 address is keyed by its /64, so a host that rotates
/// through its prefix shares one count. A mapped IPv4 address counts as the IPv4 address.
fn throttle_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return IpAddr::V4(v4);
            }
            let mut octets = v6.octets();
            octets[8..].fill(0);
            IpAddr::V6(octets.into())
        }
    }
}

impl LoginThrottle {
    pub fn new(settings: ThrottleSettings) -> Self {
        Self {
            settings,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Reserves one attempt for `ip`, or returns the time left in its window when it is blocked.
    /// A successful login still consumes an attempt: there is deliberately no success reset.
    pub fn try_attempt(&self, ip: IpAddr, now: Instant) -> Result<(), Retry> {
        let ip = throttle_key(ip);
        let window = self.settings.window;
        let max = self.settings.max_failures;
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        if seen.len() >= MAX_TRACKED && !seen.contains_key(&ip) {
            // Evict the least harmful entry: expired first, then unblocked, then least failed.
            let victim = seen
                .iter()
                .min_by_key(|(_, (count, since))| {
                    let expired = now.saturating_duration_since(*since) >= window;
                    (!expired, *count >= max, *count, *since)
                })
                .map(|(addr, _)| *addr);
            if let Some(victim) = victim {
                seen.remove(&victim);
            }
        }
        let entry = seen.entry(ip).or_insert((0, now));
        let elapsed = now.saturating_duration_since(entry.1);
        if elapsed >= window {
            *entry = (0, now);
        }
        if entry.0 >= max {
            return Err(Retry {
                after: window - now.saturating_duration_since(entry.1),
            });
        }
        entry.0 = entry.0.saturating_add(1);
        Ok(())
    }

    // There is deliberately no success reset: see the test of the same intent below.
    #[cfg(test)]
    fn count(&self, ip: IpAddr) -> Option<u32> {
        self.seen
            .lock()
            .unwrap()
            .get(&throttle_key(ip))
            .map(|(count, _)| *count)
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;

    fn throttle() -> LoginThrottle {
        LoginThrottle::new(ThrottleSettings {
            max_failures: 3,
            window: Duration::from_secs(60),
        })
    }

    #[test]
    fn an_address_is_blocked_after_max_failures_with_the_time_left() {
        let throttle = throttle();
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        let t0 = Instant::now();
        for _ in 0..3 {
            assert_eq!(throttle.try_attempt(ip, t0), Ok(()));
        }
        assert_eq!(throttle.count(ip), Some(3));
        assert_eq!(
            throttle.try_attempt(ip, t0 + Duration::from_secs(20)),
            Err(Retry {
                after: Duration::from_secs(40)
            })
        );
    }

    #[test]
    fn the_block_lifts_when_the_window_passes() {
        let throttle = throttle();
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        let t0 = Instant::now();
        for _ in 0..3 {
            throttle.try_attempt(ip, t0).unwrap();
        }
        let later = t0 + Duration::from_secs(60);
        assert_eq!(throttle.try_attempt(ip, later), Ok(()));
        assert_eq!(throttle.count(ip), Some(1), "the count restarts");
    }

    #[test]
    fn a_block_lasts_its_full_window_with_no_success_reset() {
        // A caller holding one valid credential must not be able to reset the count between
        // guesses at another, so the throttle has no success reset.
        let throttle = throttle();
        let ip: IpAddr = "192.0.2.7".parse().unwrap();
        let t0 = Instant::now();
        for _ in 0..10 {
            let _ = throttle.try_attempt(ip, t0);
        }
        assert!(throttle.try_attempt(ip, t0).is_err());
        assert!(throttle
            .try_attempt(ip, t0 + Duration::from_secs(30))
            .is_err());
        assert!(throttle
            .try_attempt(ip, t0 + Duration::from_secs(61))
            .is_ok());
    }

    #[test]
    fn a_count_at_the_maximum_saturates_instead_of_overflowing() {
        let throttle = LoginThrottle::new(ThrottleSettings {
            max_failures: u32::MAX,
            window: Duration::from_secs(60),
        });
        let ip: IpAddr = "192.0.2.8".parse().unwrap();
        let t0 = Instant::now();
        throttle.seen.lock().unwrap().insert(ip, (u32::MAX - 1, t0));
        assert_eq!(throttle.try_attempt(ip, t0), Ok(()));
        assert_eq!(throttle.count(ip), Some(u32::MAX));
        assert!(throttle.try_attempt(ip, t0).is_err());
    }

    #[test]
    fn addresses_are_counted_separately_and_ipv6_by_slash_64() {
        let throttle = throttle();
        let t0 = Instant::now();
        let a: IpAddr = "2001:db8:1:2::1".parse().unwrap();
        let same_prefix: IpAddr = "2001:db8:1:2:ffff::9".parse().unwrap();
        let other: IpAddr = "192.0.2.1".parse().unwrap();
        for _ in 0..2 {
            throttle.try_attempt(a, t0).unwrap();
        }
        throttle.try_attempt(same_prefix, t0).unwrap();
        assert!(
            throttle.try_attempt(a, t0).is_err(),
            "the /64 shares one count"
        );
        assert_eq!(throttle.try_attempt(other, t0), Ok(()));
    }

    #[test]
    fn the_table_stays_bounded_under_an_address_flood() {
        let throttle = throttle();
        let t0 = Instant::now();
        for i in 0..(MAX_TRACKED as u32 + 500) {
            let ip = IpAddr::from(std::net::Ipv4Addr::from(0x0a00_0000 + i));
            throttle.try_attempt(ip, t0).unwrap();
        }
        assert_eq!(throttle.tracked(), MAX_TRACKED);
    }

    #[test]
    fn concurrent_attempts_reserve_the_limit_exactly_once_each() {
        const THREADS: usize = 32;
        const ATTEMPTS: usize = 32;
        const MAX: usize = 20;
        let throttle = LoginThrottle::new(ThrottleSettings {
            max_failures: MAX as u32,
            window: Duration::from_secs(60),
        });
        let ip: IpAddr = "192.0.2.9".parse().unwrap();
        let now = Instant::now();
        let barrier = Barrier::new(THREADS);
        let allowed = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    barrier.wait();
                    for _ in 0..ATTEMPTS {
                        if throttle.try_attempt(ip, now).is_ok() {
                            allowed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        assert_eq!(allowed.load(Ordering::Relaxed), MAX);
        assert_eq!(throttle.count(ip), Some(MAX as u32));
    }
}
