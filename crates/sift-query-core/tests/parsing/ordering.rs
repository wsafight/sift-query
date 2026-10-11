use super::*;

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn ordering_parameters_agree_on_the_resulting_order() {
    let by_sort = parse_rich("sort=name,id&order=desc,asc").unwrap();
    let by_order_by = parse_rich("orderBy=name:desc,id:asc").unwrap();
    assert_eq!(by_sort.order(), by_order_by.order());
    assert_eq!(by_sort.order()[0].direction, SortOrder::Desc);
}

#[test]
fn ordering_defaults_to_ascending_and_appends_stable_fields() {
    let query = parse_rich("sort=name").unwrap();
    assert_eq!(query.order()[0].direction, SortOrder::Asc);
    assert_eq!(query.order().len(), 2);
    assert_eq!(query.order()[1].field, sift_query_core::FieldId(0));

    // Requesting the stable field explicitly does not duplicate it.
    let explicit = parse_rich("sort=id").unwrap();
    assert_eq!(explicit.order().len(), 1);
}

#[test]
fn ordering_parameters_reject_bad_input() {
    assert_eq!(
        rich_error("sort=id&orderBy=id:asc"),
        ParamError::ConflictingParameters("sort/order and orderBy".to_owned())
    );
    assert_eq!(
        rich_error("order=desc"),
        ParamError::ConflictingParameters("order requires sort".to_owned())
    );
    assert_eq!(
        rich_error("sort=id,name&order=asc"),
        ParamError::ConflictingParameters("sort and order lengths differ".to_owned())
    );
    assert_eq!(
        rich_error("orderBy=id"),
        ParamError::InvalidValue {
            field: "orderBy".to_owned(),
            reason: "missing direction for id".to_owned(),
        }
    );
    assert_eq!(
        rich_error("orderBy=id:sideways"),
        ParamError::InvalidValue {
            field: "order".to_owned(),
            reason: "invalid direction sideways".to_owned(),
        }
    );
    assert_eq!(
        rich_error("sort=missing"),
        ParamError::UnknownField("missing".to_owned())
    );
    assert_eq!(
        rich_error("sort=computed"),
        ParamError::NotSortable("computed".to_owned())
    );
    assert_eq!(
        rich_error("sort=id,id"),
        ParamError::DuplicateParameter("sort id".to_owned())
    );
    assert_eq!(
        rich_error("sort=id%20name"),
        ParamError::InvalidValue {
            field: "sort".to_owned(),
            reason: "list must be non-empty and contain no whitespace".to_owned(),
        }
    );
}
