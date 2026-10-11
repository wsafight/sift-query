use sift_query::{
    FieldKind, FilterExpr, Operator, ParamError, ParamParser, PredicateNode, Projection,
    QueryBuilder, QueryPlan, QueryPolicy, QueryTarget, RawParams, SearchEntity as _,
    SearchProjection as _, ServerScope, SortOrder,
};

#[derive(Debug, sift_query::SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "app_user u",
    base_where = "u.deleted = false",
    default_order = "name:desc",
    stable_order = "id:asc"
)]
struct User {
    #[search_field(column = "u.id", sortable, operators = [Eq, InList])]
    id: i64,

    #[search_field(column = "u.name", sortable, operators = [Eq, StartWith])]
    name: String,
}

#[derive(Debug, sift_query::SearchProjection)]
#[allow(dead_code)]
#[search_projection(entity = User)]
struct UserSummary {
    id: i64,
    #[search_field(source = "name")]
    display_name: String,
}

#[derive(Debug, sift_query::SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "student_score s",
    default_order = "courseId:asc",
    stable_order = "courseId:asc"
)]
struct Score {
    #[search_field(
        name = "courseId",
        column = "s.course_id",
        kind = GroupKey,
        sortable,
        operators = [Eq]
    )]
    course_id: i64,
    #[search_field(
        column = "sum(s.score)",
        kind = Aggregate,
        operators = [Ge]
    )]
    total: i64,
}

#[test]
fn derive_builds_static_metadata_and_typed_fields() {
    assert_eq!(User::META.entity_name, "User");
    assert_eq!(User::META.fields.len(), 2);
    assert_eq!(User::META.fields[0].kind, FieldKind::Scalar);
    assert_eq!(User::META.default_order[0].direction, SortOrder::Desc);
    assert_eq!(UserFields::ID.id().0, 0);
}

#[test]
fn projection_derive_maps_source_fields_and_result_aliases() {
    assert_eq!(UserSummary::FIELDS.len(), 2);
    assert_eq!(UserSummary::FIELDS[0].source, UserFields::ID.id());
    assert_eq!(UserSummary::FIELDS[1].source, UserFields::NAME.id());
    assert_eq!(UserSummary::FIELDS[1].result_alias, "display_name");
}

#[test]
fn derived_entity_works_with_the_strict_parser() {
    let raw = RawParams::parse("id=1&id=2&id-op=il&sort=name&order=asc").unwrap();
    let query = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    assert_eq!(query.order().len(), 2);
    let root = query.predicate().root().unwrap();
    let node = query.predicate().node(root).unwrap();
    let condition = match node {
        sift_query::PredicateNode::Condition(condition) => condition,
        other => panic!("unexpected root node: {other:?}"),
    };
    assert_eq!(condition.operator, Operator::InList);
}

#[test]
fn programmatic_builder_uses_the_same_validated_query_model() {
    let policy = QueryPolicy::default();
    let query = QueryBuilder::<User>::new(&policy)
        .in_list(UserFields::ID, [1_i64, 2_i64])
        .starts_with(UserFields::NAME, "Al")
        .only_select(UserFields::NAME)
        .offset(10, 20)
        .build()
        .unwrap();

    assert_eq!(
        query.projection(),
        &Projection::Fields(vec![UserFields::NAME.id()])
    );
    assert_eq!(query.paging().offset, 10);
    assert_eq!(query.order().last().unwrap().field, UserFields::ID.id());
}

#[test]
fn server_scopes_can_be_composed_without_raw_sql() {
    let first = ServerScope::<User>::new()
        .eq(UserFields::ID, 1_i64)
        .build()
        .unwrap();
    let second = ServerScope::<User>::new()
        .eq(UserFields::ID, 2_i64)
        .build()
        .unwrap();
    let combined = ServerScope::any([first, second]).unwrap();
    assert!(combined.predicate().root().is_some());
}

#[test]
fn programmatic_filter_supports_nested_boolean_groups() {
    let policy = QueryPolicy::default();
    let alternatives = FilterExpr::any([
        FilterExpr::eq(UserFields::ID, 1_i64).unwrap(),
        FilterExpr::starts_with(UserFields::NAME, "A").unwrap(),
    ])
    .unwrap();
    let query = QueryBuilder::<User>::new(&policy)
        .filter(alternatives)
        .build()
        .unwrap();
    let root = query.predicate().root().unwrap();
    assert!(matches!(
        query.predicate().node(root),
        Some(PredicateNode::Or(_))
    ));
}

#[test]
fn programmatic_filter_rejects_cross_phase_or() {
    let error = FilterExpr::any([
        FilterExpr::eq(ScoreFields::COURSE_ID, 1_i64).unwrap(),
        FilterExpr::ge(ScoreFields::TOTAL, 100_i64).unwrap(),
    ])
    .unwrap_err();
    assert_eq!(error, sift_query::ParamError::MixedPredicatePhase);
}

#[test]
fn search_first_rejects_explicit_paging() {
    let raw = RawParams::parse("size=1").unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let error =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::First).unwrap_err();
    assert_eq!(
        error,
        sift_query::PlanError::Param(ParamError::PagingNotAllowed)
    );
}

// ---------------------------------------------------------------------------
// Derive surface: aliases, sqlx interop, filter-only fields, and cursors
// ---------------------------------------------------------------------------

#[derive(Debug, sift_query::SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "app_user u",
    base_where = "u.deleted = false",
    fixed_having = "count(*) > 0",
    default_order = "id:asc",
    stable_order = "id:asc",
    max_page_size = 20,
    max_offset = 100,
    timeout_ms = 250
)]
struct Detailed {
    #[search_field(column = "u.id", sortable, operators = [Eq, InList])]
    id: i64,
    #[search_field(name = "displayName", column = "u.name", result_alias = "display_name")]
    #[sqlx(rename = "display_name")]
    display_name: String,
    #[search_field(name = "internal", column = "u.internal", filterable = false)]
    internal: i32,
    #[search_field(name = "tenantId", column = "u.tenant_id", kind = FilterOnly, filterable)]
    #[sqlx(skip)]
    tenant_id: sift_query::FilterOnly<i64>,
    #[search_field(column = "u.tag", operators = [OrLike])]
    tag: String,
}

#[derive(Debug, sift_query::SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "app_user u",
    default_order = "id:asc",
    stable_order = "id:asc",
    distinct
)]
struct DistinctRow {
    #[search_field(column = "u.id", sortable)]
    id: i64,
}

#[test]
fn derived_metadata_records_every_declared_option() {
    assert_eq!(Detailed::META.base_where, Some("u.deleted = false"));
    assert_eq!(Detailed::META.fixed_having, Some("count(*) > 0"));
    assert_eq!(Detailed::META.limits.max_page_size, 20);
    assert_eq!(Detailed::META.limits.max_offset, 100);
    assert_eq!(Detailed::META.limits.timeout_ms, 250);
    assert!(!Detailed::META.distinct);
    assert!(DistinctRow::META.distinct);
}

#[test]
fn derived_fields_carry_names_columns_aliases_and_kinds() {
    let display = Detailed::META.resolve_field("displayName").unwrap();
    assert_eq!(display.column, "u.name");
    // The alias comes from `#[sqlx(rename)]` when it agrees with the field.
    assert_eq!(display.result_alias, Some("display_name"));
    assert_eq!(display.value_type, sift_query::ValueType::Text);

    let internal = Detailed::META.resolve_field("internal").unwrap();
    assert!(!internal.filterable);

    let tenant = Detailed::META.resolve_field("tenantId").unwrap();
    assert_eq!(tenant.kind, FieldKind::FilterOnly);
    // Filter-only fields are never selectable and carry no alias.
    assert!(!tenant.selectable);
    assert_eq!(tenant.result_alias, None);
    assert!(Detailed::META.field(tenant.id).is_some());
}

#[test]
fn derived_operator_whitelists_are_exact() {
    let tag = Detailed::META.resolve_field("tag").unwrap();
    assert!(tag.operators.contains(Operator::OrLike));
    assert!(!tag.operators.contains(Operator::Eq));

    let id = Detailed::META.resolve_field("id").unwrap();
    assert!(id.operators.contains(Operator::InList));
    assert!(!id.operators.contains(Operator::Contain));
}

#[test]
fn derived_entities_validate_against_the_static_metadata_rules() {
    assert_eq!(Detailed::META.validate(), Ok(()));
    assert_eq!(DistinctRow::META.validate(), Ok(()));
    assert_eq!(User::META.validate(), Ok(()));
    assert_eq!(Score::META.validate(), Ok(()));
}

#[test]
fn derived_entities_round_trip_through_the_parser() {
    let query = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Detailed>(&RawParams::parse("tenantId=9&displayName=Al").unwrap())
        .unwrap();
    assert_eq!(
        query
            .predicate()
            .nodes()
            .iter()
            .filter(|node| matches!(node, sift_query::PredicateNode::Condition(_)))
            .count(),
        2
    );

    // `displayName` only whitelists `Eq`, so a string operator is rejected.
    let error = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Detailed>(&RawParams::parse("displayName=Al&displayName-op=sw").unwrap())
        .unwrap_err();
    assert_eq!(
        error,
        ParamError::OperatorNotAllowed {
            field: "displayName".to_owned(),
            operator: "sw",
        }
    );
}

#[test]
fn derived_filter_only_fields_are_filterable_but_not_projectable() {
    let query = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Detailed>(&RawParams::parse("tenantId=9").unwrap())
        .unwrap();
    assert!(query.predicate().root().is_some());

    let error = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Detailed>(&RawParams::parse("onlySelect=tenantId").unwrap())
        .unwrap_err();
    assert_eq!(error, ParamError::NotSelectable("tenantId".to_owned()));
}

#[test]
fn derived_or_like_accepts_multiple_patterns() {
    let query = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Detailed>(&RawParams::parse("tag=A%25&tag=B%25&tag-op=ol").unwrap())
        .unwrap();
    let Some(sift_query::PredicateNode::Condition(condition)) = query
        .predicate()
        .root()
        .and_then(|root| query.predicate().node(root))
    else {
        panic!("expected a single condition");
    };
    assert_eq!(condition.operator, Operator::OrLike);
    assert_eq!(condition.values.len(), 2);
}

#[test]
fn derived_cursor_values_come_from_the_result_struct() {
    let row = Detailed {
        id: 7,
        display_name: "Alice".to_owned(),
        internal: 1,
        tenant_id: sift_query::FilterOnly::default(),
        tag: "x".to_owned(),
    };
    // Only sortable fields with a result alias expose cursor values.
    assert_eq!(
        row.cursor_value(DetailedFields::ID.id()),
        Some(sift_query::CursorValue::Scalar(
            sift_query::ScalarValue::I64(7)
        ))
    );
    assert_eq!(row.cursor_value(DetailedFields::TAG.id()), None);
    assert_eq!(row.cursor_value(sift_query::FieldId(99)), None);
}

#[test]
fn projection_cursor_values_come_from_the_projection_struct() {
    use sift_query::SearchProjection as _;
    let row = UserSummary {
        id: 3,
        display_name: "Bob".to_owned(),
    };
    assert_eq!(
        row.cursor_value(UserFields::ID.id()),
        Some(sift_query::CursorValue::Scalar(
            sift_query::ScalarValue::I64(3)
        ))
    );
    // The projection maps `display_name` onto the entity's `name` field, so
    // that source id yields the mapped value.
    assert_eq!(
        row.cursor_value(UserFields::NAME.id()),
        Some(sift_query::CursorValue::Scalar(
            sift_query::ScalarValue::Text("Bob".to_owned())
        ))
    );
    assert_eq!(row.cursor_value(sift_query::FieldId(99)), None);
}

#[test]
fn derived_entities_work_with_server_scopes() {
    let scope = ServerScope::<Detailed>::new()
        .eq(DetailedFields::TENANT_ID, 1_i64)
        .contains(DetailedFields::TAG, "a".to_owned())
        .build()
        .unwrap();
    assert!(scope.predicate().root().is_some());
}

#[test]
fn derived_entities_reject_a_mismatched_query_policy() {
    let query = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Detailed>(&RawParams::parse("").unwrap())
        .unwrap();
    let limits = sift_query::QueryLimits {
        max_conditions: 4,
        ..sift_query::QueryLimits::default()
    };
    let other = QueryPolicy::new(sift_query::ParamNames::default(), limits).unwrap();
    assert!(sift_query::QueryInput::into_user_query(&query, &other).is_err());
}

#[test]
fn derived_metadata_is_accepted_by_the_plan_builder() {
    let query = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Detailed>(&RawParams::parse("tenantId=9").unwrap())
        .unwrap();
    let plan = QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).unwrap();
    // Filter-only fields never appear in the projection.
    assert!(!plan.projection.contains(&DetailedFields::TENANT_ID.id()));
    assert_eq!(plan.meta.entity_name, "Detailed");
}

#[derive(Debug, sift_query::SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "app_user u",
    default_order = "id:asc",
    stable_order = "id:asc",
    distinct = false
)]
struct AllKinds {
    #[search_field(column = "u.id", kind = GroupKey, sortable, selectable = false, filterable = false)]
    id: i64,
    #[search_field(column = "u.key", kind = GroupKey, sortable)]
    key: i64,
    #[search_field(column = "sum(u.total)", kind = Aggregate, operators = [Eq])]
    total: i64,
    #[search_field(column = "u.tenant_id", kind = FilterOnly, filterable)]
    #[sqlx(skip)]
    tenant: sift_query::FilterOnly<i64>,
    #[search_field(column = "u.extra")]
    #[search_ignore]
    #[sqlx(default)]
    extra: String,
}

#[test]
fn every_field_kind_and_flag_is_accepted_together() {
    assert_eq!(AllKinds::META.validate(), Ok(()));
    assert_eq!(AllKinds::META.entity_name, "AllKinds");
    assert!(!AllKinds::META.distinct);
    assert_eq!(AllKinds::META.fields.len(), 4);
    assert!(AllKinds::META.is_grouped());

    let id = AllKinds::META.resolve_field("id").unwrap();
    assert!(!id.selectable);
    assert!(!id.filterable);
    assert_eq!(id.result_alias, Some("id"));

    assert_eq!(
        AllKinds::META.resolve_field("key").unwrap().kind,
        FieldKind::GroupKey
    );
    assert_eq!(
        AllKinds::META.resolve_field("total").unwrap().kind,
        FieldKind::Aggregate
    );
    let tenant = AllKinds::META.resolve_field("tenant").unwrap();
    assert_eq!(tenant.kind, FieldKind::FilterOnly);
    assert!(AllKinds::META.resolve_field("extra").is_none());

    // Grouped entities put aggregate conditions in the HAVING phase.
    let query = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<AllKinds>(&RawParams::parse("total=1&tenant=2").unwrap())
        .unwrap();
    let (row, group) = query.predicate().partition().unwrap();
    assert!(row.root().is_some());
    assert!(group.root().is_some());
}
