#![allow(dead_code)]
extern crate sift_query_core as sift_query;
use sift_query::SearchEntity;
use sift_query_derive::SearchEntity;
use sift_query_sqlx::*;
use std::sync::Arc;

mod first {
    use super::*;
    #[derive(Debug, SearchEntity)]
    #[search_entity(stable_order = "id:asc")]
    pub struct Entity {
        #[search_field(sortable)]
        pub id: i64,
    }
}
mod second {
    use super::*;
    #[derive(Debug, SearchEntity)]
    #[search_entity(stable_order = "id:asc")]
    pub struct Entity {
        #[search_field(sortable)]
        pub id: i64,
    }
}

fn searcher() -> PgSiftQuery {
    PgSiftQuery::new(
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost:1/unused")
            .unwrap(),
    )
}

#[tokio::test]
async fn named_registry_validates_configuration_and_routes_by_type_identity() {
    let router = PgPoolRouter::default()
        .register_named("primary", searcher())
        .unwrap()
        .register_named("archive", searcher())
        .unwrap()
        .route_entity::<first::Entity>("primary")
        .unwrap()
        .route_entity::<second::Entity>("archive")
        .unwrap();
    assert_eq!(
        first::Entity::META.entity_name,
        second::Entity::META.entity_name
    );
    assert!(!Arc::ptr_eq(
        &router.searcher::<first::Entity>().unwrap(),
        &router.searcher::<second::Entity>().unwrap()
    ));
    assert!(matches!(
        router.clone().register_named("primary", searcher()),
        Err(PoolRouteError::DuplicateName(_))
    ));
    for name in ["", "../pool", "pool secret"] {
        assert!(matches!(
            router.clone().register_named(name, searcher()),
            Err(PoolRouteError::InvalidName)
        ));
    }
    assert!(matches!(
        router.clone().with_default_named("missing"),
        Err(PoolRouteError::MissingPool(_))
    ));
    assert!(matches!(
        router.route_entity::<first::Entity>("missing"),
        Err(PoolRouteError::MissingPool(_))
    ));
    assert!(matches!(
        PgPoolRouter::default().searcher::<first::Entity>(),
        Err(PoolRouteError::MissingEntity(_))
    ));
    let direct = PgPoolRouter::default()
        .register_entity::<first::Entity>(searcher())
        .register_entity::<second::Entity>(searcher());
    assert!(!Arc::ptr_eq(
        &direct.searcher::<first::Entity>().unwrap(),
        &direct.searcher::<second::Entity>().unwrap()
    ));
}
