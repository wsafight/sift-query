use super::*;

// ---------------------------------------------------------------------------
// QueryPlan::new
// ---------------------------------------------------------------------------

pub(super) fn plan_for<E: SearchEntity>(
    query: &str,
    target: QueryTarget,
) -> Result<QueryPlan<E>, PlanError> {
    let raw = RawParams::parse(query).unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy).unwrap().parse::<E>(&raw).unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), target)
}

#[test]
fn typed_targets_expand_the_projection_to_every_alias() {
    let plan = plan_for::<RichEntity>("", QueryTarget::TypedPage).unwrap();
    // `secret` has no alias, so it is excluded from the typed projection.
    assert_eq!(
        plan.projection,
        vec![
            FieldId(0),
            FieldId(1),
            FieldId(2),
            FieldId(3),
            FieldId(4),
            FieldId(6)
        ]
    );
    assert!(plan.projection_aliases.is_none());
    assert_eq!(plan.meta.entity_name, "RichEntity");
    assert_eq!(plan.target, QueryTarget::TypedPage);
}

#[test]
fn map_targets_honour_the_requested_dynamic_projection() {
    let all = plan_for::<RichEntity>("", QueryTarget::MapPage).unwrap();
    assert_eq!(all.projection.len(), 6);

    let selected = plan_for::<RichEntity>("onlySelect=name", QueryTarget::MapPage).unwrap();
    assert_eq!(selected.projection, vec![FieldId(1)]);

    let excluded = plan_for::<RichEntity>("selectExclude=name", QueryTarget::MapPage).unwrap();
    assert_eq!(
        excluded.projection,
        vec![FieldId(0), FieldId(2), FieldId(3), FieldId(4), FieldId(6)]
    );
}

#[test]
fn typed_targets_reject_a_dynamic_projection() {
    for target in [
        QueryTarget::TypedPage,
        QueryTarget::TypedList,
        QueryTarget::First,
    ] {
        let raw = RawParams::parse("onlySelect=name").unwrap();
        let policy = QueryPolicy::default();
        let query = ParamParser::new(&policy)
            .unwrap()
            .parse::<RichEntity>(&raw)
            .unwrap();
        assert_eq!(
            QueryPlan::new(&query, &ServerScope::unrestricted(), target).unwrap_err(),
            PlanError::Param(ParamError::ProjectionNotAllowed),
            "target {target:?}"
        );
    }
}

#[test]
fn first_target_rejects_explicit_paging_and_pins_the_page_size() {
    let implicit = plan_for::<RichEntity>("", QueryTarget::First).unwrap();
    assert_eq!(implicit.paging.size, 1);
    assert_eq!(implicit.paging.mode, PagingMode::Page { page: 1 });

    for query in ["page=2", "size=5", "offset=5"] {
        assert_eq!(
            plan_for::<RichEntity>(query, QueryTarget::First).unwrap_err(),
            PlanError::Param(ParamError::PagingNotAllowed),
            "query {query}"
        );
    }
}

#[test]
fn a_query_is_bound_to_the_policy_that_parsed_it() {
    use sift_query_core::QueryInput;

    let raw = RawParams::parse("").unwrap();
    let original = QueryPolicy::default();
    let query = ParamParser::new(&original)
        .unwrap()
        .parse::<RichEntity>(&raw)
        .unwrap();

    // Reusing a parsed query under a different policy is rejected, which is
    // what keeps cached queries from escaping their configuration.
    let limits = sift_query_core::QueryLimits {
        max_conditions: 8,
        ..sift_query_core::QueryLimits::default()
    };
    let other = QueryPolicy::new(sift_query_core::ParamNames::default(), limits).unwrap();
    assert_eq!(
        (&query).into_user_query(&other).unwrap_err(),
        ParamError::ConflictingParameters("query policy fingerprint mismatch".to_owned())
    );
    // The original policy still accepts it.
    assert!((&query).into_user_query(&original).is_ok());
}

#[test]
fn cursor_paging_is_rejected_for_grouped_entities() {
    let policy = QueryPolicy::default()
        .with_cursor_codec(sift_query_core::CursorCodec::new([0x77; 32]).unwrap());
    let raw = RawParams::parse("cursor=").unwrap();
    // The parser rejects a cursor for a grouped entity before planning.
    assert!(
        ParamParser::new(&policy)
            .unwrap()
            .parse::<TestEntity>(&raw)
            .is_err()
    );
}

#[test]
fn cursor_paging_requires_the_ordering_to_be_projected() {
    let codec = sift_query_core::CursorCodec::new([0x66; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<RichEntity>(&RawParams::parse("cursor=&size=2").unwrap())
        .unwrap();
    let token = codec
        .encode::<RichEntity>(
            base.order(),
            &[sift_query_core::CursorValue::Scalar(
                sift_query_core::ScalarValue::I64(3),
            )],
        )
        .unwrap();
    let raw = RawParams::parse(&format!("cursor={token}&size=2&onlySelect=name")).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<RichEntity>(&raw)
        .unwrap();
    assert_eq!(
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).unwrap_err(),
        PlanError::Param(ParamError::NotSelectable("id".to_owned()))
    );
}

#[test]
fn distinct_entities_require_ordering_fields_in_the_projection() {
    let raw = RawParams::parse("onlySelect=name").unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<DistinctEntity>(&raw)
        .unwrap();
    // Dynamic projections are only reachable through the map target, which is
    // where the distinct ordering guard applies.
    assert_eq!(
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).unwrap_err(),
        PlanError::Param(ParamError::DistinctOrderNotProjected("id".to_owned()))
    );
    // Counting and summing are unaffected: no rows are returned.
    assert!(QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::Count).is_ok());
}

#[test]
fn plans_partition_the_user_predicate_into_row_and_group_halves() {
    let plan = plan_for::<TestEntity>("id=1&score=10&score-op=ge", QueryTarget::TypedPage).unwrap();
    assert!(plan.row_predicate.root().is_some());
    assert!(plan.group_predicate.root().is_some());
    assert!(plan.server_predicate.root().is_none());
    assert!(!plan.distinct);
    assert!(!plan.force_empty);
}

#[test]
fn shape_fingerprint_seed_depends_on_the_policy() {
    let strict = plan_for::<RichEntity>("", QueryTarget::TypedPage).unwrap();
    assert_eq!(
        strict.shape_fingerprint_seed(),
        RichEntity::META.schema_fingerprint ^ QueryPolicy::default().fingerprint().rotate_left(17)
    );
}
