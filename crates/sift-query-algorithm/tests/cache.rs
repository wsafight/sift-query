use sift_query_algorithm::{BoundedLruMap, TtlLruMap};
use std::time::Duration;

#[test]
fn mixed_operations_match_a_reference_lru() {
    for capacity in [1, 2, 7] {
        let mut actual = BoundedLruMap::new(capacity).unwrap();
        let mut expected = Vec::<(u64, u64)>::new();
        let mut random = 0x1234_5678_u64;
        for step in 0..10_000 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let key = random % 11;
            let position = expected.iter().position(|(k, _)| *k == key);
            match random % 5 {
                0 => {
                    let evicted = if let Some(index) = position {
                        expected.remove(index);
                        false
                    } else if expected.len() == capacity {
                        expected.remove(0);
                        true
                    } else {
                        false
                    };
                    expected.push((key, step));
                    assert_eq!(actual.insert(key, step), evicted);
                }
                1 => {
                    let value = position.map(|index| {
                        let entry = expected.remove(index);
                        expected.push(entry);
                        entry.1
                    });
                    assert_eq!(actual.get(&key).copied(), value);
                }
                2 => {
                    let value = position.map(|index| expected.remove(index).1);
                    assert_eq!(actual.remove(&key), value);
                }
                3 => {
                    assert_eq!(actual.peek(&key).copied(), position.map(|i| expected[i].1));
                }
                _ if random.is_multiple_of(97) => {
                    actual.clear();
                    expected.clear();
                }
                _ => {}
            }
            assert_eq!(actual.len(), expected.len());
            assert_eq!(actual.is_empty(), expected.is_empty());
            if let Some((key, value)) = expected.last() {
                assert_eq!(actual.peek_mru(key), Some(value));
            }
        }
    }
}

#[test]
fn ttl_hits_do_not_refresh_lifetime_and_expiry_removes_entries() {
    let mut cache = TtlLruMap::new(2, Duration::from_secs(10)).unwrap();
    cache.insert("a", 1, Duration::ZERO);
    cache.insert("b", 2, Duration::from_secs(1));
    assert_eq!(cache.get(&"a", Duration::from_secs(9)), Some(&1));
    assert_eq!(cache.get(&"a", Duration::from_secs(10)), None);
    assert_eq!(cache.len(), 1);
    assert!(!cache.insert("c", 3, Duration::from_secs(10)));
    assert_eq!(cache.get(&"b", Duration::from_secs(10)), Some(&2));
    assert!(cache.insert("d", 4, Duration::from_secs(10)));
    assert_eq!(cache.get(&"c", Duration::from_secs(10)), None);
    assert_eq!(cache.capacity(), 2);
}

#[test]
fn replacement_remove_clear_and_zero_ttl_obey_the_contract() {
    let mut cache = TtlLruMap::new(1, Duration::from_secs(5)).unwrap();
    cache.insert(1, "old", Duration::ZERO);
    assert!(!cache.insert(1, "new", Duration::from_secs(4)));
    assert_eq!(cache.get(&1, Duration::from_secs(5)), Some(&"new"));
    assert_eq!(cache.remove(&1), Some("new"));
    assert!(cache.is_empty());
    cache.insert(2, "clock", Duration::from_secs(3));
    assert_eq!(cache.get(&2, Duration::from_secs(2)), None);
    cache.insert(3, "clear", Duration::from_secs(3));
    cache.clear();
    assert!(cache.is_empty());
    let mut zero = TtlLruMap::new(1, Duration::ZERO).unwrap();
    zero.insert(1, 2, Duration::ZERO);
    assert_eq!(zero.get(&1, Duration::ZERO), None);
    assert!(TtlLruMap::<u8, u8>::new(0, Duration::ZERO).is_err());
}
