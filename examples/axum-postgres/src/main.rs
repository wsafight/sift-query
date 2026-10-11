use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sift_query::{Page, PgSiftQuery, SearchEntity, SearchParams, ServerScope, WebError};
use sqlx::{FromRow, PgPool};

#[derive(Debug, FromRow, SearchEntity, Serialize)]
#[search_entity(
    from = "app_user u",
    base_where = "u.deleted = false",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
struct User {
    #[search_field(column = "u.id", sortable, operators = [Eq, InList])]
    id: i64,
    #[search_field(column = "u.username", operators = [Eq, StartWith, Contain])]
    username: String,
    #[search_field(column = "u.status", operators = [Eq, InList])]
    status: i32,
}

async fn list_users(
    State(search): State<PgSiftQuery>,
    query: SearchParams,
) -> Result<Json<Page<User>>, WebError> {
    let scope = ServerScope::<User>::unrestricted();
    let page = search.search::<User>(query.raw(), &scope).await?;
    Ok(Json(page))
}

async fn query_description() -> Result<Json<serde_json::Value>, WebError> {
    Ok(Json(
        sift_query::metadata::openapi::<User>(&sift_query::QueryPolicy::default())
            .map_err(sift_query::SearchError::from)?,
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPool::connect(&database_url).await?;
    let app = Router::new()
        .route("/users", get(list_users).post(list_users))
        .route("/users/query-description", get(query_description))
        .with_state(PgSiftQuery::new(pool));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
