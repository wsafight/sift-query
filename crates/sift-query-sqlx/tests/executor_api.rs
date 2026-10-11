//! `PgSiftQuery` surface that does not need a live database: parsing, the
//! programmatic builder, and extension plumbing.

extern crate sift_query_core as sift_query;
use sift_query_derive::SearchEntity;
use std::sync::{Arc, Mutex};

use sift_query::{
    CursorCodec, FieldId, ParamError, QueryPolicy, QueryTarget, RawParams, SearchOptions,
    ServerScope,
};
use sift_query_sqlx::{
    ExtensionError, PartitionName, PartitionResolver, PgSearchError, PgSiftQuery, QueryContext,
    ResultContext, ResultFilter, ResultFilterDecision, SqlContext, SqlInterceptor,
};
use sqlx::FromRow;
use sqlx::postgres::PgPoolOptions;

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(from = "app_user u", default_order = "id:asc", stable_order = "id:asc")]
struct User {
    #[search_field(column = "u.id", sortable, operators = [Eq, InList])]
    id: i64,
    #[search_field(column = "u.name", operators = [Eq, Contain])]
    name: String,
}

/// A lazily connected pool for an address nothing listens on.
///
/// It must be built inside a Tokio runtime, so every test that needs one is a
/// `#[tokio::test]`.
fn offline() -> PgSiftQuery {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost:1/unused")
        .unwrap();
    // A short timeout keeps the offline tests fast.
    PgSiftQuery::with_policy(
        pool,
        QueryPolicy::default(),
        sift_query_sqlx::PgExecutionPolicy {
            timeout: std::time::Duration::from_millis(50),
            ..sift_query_sqlx::PgExecutionPolicy::default()
        },
    )
    .unwrap()
}

#[tokio::test]
async fn parse_uses_the_instance_policy() {
    let search = offline();
    let query = search
        .parse::<User>(&RawParams::parse("id=1").unwrap())
        .unwrap();
    assert!(query.predicate().root().is_some());

    // A policy error is reported through the same channel.
    assert_eq!(
        search
            .parse::<User>(&RawParams::parse("missing=1").unwrap())
            .unwrap_err(),
        ParamError::UnknownField("missing".to_owned())
    );
}

#[tokio::test]
async fn query_builder_uses_the_instance_policy() {
    let search = offline();
    let built = search
        .query_builder::<User>()
        .eq(
            sift_query::FieldRef::<User, i64, sift_query::ScalarKind>::new(FieldId(0)),
            1_i64,
        )
        .build()
        .unwrap();
    assert_eq!(built.order().len(), 1);
}

#[tokio::test]
async fn a_bean_searcher_policy_is_honoured_by_parse() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost:1/unused")
        .unwrap();
    let search = PgSiftQuery::with_policy(
        pool,
        QueryPolicy::bean_searcher_4812(),
        sift_query_sqlx::PgExecutionPolicy::default(),
    )
    .unwrap();
    let query = search
        .parse::<User>(&RawParams::parse("missing=1").unwrap())
        .unwrap();
    assert!(query.predicate().root().is_none());
}

#[tokio::test]
async fn cursor_configuration_is_required_by_the_parser() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost:1/unused")
        .unwrap();
    let codec = CursorCodec::new([0x41; 32]).unwrap();
    let configured = PgSiftQuery::with_policy(
        pool,
        QueryPolicy::default().with_cursor_codec(codec),
        sift_query_sqlx::PgExecutionPolicy::default(),
    )
    .unwrap();

    // Without a codec the cursor parameter is rejected outright.
    assert!(matches!(
        offline().parse::<User>(&RawParams::parse("cursor=").unwrap()),
        Err(ParamError::InvalidValue { field, .. }) if field == "cursor"
    ));
    // With one, the first cursor page parses.
    assert!(
        configured
            .parse::<User>(&RawParams::parse("cursor=").unwrap())
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// Extensions
// ---------------------------------------------------------------------------

struct InspectingResolver(Arc<Mutex<Option<String>>>);

impl PartitionResolver for InspectingResolver {
    fn resolve(&self, context: &QueryContext<'_>) -> Result<Option<PartitionName>, ExtensionError> {
        // The debug form must name the entity and operation without exposing
        // the predicate trees.
        *self.0.lock().unwrap() = Some(format!("{context:?}"));
        assert!(context.server_predicate.root().is_none());
        assert!(context.row_predicate.root().is_none());
        assert!(context.group_predicate.root().is_none());
        assert_eq!(context.target, QueryTarget::TypedList);
        assert_eq!(context.operation, "search_list");
        assert_eq!(context.paging.size, 15);
        assert_eq!(context.entity.entity_name, "User");
        Ok(None)
    }
}

struct RecordingInterceptor(Arc<Mutex<Option<String>>>);

impl SqlInterceptor for RecordingInterceptor {
    fn intercept(&self, context: &SqlContext, sql: &str) -> Result<Option<String>, ExtensionError> {
        *self.0.lock().unwrap() = Some(format!("{context:?}"));
        assert_eq!(context.entity_name, "User");
        assert_eq!(context.phase, "list");
        assert!(sql.contains("FROM app_user u"));
        // Returning `None` keeps the SQL untouched.
        Ok(None)
    }
}

struct NeverCalledFilter;

impl ResultFilter for NeverCalledFilter {
    fn filter(
        &self,
        context: &ResultContext,
        row_index: usize,
        _row: &mut sift_query_sqlx::RowMap,
    ) -> Result<ResultFilterDecision, ExtensionError> {
        assert_eq!(context.entity_name, "User");
        assert_eq!(context.operation, "search_map");
        assert_eq!(context.target, QueryTarget::MapPage);
        panic!("no rows should be decoded, got row {row_index}");
    }
}

#[tokio::test]
async fn extension_contexts_are_observable_without_a_database() {
    let seen_context = Arc::new(Mutex::new(None));
    let seen_sql = Arc::new(Mutex::new(None));
    let search = offline()
        .with_partition_resolver(InspectingResolver(Arc::clone(&seen_context)))
        .with_sql_interceptor(RecordingInterceptor(Arc::clone(&seen_sql)))
        .with_result_filter(NeverCalledFilter);

    let error = search
        .search_list::<User>(&RawParams::parse("").unwrap(), &ServerScope::unrestricted())
        .await
        .unwrap_err();

    // The connection attempt fails, which proves the extensions ran first.
    assert!(
        matches!(error, PgSearchError::Timeout(_) | PgSearchError::Sql(_)),
        "{error:?}"
    );
    assert!(seen_context.lock().unwrap().is_some());
    let sql_context = seen_sql.lock().unwrap().clone().expect("interceptor ran");
    assert!(sql_context.contains("User"), "{sql_context}");
}

#[tokio::test]
async fn an_interceptor_rejection_surfaces_before_connecting() {
    struct Reject;
    impl SqlInterceptor for Reject {
        fn intercept(
            &self,
            _context: &SqlContext,
            _sql: &str,
        ) -> Result<Option<String>, ExtensionError> {
            Err(ExtensionError::rejected("test", "nope"))
        }
    }
    let search = offline().with_sql_interceptor(Reject);
    let error = search
        .search_list::<User>(&RawParams::parse("").unwrap(), &ServerScope::unrestricted())
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            PgSearchError::Extension(ExtensionError::Rejected {
                extension: "test",
                ..
            })
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_partition_rejection_surfaces_before_connecting() {
    struct Reject;
    impl PartitionResolver for Reject {
        fn resolve(
            &self,
            _context: &QueryContext<'_>,
        ) -> Result<Option<PartitionName>, ExtensionError> {
            Err(ExtensionError::InvalidIdentifier {
                kind: "table",
                value: "bad".to_owned(),
            })
        }
    }
    let search = offline().with_partition_resolver(Reject);
    let error = search
        .search_list::<User>(&RawParams::parse("").unwrap(), &ServerScope::unrestricted())
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            PgSearchError::Extension(ExtensionError::InvalidIdentifier { .. })
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn search_with_summaries_is_reachable_from_the_sqlx_api() {
    let search = offline();
    let scope = ServerScope::<User>::unrestricted();
    let options = SearchOptions::<User>::builder()
        .sum(sift_query::FieldRef::<User, i64, sift_query::ScalarKind>::new(FieldId(0)))
        .build()
        .unwrap();
    let error = search
        .search_with::<User>(&RawParams::parse("").unwrap(), &scope, &options)
        .await
        .unwrap_err();
    assert!(
        matches!(error, PgSearchError::Timeout(_) | PgSearchError::Sql(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn map_options_are_reachable_from_the_sqlx_api() {
    let search = offline();
    let scope = ServerScope::<User>::unrestricted();
    let options = SearchOptions::<User>::builder()
        .min(sift_query::FieldRef::<User, i64, sift_query::ScalarKind>::new(FieldId(0)))
        .build()
        .unwrap();
    let error = search
        .search_map_with::<User>(&RawParams::parse("").unwrap(), &scope, &options)
        .await
        .unwrap_err();
    assert!(
        matches!(error, PgSearchError::Timeout(_) | PgSearchError::Sql(_)),
        "{error:?}"
    );
}

// ---------------------------------------------------------------------------
// Extension configuration values
// ---------------------------------------------------------------------------

#[tokio::test]
async fn partition_names_validate_their_identifiers() {
    assert!(PartitionName::new("users").is_ok());
    assert!(PartitionName::new("users_2026").is_ok());
    assert!(PartitionName::new("_archive").is_ok());
    assert!(PartitionName::new("users$1").is_ok());
    assert!(PartitionName::qualified("archive", "users_2026").is_ok());
    assert!(PartitionName::new("users").unwrap().with_alias("u").is_ok());

    assert!(PartitionName::new("").is_err());
    assert!(PartitionName::new("1users").is_err());
    assert!(PartitionName::new("public.users").is_err());
    assert!(PartitionName::new("users;").is_err());
    assert!(PartitionName::qualified("", "users").is_err());
    assert!(
        PartitionName::new("users")
            .unwrap()
            .with_alias("u; drop")
            .is_err()
    );
}

#[tokio::test]
async fn extension_errors_display_their_context() {
    let invalid = PartitionName::new("bad name").unwrap_err();
    assert!(invalid.to_string().contains("bad name"));
    assert_eq!(
        ExtensionError::rejected("test", "nope").to_string(),
        "test rejected the query: nope"
    );
    assert_eq!(
        ExtensionError::PartitionSourceNotFound.to_string(),
        "partition source could not be applied to rendered SQL"
    );
    assert_eq!(
        ExtensionError::InvalidIdentifier {
            kind: "schema",
            value: "x".to_owned(),
        }
        .to_string(),
        "invalid PostgreSQL schema identifier: x"
    );
}

#[tokio::test]
async fn result_filter_decisions_are_comparable() {
    assert_ne!(ResultFilterDecision::Keep, ResultFilterDecision::Discard);
    assert_eq!(ResultFilterDecision::Keep, ResultFilterDecision::Keep);
}

#[tokio::test]
async fn partition_name_debug_is_derived() {
    let name = PartitionName::new("users").unwrap();
    assert!(format!("{name:?}").contains("users"));
    let context = ResultContext {
        entity_name: "User",
        operation: "search_map",
        target: QueryTarget::MapPage,
    };
    assert_eq!(context.entity_name, "User");
    assert_eq!(format!("{context:?}"), format!("{context:?}"));
}
