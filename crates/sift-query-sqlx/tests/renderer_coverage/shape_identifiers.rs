use super::support::*;

// ---------------------------------------------------------------------------
// Shape identifiers
// ---------------------------------------------------------------------------

#[test]
fn shape_ids_ignore_bind_values_but_track_the_policy() {
    let one = PgRenderer
        .render_list(&plan::<User>("id=1", QueryTarget::TypedPage))
        .unwrap();
    let two = PgRenderer
        .render_list(&plan::<User>("id=2", QueryTarget::TypedPage))
        .unwrap();
    assert_eq!(one.shape_id, two.shape_id);
    assert_eq!(one.sql, two.sql);

    let strict = PgRenderer
        .render_list(&plan_with::<User>(
            "id=1",
            QueryTarget::TypedPage,
            &QueryPolicy::default(),
        ))
        .unwrap();
    let compatible = PgRenderer
        .render_list(&plan_with::<User>(
            "id=1",
            QueryTarget::TypedPage,
            &QueryPolicy::bean_searcher_4812(),
        ))
        .unwrap();
    assert_ne!(strict.shape_id, compatible.shape_id);
}

#[test]
fn window_count_support_is_target_and_shape_sensitive() {
    assert!(PgRenderer.supports_window_count(&plan::<User>("", QueryTarget::TypedPage)));
    assert!(!PgRenderer.supports_window_count(&plan::<User>("", QueryTarget::Count)));
    assert!(!PgRenderer.supports_window_count(&plan::<DistinctUser>("", QueryTarget::TypedPage)));
    assert!(!PgRenderer.supports_window_count(&plan::<Score>("", QueryTarget::TypedPage)));

    let mut zero = plan::<User>("", QueryTarget::TypedPage);
    zero.paging.size = 0;
    assert!(!PgRenderer.supports_window_count(&zero));
}
