#![cfg(feature = "rls")]
#![allow(dead_code)]
extern crate sift_query_core as sift_query;
use futures_util::{StreamExt, TryStreamExt};
use sift_query_core::*;
use sift_query_derive::SearchEntity;
use sift_query_sqlx::*;
use std::time::Duration;

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_rls.items i",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
struct Item {
    #[search_field(column = "i.id", sortable)]
    id: i64,
    #[search_field(column = "i.tenant")]
    tenant: i64,
}
#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_rls.items i CROSS JOIN LATERAL pg_sleep(1)",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
struct Slow {
    #[search_field(column = "i.id", sortable)]
    id: i64,
}

#[test]
fn rls_configuration_is_bounded_and_redacts_context() {
    assert!(RlsConfig::new(&["statement_timeout"], &[]).is_err());
    assert!(RlsConfig::new(&["pg_bad.tenant"], &[]).is_err());
    assert!(RlsConfig::new(&["app.tenant", "app.tenant"], &[]).is_err());
    let config = RlsConfig::new(&["app.tenant"], &["reader"]).unwrap();
    assert!(config.context(&[], None).is_err());
    assert!(
        config
            .context(&[("app.tenant", "private-tenant")], Some("admin"))
            .is_err()
    );
    let context = config
        .context(&[("app.tenant", "private-tenant")], Some("reader"))
        .unwrap();
    assert!(!format!("{context:?}").contains("private-tenant"));
}

async fn assert_clean(pool: &sqlx::PgPool) {
    let (tenant, role): (Option<String>, String) =
        sqlx::query_as("SELECT nullif(current_setting('sift.tenant',true),''),current_user::text")
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(tenant.is_none());
    assert_ne!(role, "pg_read_all_data");
}

struct Secondary;
impl PgPoolSelector<()> for Secondary {
    fn select_pool(&self, _: &'static EntityMeta, (): &()) -> Result<String, PoolRouteError> {
        Ok("secondary".to_owned())
    }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL with a role allowed to SET ROLE pg_read_all_data"]
async fn rls_isolation_and_cleanup_against_postgres() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    sqlx::raw_sql("CREATE SCHEMA sift_rls; CREATE TABLE sift_rls.items(id bigint PRIMARY KEY,tenant bigint); INSERT INTO sift_rls.items VALUES(1,1),(2,2),(3,1); ALTER TABLE sift_rls.items ENABLE ROW LEVEL SECURITY; ALTER TABLE sift_rls.items FORCE ROW LEVEL SECURITY; CREATE POLICY tenant_policy ON sift_rls.items USING(tenant::text = current_setting('sift.tenant',true));").execute(&pool).await?;
    let config = RlsConfig::new(&["sift.tenant"], &["pg_read_all_data"])?;
    let first = config.context(&[("sift.tenant", "1")], Some("pg_read_all_data"))?;
    let second = config.context(&[("sift.tenant", "2")], Some("pg_read_all_data"))?;
    let none = config.context(&[("sift.tenant", "")], Some("pg_read_all_data"))?;
    let search = PgSiftQuery::new(pool.clone());
    let raw = RawParams::parse("")?;
    for (context, expected) in [(&first, vec![1, 3]), (&second, vec![2]), (&none, vec![])] {
        let page = search
            .search_rls::<Item>(context, &raw, &ServerScope::unrestricted())
            .await?;
        assert_eq!(
            page.data.iter().map(|row| row.id).collect::<Vec<_>>(),
            expected
        );
        assert_clean(&pool).await;
    }
    let scope = ServerScope::<Item>::new().eq(ItemFields::ID, 3).build()?;
    let other_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    let router = PgPoolRouter::with_searcher(search.clone())
        .register_named("secondary", PgSiftQuery::new(other_pool.clone()))?;
    let secondary_search = router.searcher_with_context::<Item, _>(&(), &Secondary)?;
    let unrestricted = ServerScope::unrestricted();
    let (a, b) = tokio::join!(
        search.search_rls::<Item>(&first, &raw, &unrestricted),
        secondary_search.search_rls::<Item>(&second, &raw, &unrestricted)
    );
    assert_eq!(a?.total_count, 2);
    assert_eq!(b?.total_count, 1);
    assert_clean(&pool).await;
    assert_clean(&other_pool).await;
    other_pool.close().await;
    assert_eq!(
        search.search_rls::<Item>(&first, &raw, &scope).await?.data[0].id,
        3
    );
    assert_clean(&pool).await;
    let mut transaction = search.begin_rls(&first, Duration::from_secs(5)).await?;
    assert!(
        transaction
            .search::<Item>(
                &RawParams::parse("unknown=1")?,
                &ServerScope::unrestricted()
            )
            .await
            .is_err()
    );
    drop(transaction);
    assert_clean(&pool).await;
    let slow = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            timeout: Duration::from_millis(50),
            ..PgExecutionPolicy::default()
        },
    )?;
    assert!(
        slow.search_rls::<Slow>(&first, &raw, &ServerScope::unrestricted())
            .await
            .is_err()
    );
    assert_clean(&pool).await;
    let cancel_scope = ServerScope::unrestricted();
    let cancel = search.search_rls::<Slow>(&first, &raw, &cancel_scope);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), cancel)
            .await
            .is_err()
    );
    assert_clean(&pool).await;
    let mut stream = search.scan_map_stream_rls::<Item>(
        &first,
        &raw,
        &ServerScope::unrestricted(),
        ScanPolicy::default(),
    )?;
    assert!(stream.next().await.unwrap().is_ok());
    drop(stream);
    assert_clean(&pool).await;
    let rows = search
        .scan_map_stream_rls::<Item>(
            &second,
            &raw,
            &ServerScope::unrestricted(),
            ScanPolicy::default(),
        )?
        .try_collect::<Vec<_>>()
        .await?;
    assert_eq!(rows.len(), 1);
    assert_clean(&pool).await;
    let mut csv = Vec::new();
    assert_eq!(
        CsvExporter::new(CsvExportPolicy::default())
            .export_rls::<Item, _>(
                &search,
                &first,
                &raw,
                &ServerScope::unrestricted(),
                &mut csv
            )
            .await?,
        2
    );
    assert!(!String::from_utf8(csv)?.contains("2,2"));
    assert_clean(&pool).await;
    let mut external = pool.begin().await?;
    first.apply_to(&mut external).await?;
    assert_eq!(
        search
            .search_on::<Item>(&mut external, &raw, &ServerScope::unrestricted())
            .await?
            .total_count,
        2
    );
    external.rollback().await?;
    assert_clean(&pool).await;
    sqlx::query("DROP SCHEMA sift_rls CASCADE")
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
