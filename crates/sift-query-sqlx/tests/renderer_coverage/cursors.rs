use super::support::*;

// ---------------------------------------------------------------------------
// Cursors
// ---------------------------------------------------------------------------

#[test]
fn multi_key_cursors_render_keyset_predicates_for_every_key() {
    let codec = sift_query_core::CursorCodec::new([0x91; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&RawParams::parse("sort=name&order=desc&cursor=&size=5").unwrap())
        .unwrap();
    let token = codec
        .encode::<User>(
            base.order(),
            &[
                sift_query_core::CursorValue::Scalar(ScalarValue::Text("M".to_owned())),
                sift_query_core::CursorValue::Scalar(ScalarValue::I64(10)),
            ],
        )
        .unwrap();
    let raw = RawParams::parse(&format!("sort=name&order=desc&cursor={token}&size=5")).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let built =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    // `name` is ordered descending with NULLS FIRST, so the comparison is `<`.
    assert!(rendered.sql.contains("u.name < $1"), "{}", rendered.sql);
    assert!(
        rendered.sql.contains(
            "((u.name < $1) OR (u.name IS NOT DISTINCT FROM $2 AND (u.id > $3 OR u.id IS NULL)))"
        ),
        "{}",
        rendered.sql
    );
    assert!(!rendered.sql.contains("OFFSET"));
}

#[test]
fn null_cursor_keys_render_null_aware_predicates() {
    let codec = sift_query_core::CursorCodec::new([0x92; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&RawParams::parse("cursor=&size=5").unwrap())
        .unwrap();
    let token = codec
        .encode::<User>(base.order(), &[sift_query_core::CursorValue::Null])
        .unwrap();
    let raw = RawParams::parse(&format!("cursor={token}&size=5")).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let built =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    // `id` is ordered ascending with NULLS LAST, so a NULL cursor key means
    // "everything after the nulls", which is nothing.
    assert!(rendered.sql.contains("WHERE u.deleted = false AND 1 = 0"));
}

#[test]
fn a_cursor_with_a_null_leading_key_still_advances_the_remaining_keys() {
    let codec = sift_query_core::CursorCodec::new([0x93; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&RawParams::parse("sort=name&order=asc&cursor=&size=5").unwrap())
        .unwrap();
    let token = codec
        .encode::<User>(
            base.order(),
            &[
                sift_query_core::CursorValue::Null,
                sift_query_core::CursorValue::Scalar(ScalarValue::I64(10)),
            ],
        )
        .unwrap();
    let raw = RawParams::parse(&format!("sort=name&order=asc&cursor={token}&size=5")).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let built =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    // Ascending with NULLS LAST: a null `name` is the last bucket, so the
    // second key continues within that bucket.
    assert!(
        rendered
            .sql
            .contains("(u.name IS NULL AND (u.id > $1 OR u.id IS NULL))"),
        "{}",
        rendered.sql
    );
}

#[test]
fn a_cursor_value_count_mismatch_is_reported_by_the_renderer() {
    let codec = sift_query_core::CursorCodec::new([0x94; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&RawParams::parse("cursor=&size=5").unwrap())
        .unwrap();
    let token = codec
        .encode::<User>(
            base.order(),
            &[sift_query_core::CursorValue::Scalar(ScalarValue::I64(1))],
        )
        .unwrap();
    let raw = RawParams::parse(&format!("cursor={token}&size=5")).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let mut built =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    // Simulate an ordering that grew after the cursor was minted.
    built.order.push(sift_query_core::OrderBy {
        field: FieldId(1),
        direction: SortOrder::Asc,
        nulls: sift_query_core::NullOrder::Last,
    });
    assert_eq!(
        PgRenderer.render_list(&built).unwrap_err(),
        RenderError::InvalidCondition("cursor value count does not match ordering".to_owned())
    );
}

#[test]
fn a_second_page_uses_a_strict_comparison_without_an_offset() {
    let codec = sift_query_core::CursorCodec::new([0x95; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&RawParams::parse("cursor=&size=5").unwrap())
        .unwrap();
    let token = codec
        .encode::<User>(
            base.order(),
            &[sift_query_core::CursorValue::Scalar(ScalarValue::I64(10))],
        )
        .unwrap();
    let raw = RawParams::parse(&format!("cursor={token}&size=5")).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let built =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    assert!(rendered.sql.contains("(u.id > $1 OR u.id IS NULL)"));
    assert!(rendered.sql.ends_with("LIMIT $2"));
}

#[test]
fn descending_cursors_compare_with_less_than() {
    let codec = sift_query_core::CursorCodec::new([0x96; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&RawParams::parse("sort=id&order=desc&cursor=&size=5").unwrap())
        .unwrap();
    let token = codec
        .encode::<User>(
            base.order(),
            &[sift_query_core::CursorValue::Scalar(ScalarValue::I64(10))],
        )
        .unwrap();
    let raw = RawParams::parse(&format!("sort=id&order=desc&cursor={token}&size=5")).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let built =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    assert!(rendered.sql.contains("u.id < $1"));
}
