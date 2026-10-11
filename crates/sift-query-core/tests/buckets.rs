use sift_query_core::{
    BucketRequest, FieldId, FieldRef, NumericBucketRequest, ScalarKind, TimeBucketRequest,
    TimeBucketUnit,
};

mod support;
use support::RichEntity;

fn id() -> FieldRef<RichEntity, i64, ScalarKind> {
    FieldRef::new(FieldId(0))
}

#[test]
fn numeric_buckets_require_sorted_matching_boundaries() {
    let request = NumericBucketRequest::new(id(), [0_i64, 10_i64]).unwrap();
    assert_eq!(request.boundaries().len(), 2);
    let _ = BucketRequest::Numeric(request);
    assert!(NumericBucketRequest::new(id(), [10_i64, 0_i64]).is_err());
    assert!(NumericBucketRequest::new(id(), [0_i32, 1_i32]).is_err());
}

#[test]
fn time_bucket_timezones_are_server_validated() {
    let request = TimeBucketRequest::new(id(), TimeBucketUnit::Day, "UTC");
    assert!(request.is_ok());
    assert!(TimeBucketRequest::new(id(), TimeBucketUnit::Day, "UTC'; DROP TABLE users").is_err());
}
