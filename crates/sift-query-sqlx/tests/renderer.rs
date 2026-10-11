use sift_query_core::{
    CursorCodec, CursorValue, CustomOperator, EntityLimits, EntityMeta, FieldId, FieldKind,
    FieldMeta, FieldRef, FilterExpr, Operator, OperatorSet, OrderMeta, ParamParser, QueryBuilder,
    QueryPlan, QueryPolicy, QueryTarget, RawParams, ScalarKind, ScalarValue, SearchEntity,
    ServerScope, SortOrder, ValueType,
};
use sift_query_sqlx::{PgBindValue, PgRenderer};

const ID_OPS: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::Gt,
    Operator::Ge,
    Operator::Lt,
    Operator::Le,
    Operator::Between,
    Operator::NotBetween,
    Operator::InList,
    Operator::NotIn,
    Operator::IsNull,
    Operator::NotNull,
    Operator::AlwaysTrue,
    Operator::AlwaysFalse,
];
const NAME_OPS: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::IsNull,
    Operator::NotNull,
    Operator::StartWith,
    Operator::EndWith,
    Operator::Contain,
    Operator::OrLike,
    Operator::NotLike,
    Operator::Empty,
    Operator::NotEmpty,
];
static FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "id",
        column: "u.id",
        result_alias: Some("id"),
        value_type: ValueType::I64,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(ID_OPS),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "name",
        column: "u.name",
        result_alias: Some("name"),
        value_type: ValueType::Text,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(NAME_OPS),
    },
];
static ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];
static META: EntityMeta = EntityMeta {
    entity_name: "User",
    from: "app_user u",
    base_where: Some("u.deleted = false"),
    fixed_having: None,
    distinct: false,
    fields: FIELDS,
    default_order: ORDER,
    stable_order: ORDER,
    limits: EntityLimits {
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 5_000,
    },
    schema_fingerprint: 7,
};

struct User;
impl SearchEntity for User {
    const META: &'static EntityMeta = &META;
}

static DISTINCT_META: EntityMeta = EntityMeta {
    distinct: true,
    schema_fingerprint: 8,
    ..META
};

#[derive(Debug)]
struct DistinctUser;
impl SearchEntity for DistinctUser {
    const META: &'static EntityMeta = &DISTINCT_META;
}

const SCORE_OPS: &[Operator] = &[Operator::Ge];
static SCORE_FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "courseId",
        column: "s.course_id",
        result_alias: Some("course_id"),
        value_type: ValueType::I64,
        kind: FieldKind::GroupKey,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(ID_OPS),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "total",
        column: "sum(s.score)",
        result_alias: Some("total"),
        value_type: ValueType::I64,
        kind: FieldKind::Aggregate,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(SCORE_OPS),
    },
];
static SCORE_ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];
static SCORE_META: EntityMeta = EntityMeta {
    entity_name: "Score",
    from: "student_score s",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: SCORE_FIELDS,
    default_order: SCORE_ORDER,
    stable_order: SCORE_ORDER,
    limits: EntityLimits {
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 5_000,
    },
    schema_fingerprint: 9,
};

struct Score;
impl SearchEntity for Score {
    const META: &'static EntityMeta = &SCORE_META;
}

fn plan(query: &str) -> QueryPlan<User> {
    let raw = RawParams::parse(query).unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap()
}

#[test]
fn in_list_uses_one_array_slot_and_stable_shape() {
    let one = PgRenderer.render_list(&plan("id=1&id-op=il")).unwrap();
    let many = PgRenderer.render_list(&plan("id=1&id=2&id-op=il")).unwrap();
    assert_eq!(one.sql, many.sql);
    assert_eq!(one.shape_id, many.shape_id);
    assert_eq!(many.binds.len(), 3);
    assert!(many.sql.contains("u.id = ANY($1)"));
    assert!(many.sql.ends_with("LIMIT $2 OFFSET $3"));
}

#[test]
fn contain_escapes_user_wildcards() {
    let rendered = PgRenderer
        .render_list(&plan("name=a%25_%5C&name-op=ct"))
        .unwrap();
    assert!(rendered.sql.contains("u.name LIKE $1 ESCAPE '\\'"));
    assert_eq!(
        rendered.binds[0],
        PgBindValue::Scalar(ScalarValue::Text("%a\\%\\_\\\\%".to_owned()))
    );
}

#[test]
fn count_omits_order_and_paging() {
    let rendered = PgRenderer.render_count(&plan("name=Alice")).unwrap();
    assert!(!rendered.sql.contains("ORDER BY"));
    assert!(!rendered.sql.contains("LIMIT"));
    assert_eq!(rendered.binds.len(), 1);
}

#[test]
fn facet_rendering_is_bounded_and_reuses_authorization_predicates() {
    let plan = plan("name=Al&name-op=sw");
    let rendered = PgRenderer
        .render_facet(&plan, FieldId(1), 12, false)
        .unwrap();
    assert!(
        rendered
            .sql
            .contains("u.deleted = false AND u.name LIKE $1 ESCAPE '\\'")
    );
    assert!(rendered.sql.contains("u.name IS NOT NULL"));
    assert!(rendered.sql.contains("GROUP BY u.name"));
    assert!(
        rendered
            .sql
            .contains("ORDER BY count(*) DESC, u.name ASC LIMIT $2")
    );
    assert_eq!(rendered.binds.len(), 2);
}

#[test]
fn facet_can_exclude_its_own_user_filter() {
    let plan = plan("name=Al&name-op=sw");
    let rendered = PgRenderer
        .render_facet_with_options(&plan, FieldId(1), 12, true, true)
        .unwrap();
    assert!(rendered.sql.contains("u.deleted = false"));
    assert!(!rendered.sql.contains("u.name LIKE"));
    assert!(rendered.sql.contains("LIMIT $1"));
    assert_eq!(rendered.binds.len(), 1);
}

#[test]
fn renders_all_builtin_operator_shapes() {
    let cases = [
        ("id=1", "u.id = $1"),
        ("id=1&id-op=ne", "u.id <> $1"),
        ("id=1&id-op=gt", "u.id > $1"),
        ("id=1&id-op=ge", "u.id >= $1"),
        ("id=1&id-op=lt", "u.id < $1"),
        ("id=1&id-op=le", "u.id <= $1"),
        ("id=1&id=2&id-op=bt", "u.id BETWEEN $1 AND $2"),
        ("id=1&id=2&id-op=nb", "u.id NOT BETWEEN $1 AND $2"),
        ("id=1&id=2&id-op=il", "u.id = ANY($1)"),
        ("id=1&id=2&id-op=ni", "u.id <> ALL($1)"),
        ("id-op=nl", "u.id IS NULL"),
        ("id-op=nn", "u.id IS NOT NULL"),
        ("name-op=ey", "(u.name IS NULL OR u.name = '')"),
        ("name-op=ny", "(u.name IS NOT NULL AND u.name <> '')"),
        ("name=Al&name-op=sw", "u.name LIKE $1 ESCAPE '\\'"),
        ("name=ce&name-op=ew", "u.name LIKE $1 ESCAPE '\\'"),
        ("name=li&name-op=ct", "u.name LIKE $1 ESCAPE '\\'"),
        ("name=%25A%25&name-op=nk", "u.name NOT LIKE $1"),
        (
            "name=A%25&name=B%25&name-op=ol",
            "(u.name LIKE $1 OR u.name LIKE $2)",
        ),
        ("id-op=at", "1 = 1"),
        ("id-op=af", "1 = 0"),
    ];
    for (query, expected) in cases {
        let rendered = PgRenderer.render_list(&plan(query)).unwrap();
        assert!(
            rendered.sql.contains(expected),
            "expected {expected:?} in {:?}",
            rendered.sql
        );
    }
}

#[test]
fn grouped_count_uses_minimal_inner_projection() {
    let raw = RawParams::parse("total=10&total-op=ge").unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<Score>(&raw)
        .unwrap();
    let plan = QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::Count).unwrap();
    let rendered = PgRenderer.render_count(&plan).unwrap();
    assert_eq!(
        rendered.sql.as_ref(),
        "SELECT count(*) AS \"sq_count\" FROM (SELECT 1 FROM student_score s \
         GROUP BY s.course_id HAVING sum(s.score) >= $1) sq_cluster"
    );
}

#[test]
fn distinct_map_rejects_a_hidden_stable_order_column() {
    let raw = RawParams::parse("onlySelect=name").unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<DistinctUser>(&raw)
        .unwrap();
    let error =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).unwrap_err();
    assert!(matches!(
        error,
        sift_query_core::PlanError::Param(sift_query_core::ParamError::DistinctOrderNotProjected(
            _
        ))
    ));
}

#[test]
fn cursor_uses_keyset_predicate_without_offset() {
    let codec = CursorCodec::new([0x44; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&RawParams::parse("cursor=&size=10").unwrap())
        .unwrap();
    let token = codec
        .encode::<User>(base.order(), &[CursorValue::Scalar(ScalarValue::I64(10))])
        .unwrap();
    let raw = RawParams::parse(&format!("cursor={token}&size=10")).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let plan =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&plan).unwrap();
    assert!(rendered.sql.contains("(u.id > $1 OR u.id IS NULL)"));
    assert!(rendered.sql.ends_with("LIMIT $2"));
    assert!(!rendered.sql.contains("OFFSET"));
}

#[test]
fn custom_operator_uses_only_validated_field_and_bind_placeholders() {
    let operator = CustomOperator::new("multiple_of", "mod({field}, {0}) = 0", 1).unwrap();
    let policy = QueryPolicy::default();
    let field = FieldRef::<User, i64, ScalarKind>::new(FieldId(0));
    let expression = FilterExpr::custom(field, operator, [2_i64]).unwrap();
    let query = QueryBuilder::<User>::new(&policy)
        .filter(expression)
        .build()
        .unwrap();
    let plan =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&plan).unwrap();
    assert!(rendered.sql.contains("mod(u.id, $1) = 0"));
}
