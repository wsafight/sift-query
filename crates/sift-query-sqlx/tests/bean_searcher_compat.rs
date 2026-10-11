extern crate sift_query_core as sift_query;
use sift_query::{ParamParser, QueryPlan, QueryPolicy, QueryTarget, RawParams, ServerScope};
use sift_query_derive::SearchEntity;
use sift_query_sqlx::PgRenderer;

#[derive(Debug, SearchEntity)]
#[allow(dead_code)]
#[search_entity(from = "app_user u", default_order = "id:asc", stable_order = "id:asc")]
struct User {
    #[search_field(column = "u.id", sortable)]
    id: i64,
    #[search_field(column = "u.name", sortable)]
    name: String,
}

#[derive(Debug, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "student_score s",
    default_order = "courseId:asc",
    stable_order = "courseId:asc"
)]
struct StudentCourse {
    #[search_field(
        name = "courseId",
        column = "s.course_id",
        kind = GroupKey,
        sortable
    )]
    course_id: i64,
    #[search_field(
        name = "avgScore",
        column = "avg(s.score::double precision)",
        kind = Aggregate,
        operators = [Eq, Gt]
    )]
    avg_score: f64,
}

fn plan<E: sift_query::SearchEntity>(query: &str, target: QueryTarget) -> QueryPlan<E> {
    let raw = RawParams::parse(query).unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy).unwrap().parse::<E>(&raw).unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), target).unwrap()
}

#[test]
fn preserves_multi_field_ordering_semantics() {
    let rendered = PgRenderer
        .render_list(&plan::<User>(
            "orderBy=id:asc,name:desc",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    assert!(
        rendered
            .sql
            .contains("ORDER BY u.id ASC NULLS LAST, u.name DESC NULLS FIRST")
    );
}

#[test]
fn routes_group_keys_to_where_and_aggregates_to_having() {
    let rendered = PgRenderer
        .render_list(&plan::<StudentCourse>(
            "courseId=5&avgScore=80&avgScore-op=gt",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    assert!(rendered.sql.contains("WHERE s.course_id = $1"));
    assert!(rendered.sql.contains("GROUP BY s.course_id"));
    assert!(
        rendered
            .sql
            .contains("HAVING avg(s.score::double precision) > $2")
    );
}

#[test]
fn applies_local_absorption_without_expanding_the_expression() {
    let rendered = PgRenderer
        .render_list(&plan::<User>(
            "A.id=1&B.name=Alice&gexpr=A%7C%28A%26B%29",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    assert!(rendered.sql.contains("WHERE u.id = $1"));
    assert!(!rendered.sql.contains("u.name ="));
    assert_eq!(rendered.binds.len(), 3);
}
