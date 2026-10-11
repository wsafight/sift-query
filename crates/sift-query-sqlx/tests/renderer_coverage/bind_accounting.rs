use super::support::*;

// ---------------------------------------------------------------------------
// Bind accounting
// ---------------------------------------------------------------------------

#[test]
fn bind_values_follow_the_placeholder_order() {
    let rendered = PgRenderer
        .render_list(&plan::<User>(
            "name=A&age=1&age-op=ge&id=5",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    // Conditions render in field-id order, so the binds follow the entity
    // metadata rather than the order the parameters arrived in.
    assert_eq!(
        rendered.binds,
        vec![
            PgBindValue::Scalar(ScalarValue::I64(5)),
            PgBindValue::Scalar(ScalarValue::Text("A".to_owned())),
            PgBindValue::Scalar(ScalarValue::I32(1)),
            PgBindValue::Scalar(ScalarValue::I64(15)),
            PgBindValue::Scalar(ScalarValue::I64(0)),
        ]
    );
    assert!(
        rendered
            .sql
            .contains("u.id = $1 AND u.name = $2 AND u.age >= $3")
    );
}

#[test]
fn array_binds_carry_their_element_type() {
    let rendered = PgRenderer
        .render_list(&plan::<User>("id=1&id=2&id-op=il", QueryTarget::TypedPage))
        .unwrap();
    let PgBindValue::Array(list) = &rendered.binds[0] else {
        panic!("expected an array bind");
    };
    assert_eq!(list.element_type, ValueType::I64);
    assert_eq!(list.values.len(), 2);
}

#[test]
fn offset_overflow_is_reported_instead_of_rendered() {
    let mut broken = plan::<User>("", QueryTarget::TypedPage);
    broken.paging.offset = u64::MAX;
    assert_eq!(
        PgRenderer.render_list(&broken).unwrap_err(),
        RenderError::InvalidCondition("offset overflow".to_owned())
    );
}
