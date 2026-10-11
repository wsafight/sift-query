use std::hash::Hash;
use std::time::Duration;

use crate::{BoundedLruMap, InvalidCapacity};

/// A bounded, lazy-expiring LRU. The caller supplies monotonic timestamps as
/// durations from one fixed origin; this type owns no clock, locks, or runtime.
/// A hit promotes a live entry but never extends its lifetime. A lookup removes
/// an expired entry without promoting it. Other expired entries remain bounded
/// by capacity until looked up or evicted.
#[derive(Debug)]
pub struct TtlLruMap<K, V> {
    entries: BoundedLruMap<K, Timed<V>>,
    ttl: Duration,
}

#[derive(Debug)]
struct Timed<V> {
    created: Duration,
    value: V,
}

impl<K: Clone + Eq + Hash, V> TtlLruMap<K, V> {
    /// A zero TTL is allowed and makes every entry immediately expired.
    pub fn new(capacity: usize, ttl: Duration) -> Result<Self, InvalidCapacity> {
        Ok(Self {
            entries: BoundedLruMap::new(capacity)?,
            ttl,
        })
    }

    pub fn get(&mut self, key: &K, now: Duration) -> Option<&V> {
        self.entries
            .get_validated(key, |entry| {
                // A timestamp before insertion cannot establish freshness.
                now.checked_sub(entry.created)
                    .is_some_and(|age| age < self.ttl)
            })
            .map(|entry| &entry.value)
    }

    /// Returns whether a different key was evicted, as with `BoundedLruMap`.
    pub fn insert(&mut self, key: K, value: V, now: Duration) -> bool {
        self.entries.insert(
            key,
            Timed {
                created: now,
                value,
            },
        )
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.entries.remove(key).map(|entry| entry.value)
    }

    /// Number of retained entries, including entries not yet checked for expiry.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
