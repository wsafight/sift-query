use super::*;

// ---------------------------------------------------------------------------
// Projection
// ---------------------------------------------------------------------------

#[test]
fn projection_parameters_map_to_field_ids_in_metadata_order() {
    let query = parse_rich("onlySelect=age,id").unwrap();
    assert_eq!(
        query.projection(),
        &sift_query_core::Projection::Fields(vec![
            sift_query_core::FieldId(0),
            sift_query_core::FieldId(2)
        ])
    );

    let excluded = parse_rich("selectExclude=name,age").unwrap();
    assert_eq!(
        excluded.projection(),
        &sift_query_core::Projection::Fields(vec![
            sift_query_core::FieldId(0),
            sift_query_core::FieldId(3),
            sift_query_core::FieldId(4),
            sift_query_core::FieldId(6)
        ])
    );
}

#[test]
fn projection_parameters_reject_bad_input() {
    assert_eq!(
        rich_error("onlySelect=id&selectExclude=name"),
        ParamError::ConflictingParameters("onlySelect and selectExclude".to_owned())
    );
    assert_eq!(
        rich_error("onlySelect=id,id"),
        ParamError::DuplicateParameter("id".to_owned())
    );
    assert_eq!(
        rich_error("onlySelect=missing"),
        ParamError::UnknownField("missing".to_owned())
    );
    assert_eq!(
        rich_error("onlySelect=secret"),
        ParamError::NotSelectable("secret".to_owned())
    );
    assert_eq!(
        rich_error("onlySelect=id,name,"),
        ParamError::InvalidValue {
            field: "projection".to_owned(),
            reason: "list contains an empty item".to_owned(),
        }
    );
    assert_eq!(
        rich_error("onlySelect="),
        ParamError::InvalidValue {
            field: "projection".to_owned(),
            reason: "list must be non-empty and contain no whitespace".to_owned(),
        }
    );
    assert_eq!(
        rich_error("selectExclude=id,name,age,active,score,computed"),
        ParamError::EmptyProjection
    );
}
