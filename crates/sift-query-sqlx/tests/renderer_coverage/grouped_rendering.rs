use super::support::*;

// ---------------------------------------------------------------------------
// Grouped rendering
// ---------------------------------------------------------------------------

#[test]
fn grouped_lists_render_group_by_and_having() {
    let rendered = PgRenderer
        .render_list(&plan::<Score>(
            "courseId=1&total=10&total-op=ge",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    assert!(rendered.sql.contains("GROUP BY s.course_id"));
    assert!(
        rendered
            .sql
            .contains("HAVING count(*) > 0 AND sum(s.score) >= $2")
    );
}

#[test]
fn grouped_counts_use_a_subquery_with_the_fixed_having_clause() {
    let rendered = PgRenderer
        .render_count(&plan::<Score>("", QueryTarget::Count))
        .unwrap();
    assert_eq!(
        rendered.sql.as_ref(),
        "SELECT count(*) AS \"sq_count\" FROM (SELECT 1 FROM student_score s \
         GROUP BY s.course_id HAVING count(*) > 0) sq_cluster"
    );
}

#[test]
fn grouped_clusters_with_summaries_project_each_requested_field() {
    let age = FieldRef::<Score, i64, ScalarKind>::new(FieldId(1));
    let options = SearchOptions::<Score>::builder()
        .sum(age)
        .max(age)
        .build()
        .unwrap();
    let rendered = PgRenderer
        .render_cluster(&plan::<Score>("", QueryTarget::Count), Some(&options))
        .unwrap();
    assert_eq!(
        rendered.sql.as_ref(),
        "SELECT count(*) AS \"sq_count\", sum(sq_0) AS \"sq_0\", max(sq_1) AS \"sq_1\" \
         FROM (SELECT sum(s.score) AS \"sq_0\", sum(s.score) AS \"sq_1\" FROM student_score s \
         GROUP BY s.course_id HAVING count(*) > 0) sq_cluster"
    );
}

#[test]
fn clustered_summaries_without_a_grouping_select_count_only() {
    let rendered = PgRenderer
        .render_cluster(&plan::<User>("", QueryTarget::Count), None)
        .unwrap();
    assert_eq!(
        rendered.sql.as_ref(),
        "SELECT count(*) AS \"sq_count\" FROM app_user u WHERE u.deleted = false"
    );
}

#[test]
fn unsupported_summary_types_are_rejected_before_rendering() {
    // `sum` over text has no PostgreSQL aggregate.
    let title = FieldRef::<Typed, String, ScalarKind>::new(FieldId(5));
    let options = SearchOptions::<Typed>::builder()
        .sum(title)
        .build()
        .unwrap();
    assert_eq!(
        PgRenderer
            .render_cluster(&plan::<Typed>("", QueryTarget::Count), Some(&options))
            .unwrap_err(),
        RenderError::InvalidCondition(
            "summary field type does not support the PostgreSQL aggregate".to_owned()
        )
    );
}

#[test]
fn every_summary_function_renders_its_own_aggregate() {
    let custom =
        CustomAggregate::new("spread", "max({field}) - min({field})", ValueType::I32).unwrap();
    let age = FieldRef::<User, i32, ScalarKind>::new(FieldId(2));
    let options = SearchOptions::<User>::builder()
        .sum(age)
        .avg(age)
        .min(age)
        .max(age)
        .custom(age, custom)
        .build()
        .unwrap();
    let rendered = PgRenderer
        .render_cluster(&plan::<User>("", QueryTarget::Count), Some(&options))
        .unwrap();
    for (index, fragment) in [
        "sum(u.age)",
        "avg(u.age)",
        "min(u.age)",
        "max(u.age)",
        "max(u.age) - min(u.age)",
    ]
    .iter()
    .enumerate()
    {
        assert!(
            rendered
                .sql
                .contains(&format!("{fragment} AS \"sq_{index}\"")),
            "missing {fragment} at {index}: {}",
            rendered.sql
        );
    }
}
