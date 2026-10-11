use super::support::*;

// ---------------------------------------------------------------------------
// Extended coverage: every bindable scalar and array type, every execution
// target, and the extension failure modes.
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "sift_query_test.wide w",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
pub(super) struct Wide {
    #[search_field(column = "w.id", sortable, operators = [Eq, InList, NotIn, NotBetween])]
    pub(super) id: i64,
    #[search_field(column = "w.flag", operators = [Eq, InList])]
    pub(super) flag: bool,
    #[search_field(column = "w.count32", operators = [Eq, InList])]
    pub(super) count32: i32,
    #[search_field(column = "w.ratio32", operators = [Eq, InList])]
    pub(super) ratio32: f32,
    #[search_field(column = "w.ratio64", operators = [Eq, InList])]
    pub(super) ratio64: f64,
    #[search_field(column = "w.title", operators = [Eq, InList, Contain, StartWith])]
    pub(super) title: String,
    #[search_field(column = "w.data", operators = [Eq])]
    pub(super) data: Vec<u8>,
    #[search_field(column = "w.day", operators = [Eq, InList])]
    pub(super) day: chrono::NaiveDate,
    #[search_field(column = "w.clock", operators = [Eq, InList])]
    pub(super) clock: chrono::NaiveTime,
    #[search_field(column = "w.local_at", operators = [Eq, InList])]
    pub(super) local_at: chrono::NaiveDateTime,
    #[search_field(column = "w.instant", operators = [Eq, InList])]
    pub(super) instant: chrono::DateTime<chrono::Utc>,
    #[search_field(column = "w.external", operators = [Eq, InList])]
    pub(super) external: uuid::Uuid,
    #[search_field(column = "w.amount", operators = [Eq, InList])]
    pub(super) amount: rust_decimal::Decimal,
    #[search_field(filterable = false)]
    pub(super) payload: serde_json::Value,
}

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "sift_query_test.wide w",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
pub(super) struct WideCursor {
    #[search_field(column = "w.id", sortable)]
    pub(super) id: i64,
    #[search_field(column = "w.title", sortable)]
    pub(super) title: String,
    #[search_field(column = "w.amount", sortable)]
    pub(super) amount: rust_decimal::Decimal,
}

#[derive(Debug, FromRow, sift_query_derive::SearchProjection)]
#[allow(dead_code)]
#[search_projection(entity = WideCursor)]
pub(super) struct WideProjection {
    pub(super) id: i64,
    #[search_field(source = "title")]
    pub(super) heading: String,
}

pub(super) async fn create_wide_table(
    pool: &sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("CREATE SCHEMA IF NOT EXISTS sift_query_test")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS sift_query_test.wide")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE sift_query_test.wide (\
             id BIGINT PRIMARY KEY, flag BOOLEAN NOT NULL, count32 INTEGER NOT NULL, \
             ratio32 REAL NOT NULL, ratio64 DOUBLE PRECISION NOT NULL, title TEXT NOT NULL, \
             data BYTEA NOT NULL, day DATE NOT NULL, clock TIME NOT NULL, \
             local_at TIMESTAMP NOT NULL, instant TIMESTAMPTZ NOT NULL, \
             external UUID NOT NULL, amount NUMERIC NOT NULL, payload JSONB NOT NULL\
         )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO sift_query_test.wide VALUES \
         (1, true, 7, 1.5, 2.5, 'alpha', decode('fbff', 'hex'), DATE '2026-10-08', \
          TIME '12:34:56', TIMESTAMP '2026-10-08 12:34:56', \
          TIMESTAMPTZ '2026-10-08 04:34:56+00', \
          UUID '123e4567-e89b-12d3-a456-426614174000', 12.50, '{\"ok\":true}'::jsonb), \
         (2, false, 8, 3.5, 4.5, 'beta', decode('00', 'hex'), DATE '2026-10-09', \
          TIME '01:02:03', TIMESTAMP '2026-10-09 01:02:03', \
          TIMESTAMPTZ '2026-10-09 01:02:03+00', \
          UUID '123e4567-e89b-12d3-a456-426614174001', 20.00, '{\"ok\":false}'::jsonb)",
    )
    .execute(pool)
    .await?;
    Ok(())
}
