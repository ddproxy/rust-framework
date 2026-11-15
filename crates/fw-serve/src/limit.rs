use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};

/// Open connections per remote address, so one source cannot take every global slot.
pub struct AddrLimit {
    limit: usize,
    active: Mutex<HashMap<IpAddr, usize>>,
}

/// Holds one address slot. Dropping it, when the connection ends, frees the slot.
pub struct AddrSlot {
    limit: Arc<AddrLimit>,
    ip: IpAddr,
}

impl AddrLimit {
    pub fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit,
            active: Mutex::new(HashMap::new()),
        })
    }

    pub fn acquire(self: &Arc<Self>, ip: IpAddr) -> Option<AddrSlot> {
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let count = active.entry(ip).or_insert(0);
        if *count >= self.limit {
            return None;
        }
        *count += 1;
        Some(AddrSlot {
            limit: Arc::clone(self),
            ip,
        })
    }

    fn release(&self, ip: IpAddr) {
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = active.get_mut(&ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                active.remove(&ip);
            }
        }
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.active.lock().unwrap().len()
    }
}

impl Drop for AddrSlot {
    fn drop(&mut self) {
        self.limit.release(self.ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_counts_open_connections_and_frees_slots_on_drop() {
        let limit = AddrLimit::new(2);
        let a: IpAddr = "192.0.2.1".parse().unwrap();
        let b: IpAddr = "192.0.2.2".parse().unwrap();
        let first = limit.acquire(a).expect("first slot");
        let second = limit.acquire(a).expect("second slot");
        assert!(limit.acquire(a).is_none(), "the address limit is reached");
        let other = limit.acquire(b).expect("another address has its own slots");
        drop(second);
        let again = limit
            .acquire(a)
            .expect("a closed connection frees its slot");
        drop((first, again, other));
        assert_eq!(limit.tracked(), 0, "entries are removed at zero");
    }
}
