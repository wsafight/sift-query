extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;
use sift_query_mysql::{MySqlPoolRouter, MySqlSiftQuery, PoolRouteError, PoolSelector};
use sqlx::mysql::MySqlPoolOptions;
use std::sync::Arc;

#[derive(Debug, SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct Entity {
    #[search_field(sortable)]
    id: i64,
}

struct Selector;

impl PoolSelector<u8> for Selector {
    fn select_pool(
        &self,
        _: &'static sift_query::EntityMeta,
        context: &u8,
    ) -> Result<String, PoolRouteError> {
        Ok(if *context == 0 {
            String::new()
        } else {
            "archive".to_owned()
        })
    }
}

fn searcher() -> MySqlSiftQuery {
    MySqlSiftQuery::new(
        MySqlPoolOptions::new()
            .connect_lazy("mysql://root:root@localhost:1/unused")
            .unwrap(),
    )
}

#[tokio::test]
async fn named_router_routes_context_and_default_without_connecting() {
    let router = MySqlPoolRouter::default()
        .register_named("primary", searcher())
        .unwrap()
        .register_named("archive", searcher())
        .unwrap()
        .with_default_named("primary")
        .unwrap()
        .route_entity::<Entity>("primary")
        .unwrap();
    let selector = Selector;
    let default = router
        .searcher_with_context::<Entity, _>(&0, &selector)
        .unwrap();
    let archive = router
        .searcher_with_context::<Entity, _>(&1, &selector)
        .unwrap();
    assert!(!Arc::ptr_eq(&default, &archive));
    assert!(
        router
            .clone()
            .register_named("primary", searcher())
            .is_err()
    );
}
