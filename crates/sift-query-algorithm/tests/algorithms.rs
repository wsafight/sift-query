use sift_query_algorithm::{BoundedLruMap, FNV1A_64_OFFSET_BASIS, StableHasher64};

#[test]
fn bounded_lru_evicts_the_least_recently_used_entry() {
    let mut cache = BoundedLruMap::new(2).unwrap();
    assert!(!cache.insert("a", 1));
    assert!(!cache.insert("b", 2));
    assert_eq!(cache.get(&"a"), Some(&1));
    assert!(cache.insert("c", 3));
    assert_eq!(cache.get(&"a"), Some(&1));
    assert_eq!(cache.get(&"b"), None);
    assert_eq!(cache.get(&"c"), Some(&3));
    assert_eq!(cache.len(), 2);
}

#[test]
fn bounded_lru_rejects_zero_capacity_and_can_be_cleared() {
    assert!(BoundedLruMap::<u8, u8>::new(0).is_err());
    let mut cache = BoundedLruMap::new(1).unwrap();
    cache.insert(1, 2);
    cache.clear();
    assert!(cache.is_empty());
    assert_eq!(cache.capacity(), 1);
}

#[test]
fn stable_hash_is_repeatable_and_delimiters_disambiguate_parts() {
    let mut first = StableHasher64::default();
    first.write_delimited("ab");
    first.write_delimited("c");

    let mut second = StableHasher64::new(FNV1A_64_OFFSET_BASIS);
    second.write_delimited("ab");
    second.write_delimited("c");

    let mut differently_partitioned = StableHasher64::default();
    differently_partitioned.write_delimited("a");
    differently_partitioned.write_delimited("bc");

    assert_eq!(first.finish(), second.finish());
    assert_ne!(first.finish(), differently_partitioned.finish());

    let mut standard_vector = StableHasher64::default();
    standard_vector.write("hello");
    assert_eq!(standard_vector.finish(), 0xa430_d846_80aa_bd0b);
}

use sift_query_algorithm::InvalidCapacity;

#[test]
fn invalid_capacity_is_a_displayable_error() {
    let error = BoundedLruMap::<u8, u8>::new(0).unwrap_err();
    assert_eq!(error, InvalidCapacity);
    assert_eq!(error.to_string(), "capacity must be greater than zero");
    let as_error: &dyn std::error::Error = &error;
    assert!(as_error.source().is_none());
    assert_eq!(format!("{error:?}"), "InvalidCapacity");
}

#[test]
fn replacing_an_existing_key_does_not_evict() {
    let mut cache = BoundedLruMap::new(2).unwrap();
    assert!(!cache.insert("a", 1));
    assert!(!cache.insert("b", 2));
    // Replacing a present key is not an eviction.
    assert!(!cache.insert("a", 9));
    assert_eq!(cache.get(&"a"), Some(&9));
    assert_eq!(cache.get(&"b"), Some(&2));
    assert_eq!(cache.len(), 2);
    assert!(!cache.is_empty());
}

#[test]
fn eviction_prefers_the_least_recently_used_entry() {
    let mut cache = BoundedLruMap::new(3).unwrap();
    cache.insert("a", 1);
    cache.insert("b", 2);
    cache.insert("c", 3);
    // Touch `a` so `b` becomes the oldest.
    assert_eq!(cache.get(&"a"), Some(&1));
    assert!(cache.insert("d", 4));
    assert_eq!(cache.get(&"b"), None);
    assert_eq!(cache.get(&"a"), Some(&1));
    assert_eq!(cache.get(&"c"), Some(&3));
    assert_eq!(cache.get(&"d"), Some(&4));
    assert_eq!(cache.len(), 3);
}

#[test]
fn capacity_one_keeps_only_the_newest_entry() {
    let mut cache = BoundedLruMap::new(1).unwrap();
    assert!(!cache.insert("a", 1));
    assert!(cache.insert("b", 2));
    assert_eq!(cache.get(&"a"), None);
    assert_eq!(cache.get(&"b"), Some(&2));
    assert_eq!(cache.capacity(), 1);
}

#[test]
fn replacement_churn_and_clear_preserve_lru_order() {
    let mut cache = BoundedLruMap::new(3).unwrap();
    for value in 0..100 {
        cache.insert(value, value);
        assert_eq!(cache.get(&value), Some(&value));
        assert!(cache.len() <= 3);
    }
    assert!(!cache.insert(97, 1000));
    assert!(cache.insert(100, 100));
    assert_eq!(cache.get(&98), None);
    assert_eq!(cache.get(&97), Some(&1000));
    cache.clear();
    for value in 0..3 {
        assert!(!cache.insert(value, value));
    }
    assert!(cache.insert(3, 3));
    assert_eq!(cache.get(&0), None);
}

#[test]
fn shared_mru_reads_do_not_change_eviction_order() {
    let mut cache = BoundedLruMap::new(2).unwrap();
    cache.insert("a", 1);
    cache.insert("b", 2);
    assert_eq!(cache.peek_mru(&"a"), None);
    assert_eq!(cache.peek_mru(&"b"), Some(&2));
    cache.get(&"a");
    assert_eq!(cache.peek_mru(&"a"), Some(&1));
    cache.insert("c", 3);
    assert_eq!(cache.get(&"b"), None);
}

#[test]
fn cache_debug_reports_its_capacity() {
    let cache = BoundedLruMap::<u8, u8>::new(4).unwrap();
    let rendered = format!("{cache:?}");
    assert!(rendered.contains("capacity: 4"), "{rendered}");
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
}

#[test]
fn stable_hash_writes_raw_bytes_without_delimiters() {
    let mut hash = StableHasher64::default();
    hash.write([]);
    assert_eq!(hash.finish(), FNV1A_64_OFFSET_BASIS);

    let mut single = StableHasher64::default();
    single.write([0x00]);
    assert_ne!(single.finish(), FNV1A_64_OFFSET_BASIS);

    // Writing byte-by-byte matches writing the whole slice at once.
    let mut chunked = StableHasher64::default();
    chunked.write([1u8, 2]);
    chunked.write([3u8]);
    let mut whole = StableHasher64::default();
    whole.write([1u8, 2, 3]);
    assert_eq!(chunked.finish(), whole.finish());

    // A delimiter changes the result even for an empty write.
    let mut delimited = StableHasher64::default();
    delimited.write_delimited([]);
    assert_ne!(delimited.finish(), FNV1A_64_OFFSET_BASIS);
}

#[test]
fn stable_hash_seeds_are_independent() {
    let mut first = StableHasher64::new(1);
    first.write("x");
    let mut second = StableHasher64::new(2);
    second.write("x");
    assert_ne!(first.finish(), second.finish());

    // The hasher is copyable and finishes without consuming the caller's copy.
    let hasher = StableHasher64::default();
    let copied = hasher;
    assert_eq!(hasher.finish(), copied.finish());
    assert_eq!(format!("{hasher:?}"), format!("{copied:?}"));
}
