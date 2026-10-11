use super::support::*;
use super::wide_support::*;

/// Exercises scalar and array binding for every supported PostgreSQL type.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn binds_every_scalar_and_array_type() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    let _lock = serialise(&pool).await?;
    create_wide_table(&pool).await?;
    let search = PgSiftQuery::new(pool.clone());
    let scope = ServerScope::<Wide>::unrestricted();

    // Scalar equality for every type.
    let scalar_queries = [
        "id=1",
        "flag=true",
        "count32=7",
        "ratio32=1.5",
        "ratio64=2.5",
        "title=alpha",
        "data=-_8",
        "day=2026-10-08",
        "clock=12%3A34%3A56",
        "local_at=2026-10-08T12%3A34%3A56",
        "instant=2026-10-08T04%3A34%3A56Z",
        "external=123e4567-e89b-12d3-a456-426614174000",
        "amount=12.50",
    ];
    for query in scalar_queries {
        let found = search
            .search_list::<Wide>(&RawParams::parse(&format!("{query}&size=10"))?, &scope)
            .await?;
        assert_eq!(found.len(), 1, "query {query}");
        assert_eq!(found[0].id, 1, "query {query}");
    }

    // Array binding for every array-capable type.
    let array_queries = [
        "id=1&id=2&id-op=il",
        "flag=true&flag=false&flag-op=il",
        "count32=7&count32=8&count32-op=il",
        "ratio32=1.5&ratio32=3.5&ratio32-op=il",
        "ratio64=2.5&ratio64=4.5&ratio64-op=il",
        "title=alpha&title=beta&title-op=il",
        "day=2026-10-08&day=2026-10-09&day-op=il",
        "clock=12%3A34%3A56&clock=01%3A02%3A03&clock-op=il",
        "local_at=2026-10-08T12%3A34%3A56&local_at=2026-10-09T01%3A02%3A03&local_at-op=il",
        "instant=2026-10-08T04%3A34%3A56Z&instant=2026-10-09T01%3A02%3A03Z&instant-op=il",
        "external=123e4567-e89b-12d3-a456-426614174000&external=123e4567-e89b-12d3-a456-426614174001&external-op=il",
        "amount=12.50&amount=20.00&amount-op=il",
    ];
    for query in array_queries {
        let found = search
            .search_list::<Wide>(&RawParams::parse(&format!("{query}&size=10"))?, &scope)
            .await?;
        assert_eq!(found.len(), 2, "query {query}");
    }

    // NOT IN and NOT BETWEEN round out the negative array forms.
    let none = search
        .search_list::<Wide>(&RawParams::parse("id=1&id=2&id-op=ni&size=10")?, &scope)
        .await?;
    assert!(none.is_empty());

    sqlx::query("DROP TABLE sift_query_test.wide")
        .execute(&pool)
        .await?;
    Ok(())
}
