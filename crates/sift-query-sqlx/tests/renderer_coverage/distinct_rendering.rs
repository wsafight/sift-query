use super::support::*;
use super::targets_and_projections::NO_ALIAS_META;

// ---------------------------------------------------------------------------
// DISTINCT rendering
// ---------------------------------------------------------------------------

#[test]
fn distinct_lists_render_the_distinct_keyword() {
    let rendered = PgRenderer
        .render_list(&plan::<DistinctUser>("", QueryTarget::TypedPage))
        .unwrap();
    assert!(rendered.sql.starts_with("SELECT DISTINCT u.id AS \"id\""));
}

#[test]
fn distinct_clusters_project_the_summarized_field_inside_the_subquery() {
    let id = FieldRef::<DistinctUser, i64, ScalarKind>::new(FieldId(0));
    let options = SearchOptions::<DistinctUser>::builder()
        .sum(id)
        .build()
        .unwrap();
    let rendered = PgRenderer
        .render_cluster(
            &plan::<DistinctUser>("", QueryTarget::Count),
            Some(&options),
        )
        .unwrap();
    assert_eq!(
        rendered.sql.as_ref(),
        "SELECT count(*) AS \"sq_count\", sum(sq_0) AS \"sq_0\" \
         FROM (SELECT DISTINCT u.id, u.name, u.age, u.id AS \"sq_0\" FROM app_user u \
         WHERE u.deleted = false) sq_cluster"
    );
}

#[test]
fn distinct_summaries_require_the_field_in_the_final_projection() {
    let name = FieldRef::<DistinctUser, String, ScalarKind>::new(FieldId(1));
    let options = SearchOptions::<DistinctUser>::builder()
        .min(name)
        .build()
        .unwrap();
    let mut broken = plan::<DistinctUser>("", QueryTarget::Count);
    broken.projection = vec![FieldId(0)];
    assert_eq!(
        PgRenderer
            .render_cluster(&broken, Some(&options))
            .unwrap_err(),
        RenderError::InvalidCondition(
            "DISTINCT summaries require the summarized field in the final projection".to_owned()
        )
    );
}

#[test]
fn sum_over_distinct_wraps_the_distinct_projection_in_a_subquery() {
    let rendered = PgRenderer
        .render_sum(&plan::<DistinctUser>("", QueryTarget::Sum), FieldId(0))
        .unwrap();
    assert_eq!(
        rendered.sql.as_ref(),
        "SELECT sum(\"id\") AS \"sq_sum\" FROM (SELECT DISTINCT u.id AS \"id\", u.name AS \"name\", \
         u.age AS \"age\" FROM app_user u WHERE u.deleted = false) sq_sum_source"
    );
}

#[test]
fn sum_over_distinct_requires_the_field_in_the_final_projection() {
    let mut broken = plan::<DistinctUser>("", QueryTarget::Sum);
    broken.projection = vec![FieldId(1)];
    assert_eq!(
        PgRenderer.render_sum(&broken, FieldId(0)).unwrap_err(),
        RenderError::InvalidCondition(
            "SUM over DISTINCT entities requires the field in the final projection".to_owned()
        )
    );
}

static NO_ALIAS_DISTINCT_META: EntityMeta = EntityMeta {
    distinct: true,
    schema_fingerprint: 36,
    ..NO_ALIAS_META
};
struct NoAliasDistinct;
impl SearchEntity for NoAliasDistinct {
    const META: &'static EntityMeta = &NO_ALIAS_DISTINCT_META;
}

#[test]
fn sum_over_distinct_requires_a_result_alias() {
    assert_eq!(
        PgRenderer
            .render_sum(&plan::<NoAliasDistinct>("", QueryTarget::Sum), FieldId(0))
            .unwrap_err(),
        RenderError::MissingAlias("id".to_owned())
    );
}

#[test]
fn sum_over_grouped_and_plain_entities_renders_each_shape() {
    let plain = PgRenderer
        .render_sum(&plan::<User>("", QueryTarget::Sum), FieldId(2))
        .unwrap();
    assert_eq!(
        plain.sql.as_ref(),
        "SELECT sum(u.age) AS \"sq_sum\" FROM app_user u WHERE u.deleted = false"
    );

    let grouped = PgRenderer
        .render_sum(&plan::<Score>("", QueryTarget::Sum), FieldId(1))
        .unwrap();
    assert!(grouped.sql.contains(
        "SELECT sum(\"sq_value\") AS \"sq_sum\" FROM (SELECT sum(s.score) AS \"sq_value\""
    ));
    assert!(grouped.sql.contains("GROUP BY s.course_id"));
    assert!(grouped.sql.contains("HAVING count(*) > 0"));

    assert_eq!(
        PgRenderer
            .render_sum(&plan::<User>("", QueryTarget::Sum), FieldId(99))
            .unwrap_err(),
        RenderError::UnknownField(99)
    );
}
