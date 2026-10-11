use super::*;
use sift_query_core::{
    EntityLimits, EntityMeta, FieldId, FieldKind, FieldMeta, Operator, OperatorSet, OrderMeta,
    ParamParser, QueryPolicy, QueryTarget, RawParams, SearchEntity, ServerScope, SortOrder,
    ValueType,
};

#[test]
fn the_extension_debug_form_reports_its_configuration() {
    let empty = PgExtensions::default();
    let rendered = format!("{empty:?}");
    assert!(
        rendered.contains("has_partition_resolver: false"),
        "{rendered}"
    );
    assert!(rendered.contains("sql_interceptors: 0"), "{rendered}");
    assert!(rendered.contains("result_filters: 0"), "{rendered}");

    let mut configured = PgExtensions::default();
    configured.set_partition_resolver(Arc::new(Decline));
    configured.push_sql_interceptor(Arc::new(Noop));
    configured.push_result_filter(Arc::new(Keep));
    let rendered = format!("{configured:?}");
    assert!(
        rendered.contains("has_partition_resolver: true"),
        "{rendered}"
    );
    assert!(rendered.contains("sql_interceptors: 1"), "{rendered}");
    assert!(rendered.contains("result_filters: 1"), "{rendered}");
}

struct Decline;

impl PartitionResolver for Decline {
    fn resolve(
        &self,
        _context: &QueryContext<'_>,
    ) -> Result<Option<PartitionName>, ExtensionError> {
        Ok(None)
    }
}

struct Noop;

impl SqlInterceptor for Noop {
    fn intercept(
        &self,
        _context: &SqlContext,
        _sql: &str,
    ) -> Result<Option<String>, ExtensionError> {
        Ok(None)
    }
}

struct Keep;

impl ResultFilter for Keep {
    fn filter(
        &self,
        _context: &ResultContext,
        _row_index: usize,
        _row: &mut RowMap,
    ) -> Result<ResultFilterDecision, ExtensionError> {
        Ok(ResultFilterDecision::Keep)
    }
}

#[test]
fn rewriting_sql_without_the_entity_source_is_reported() {
    // A partition rewrite only works when the rendered SQL still names the
    // entity source. A renderer or interceptor that already replaced the
    // `FROM` clause makes the rewrite impossible, which is reported rather
    // than silently skipped.
    let extensions = PgExtensions::default();
    let error = extensions
        .apply_rendered(
            &plan(),
            "search",
            "list",
            Some("\"other\""),
            rendered("SELECT u.id FROM elsewhere"),
        )
        .unwrap_err();
    assert_eq!(error, ExtensionError::PartitionSourceNotFound);
}

#[test]
fn a_rendered_query_is_returned_unchanged_when_nothing_applies() {
    let extensions = PgExtensions::default();
    let original = rendered("SELECT u.id FROM app_user u");
    let output = extensions
        .apply_rendered(&plan(), "search", "list", None, original.clone())
        .unwrap();
    assert_eq!(output.sql, original.sql);
    assert_eq!(output.shape_id, original.shape_id);
}

#[test]
fn a_partition_rewrite_changes_the_shape_id() {
    let extensions = PgExtensions::default();
    let original = rendered("SELECT u.id FROM app_user u");
    let output = extensions
        .apply_rendered(
            &plan(),
            "search",
            "list",
            Some("\"archive\".\"u\""),
            original.clone(),
        )
        .unwrap();
    assert!(output.sql.contains("FROM \"archive\".\"u\""));
    assert_ne!(output.shape_id, original.shape_id);
}

fn rendered(sql: &str) -> RenderedPgQuery {
    RenderedPgQuery {
        sql: Arc::from(sql),
        binds: Vec::new(),
        shape_id: QueryShapeId(1),
    }
}

static FIELDS: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "id",
    column: "u.id",
    result_alias: Some("id"),
    value_type: ValueType::I64,
    kind: FieldKind::Scalar,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(&[Operator::Eq]),
}];
static ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];
static META: EntityMeta = EntityMeta {
    entity_name: "User",
    from: "app_user u",
    base_where: None,
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
    schema_fingerprint: 61,
};

#[derive(Debug)]
struct Entity;

impl SearchEntity for Entity {
    const META: &'static EntityMeta = &META;
}

fn plan() -> QueryPlan<Entity> {
    let raw = RawParams::parse("").unwrap();
    let parsed = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Entity>(&raw)
        .unwrap();
    QueryPlan::new(
        &parsed,
        &ServerScope::unrestricted(),
        QueryTarget::TypedPage,
    )
    .unwrap()
}
#[tokio::test]
async fn page_enrichers_cannot_change_database_metadata() {
    use crate::{EnrichContext, ExtensionFuture, Page, PageEnricher, RowMap};
    struct Corrupt;
    impl PageEnricher for Corrupt {
        fn enrich(
            &self,
            mut page: Page<RowMap>,
            _: EnrichContext,
        ) -> ExtensionFuture<Page<RowMap>> {
            Box::pin(async move {
                page.total_count += 1;
                Ok(page)
            })
        }
    }
    let mut extensions = super::PgExtensions::default();
    extensions.push_page_enricher(std::sync::Arc::new(Corrupt));
    assert!(
        extensions
            .enrich_page(
                "Entity",
                "search_map",
                Page {
                    data: Vec::new(),
                    total_count: 0,
                    summaries: Vec::new(),
                    next_cursor: None
                }
            )
            .await
            .is_err()
    );
}
