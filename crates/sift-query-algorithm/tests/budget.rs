use sift_query_algorithm::BoundedCounter;

#[test]
fn inclusive_limit_and_failed_additions_preserve_state() {
    let mut counter = BoundedCounter::new(5);
    counter.try_add(2).unwrap();
    counter.try_add(3).unwrap();
    assert_eq!(counter.used(), 5);
    assert_eq!(counter.remaining(), 0);
    assert!(counter.try_add(1).is_err());
    assert_eq!(counter.used(), 5);
    counter.try_add(0).unwrap();
    assert_eq!(counter.limit(), 5);
}

#[test]
fn overflow_and_zero_limit_are_checked() {
    let mut counter = BoundedCounter::new(usize::MAX);
    counter.try_add(usize::MAX).unwrap();
    assert!(counter.try_add(1).is_err());
    assert_eq!(counter.used(), usize::MAX);
    let mut zero = BoundedCounter::new(0);
    zero.try_add(0).unwrap();
    assert!(zero.try_add(1).is_err());
}
