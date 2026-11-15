use std::collections::HashMap;
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

/// Counts failed token checks per address. A blocked address gets the time left in its window.
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

    /// How long `ip` must wait, or `None` when it may try.
    pub fn blocked(&self, ip: IpAddr, now: Instant) -> Option<Duration> {
        let ip = throttle_key(ip);
        let seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        let (count, since) = seen.get(&ip)?;
        let elapsed = now.saturating_duration_since(*since);
        if elapsed >= self.settings.window {
            return None;
        }
        (*count >= self.settings.max_failures).then(|| self.settings.window - elapsed)
    }

    /// Records a failure and returns how many the address has inside the current window.
    pub fn fail(&self, ip: IpAddr, now: Instant) -> u32 {
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
        if now.saturating_duration_since(entry.1) >= window {
            *entry = (0, now);
        }
        entry.0 += 1;
        entry.0
    }

    /// A good login clears the address's count.
    pub fn succeed(&self, ip: IpAddr) {
        let ip = throttle_key(ip);
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&ip);
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(throttle.fail(ip, t0), 1);
        assert_eq!(throttle.fail(ip, t0), 2);
        assert!(
            throttle.blocked(ip, t0).is_none(),
            "two failures are allowed"
        );
        assert_eq!(throttle.fail(ip, t0), 3);
        let wait = throttle
            .blocked(ip, t0 + Duration::from_secs(20))
            .expect("blocked");
        assert_eq!(wait, Duration::from_secs(40));
    }

    #[test]
    fn the_block_lifts_when_the_window_passes() {
        let throttle = throttle();
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        let t0 = Instant::now();
        for _ in 0..3 {
            throttle.fail(ip, t0);
        }
        assert!(throttle.blocked(ip, t0 + Duration::from_secs(60)).is_none());
        assert_eq!(
            throttle.fail(ip, t0 + Duration::from_secs(61)),
            1,
            "the count restarts"
        );
    }

    #[test]
    fn a_good_login_clears_the_count() {
        let throttle = throttle();
        let ip: IpAddr = "192.0.2.7".parse().unwrap();
        let t0 = Instant::now();
        for _ in 0..3 {
            throttle.fail(ip, t0);
        }
        throttle.succeed(ip);
        assert!(throttle.blocked(ip, t0).is_none());
    }

    #[test]
    fn addresses_are_counted_separately_and_ipv6_by_slash_64() {
        let throttle = throttle();
        let t0 = Instant::now();
        let a: IpAddr = "2001:db8:1:2::1".parse().unwrap();
        let same_prefix: IpAddr = "2001:db8:1:2:ffff::9".parse().unwrap();
        let other: IpAddr = "192.0.2.1".parse().unwrap();
        for _ in 0..2 {
            throttle.fail(a, t0);
        }
        throttle.fail(same_prefix, t0);
        assert!(
            throttle.blocked(a, t0).is_some(),
            "the /64 shares one count"
        );
        assert!(throttle.blocked(other, t0).is_none());
    }

    #[test]
    fn the_table_stays_bounded_under_an_address_flood() {
        let throttle = throttle();
        let t0 = Instant::now();
        for i in 0..(MAX_TRACKED as u32 + 500) {
            let ip = IpAddr::from(std::net::Ipv4Addr::from(0x0a00_0000 + i));
            throttle.fail(ip, t0);
        }
        assert_eq!(throttle.tracked(), MAX_TRACKED);
    }
}
