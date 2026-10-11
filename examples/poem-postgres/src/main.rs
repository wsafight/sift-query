use poem::listener::TcpListener;
use poem::web::{Data, Json};
use poem::{EndpointExt, Route, Server, get, handler};
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

#[handler]
async fn list_users(
    query: SearchParams,
    Data(search): Data<&PgSiftQuery>,
) -> poem::Result<Json<Page<User>>> {
    let page = search
        .search::<User>(query.raw(), &ServerScope::unrestricted())
        .await
        .map_err(WebError::from)?;
    Ok(Json(page))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPool::connect_lazy(&database_url)?;
    let app = Route::new()
        .at("/users", get(list_users).post(list_users))
        .data(PgSiftQuery::new(pool));
    Server::new(TcpListener::bind("127.0.0.1:3002"))
        .run(app)
        .await?;
    Ok(())
}
