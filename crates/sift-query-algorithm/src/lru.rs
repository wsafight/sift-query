use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCapacity;

impl fmt::Display for InvalidCapacity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("capacity must be greater than zero")
    }
}

impl std::error::Error for InvalidCapacity {}

/// A bounded LRU with constant-time promotion and eviction, using safe indices.
/// `insert` returns whether an existing entry was evicted.
#[derive(Debug)]
pub struct BoundedLruMap<K, V> {
    capacity: usize,
    entries: HashMap<K, usize>,
    nodes: Vec<Option<Entry<K, V>>>,
    free: Vec<usize>,
    head: Option<usize>,
    tail: Option<usize>,
}

#[derive(Debug)]
struct Entry<K, V> {
    key: K,
    value: V,
    previous: Option<usize>,
    next: Option<usize>,
}

impl<K, V> BoundedLruMap<K, V>
where
    K: Clone + Eq + Hash,
{
    pub fn new(capacity: usize) -> Result<Self, InvalidCapacity> {
        if capacity == 0 {
            return Err(InvalidCapacity);
        }
        Ok(Self {
            capacity,
            entries: HashMap::with_capacity(capacity),
            nodes: Vec::new(),
            free: Vec::new(),
            head: None,
            tail: None,
        })
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let index = *self.entries.get(key)?;
        self.promote(index);
        Some(&self.entry(index).value)
    }

    /// Reads an entry without changing eviction order.
    pub fn peek(&self, key: &K) -> Option<&V> {
        let index = *self.entries.get(key)?;
        Some(&self.entry(index).value)
    }

    /// Checks validity before promotion with one hash lookup on live hits.
    pub(crate) fn get_validated(&mut self, key: &K, valid: impl FnOnce(&V) -> bool) -> Option<&V> {
        let index = *self.entries.get(key)?;
        if !valid(&self.entry(index).value) {
            self.remove(key);
            return None;
        }
        self.promote(index);
        Some(&self.entry(index).value)
    }

    /// Removes an entry and reuses its storage on a subsequent insertion.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index = self.entries.remove(key)?;
        self.detach(index);
        let entry = self.nodes[index].take().expect("LRU index is occupied");
        self.free.push(index);
        Some(entry.value)
    }

    /// Returns a hit only when this key is already most recently used.
    /// Reading it cannot change LRU order, so callers may use a shared lock.
    pub fn peek_mru(&self, key: &K) -> Option<&V> {
        let index = *self.entries.get(key)?;
        (self.tail == Some(index)).then(|| &self.entry(index).value)
    }

    /// Inserts or replaces a value. Returns `true` only when a different key
    /// had to be evicted to stay within capacity.
    pub fn insert(&mut self, key: K, value: V) -> bool {
        if let Some(index) = self.entries.get(&key).copied() {
            self.entry_mut(index).value = value;
            self.promote(index);
            return false;
        }

        let evicted = self.entries.len() == self.capacity;
        let index = if evicted {
            let index = self.head.expect("full LRU has a head");
            self.detach(index);
            let old = self.nodes[index].take().expect("LRU index is occupied");
            self.entries.remove(&old.key);
            index
        } else if let Some(index) = self.free.pop() {
            index
        } else {
            self.nodes.push(None);
            self.nodes.len() - 1
        };
        self.entries.insert(key.clone(), index);
        self.nodes[index] = Some(Entry {
            key,
            value,
            previous: None,
            next: None,
        });
        self.append(index);
        evicted
    }

    fn entry(&self, index: usize) -> &Entry<K, V> {
        self.nodes[index].as_ref().expect("LRU index is occupied")
    }

    fn entry_mut(&mut self, index: usize) -> &mut Entry<K, V> {
        self.nodes[index].as_mut().expect("LRU index is occupied")
    }

    fn detach(&mut self, index: usize) {
        let (previous, next) = (self.entry(index).previous, self.entry(index).next);
        if let Some(previous) = previous {
            self.entry_mut(previous).next = next;
        } else {
            self.head = next;
        }
        if let Some(next) = next {
            self.entry_mut(next).previous = previous;
        } else {
            self.tail = previous;
        }
    }

    fn append(&mut self, index: usize) {
        let previous = self.tail;
        self.entry_mut(index).previous = previous;
        self.entry_mut(index).next = None;
        if let Some(previous) = previous {
            self.entry_mut(previous).next = Some(index);
        } else {
            self.head = Some(index);
        }
        self.tail = Some(index);
    }

    fn promote(&mut self, index: usize) {
        if self.tail != Some(index) {
            self.detach(index);
            self.append(index);
        }
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.nodes.clear();
        self.free.clear();
        self.head = None;
        self.tail = None;
    }
}

#[cfg(test)]
mod tests {
    use super::BoundedLruMap;

    #[test]
    fn removed_slots_are_reused_without_growing_storage() {
        let mut cache = BoundedLruMap::new(3).unwrap();
        for key in 0..3 {
            cache.insert(key, key);
        }
        for key in 3..10_000 {
            assert!(cache.remove(&(key - 3)).is_some());
            assert!(!cache.insert(key, key));
            assert_eq!(cache.nodes.len(), 3);
            assert_eq!(cache.free, []);
        }
        cache.clear();
        assert!(cache.nodes.is_empty());
        assert_eq!(cache.free, []);
    }
}
