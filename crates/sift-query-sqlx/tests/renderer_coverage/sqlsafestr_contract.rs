use super::support::*;

// ---------------------------------------------------------------------------
// SqlSafeStr contract
// ---------------------------------------------------------------------------

#[test]
fn user_values_are_never_interpolated_into_the_sql_text() {
    // `PgSiftQuery` hands sqlx an `AssertSqlSafe` string, so the renderer must
    // never place a user-supplied value in the SQL text itself. Each case below
    // carries a payload that would be visible if it were concatenated.
    let payloads = ["1 OR 1=1", "'; DROP TABLE users; --", "%", "_", "\\"];
    for payload in payloads {
        let encoded = payload
            .bytes()
            .map(|byte| match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    char::from(byte).to_string()
                }
                other => format!("%{other:02X}"),
            })
            .collect::<String>();

        for query in [
            format!("name={encoded}"),
            format!("name={encoded}&name-op=ct"),
            format!("name={encoded}&name-op=sw"),
            format!("name={encoded}&name-op=ew"),
            format!("name={encoded}&name-op=nk"),
            format!("name={encoded}&name-op=ol"),
            format!("name={encoded}&name-op=eq&name-ic=true"),
        ] {
            let rendered = PgRenderer
                .render_list(&plan::<User>(&query, QueryTarget::TypedPage))
                .unwrap();
            // The payload must reach the database as a bind value...
            let carried = rendered.binds.iter().any(|bind| match bind {
                PgBindValue::Scalar(ScalarValue::Text(value)) => {
                    value.contains(payload) || value.contains("\\%") || value.contains("\\_")
                }
                PgBindValue::Scalar(_) | PgBindValue::Array(_) => false,
            });
            assert!(carried, "payload {payload:?} was not bound for {query}");

            // ...and never appear in the SQL text, not even escaped.
            assert!(
                !rendered.sql.contains("DROP") && !rendered.sql.contains("OR 1=1"),
                "payload leaked into SQL for {query}: {}",
                rendered.sql
            );
        }
    }
}

#[test]
fn identifier_like_values_stay_out_of_the_sql_text() {
    // Even a value that looks like a column reference is bound, not inlined.
    let rendered = PgRenderer
        .render_list(&plan::<User>(
            "name=u.id&name-op=eq",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    assert!(rendered.sql.contains("u.name = $1"), "{}", rendered.sql);
    assert_eq!(
        rendered.binds[0],
        PgBindValue::Scalar(ScalarValue::Text("u.id".to_owned()))
    );
    // `u.id` appears in the SQL only as the projected column and the compared
    // column; the user's text is carried by the bind, not the SQL.
    assert_eq!(rendered.sql.matches("u.id").count(), 2);
    assert!(!rendered.sql.contains("u.name = u.id"));
}

#[test]
fn placeholders_are_numbered_in_emission_order() {
    // sqlx 0.9 uses `SqlSafeStr` for the text but still relies on PostgreSQL
    // `$n` placeholders matching the bind order exactly.
    let rendered = PgRenderer
        .render_list(&plan::<User>(
            "id=1&name=a&name-op=ct&id-op=ge",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    let placeholders = rendered
        .sql
        .match_indices('$')
        .map(|(index, _)| {
            let rest = &rendered.sql[index + 1..];
            let digits = rest
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            format!("${digits}")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        placeholders,
        vec!["$1", "$2", "$3", "$4"],
        "{}",
        rendered.sql
    );
    assert_eq!(rendered.binds.len(), 4);
}
