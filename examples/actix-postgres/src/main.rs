use actix_web::{App, HttpServer, route, web};
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

#[route("/users", method = "GET", method = "POST")]
async fn list_users(
    query: SearchParams,
    search: web::Data<PgSiftQuery>,
) -> Result<web::Json<Page<User>>, WebError> {
    let page = search
        .search::<User>(query.raw(), &ServerScope::unrestricted())
        .await?;
    Ok(web::Json(page))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|error| std::io::Error::other(error.to_string()))?;
    let pool = PgPool::connect_lazy(&database_url)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let search = web::Data::new(PgSiftQuery::new(pool));
    HttpServer::new(move || App::new().app_data(search.clone()).service(list_users))
        .bind(("127.0.0.1", 3001))?
        .run()
        .await
}
