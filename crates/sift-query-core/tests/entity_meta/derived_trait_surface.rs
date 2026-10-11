use super::queryplan_for_projection::Summary;
use super::*;

// ---------------------------------------------------------------------------
// Derived trait surface
// ---------------------------------------------------------------------------

#[test]
fn field_references_are_copyable_and_debuggable() {
    let original = FieldRef::<RichEntity, i64, sift_query_core::ScalarKind>::new(FieldId(3));
    let copied = original;
    let cloned = original;
    assert_eq!(copied.id(), FieldId(3));
    assert_eq!(cloned.id(), FieldId(3));
    assert_eq!(format!("{original:?}"), "FieldRef(FieldId(3))");
}

#[test]
fn filter_only_markers_are_default_copyable_and_debuggable() {
    use sift_query_core::FilterOnly;
    let marker = FilterOnly::<i64>::default();
    let copied = marker;
    let cloned = marker;
    assert_eq!(format!("{marker:?}"), "FilterOnly");
    assert_eq!(format!("{copied:?}"), "FilterOnly");
    assert_eq!(format!("{cloned:?}"), "FilterOnly");
}

#[test]
fn search_projection_cursor_values_default_to_none() {
    let projection = Summary;
    assert_eq!(projection.cursor_value(FieldId(0)), None);
    // The default trait method on the entity side does the same.
    assert_eq!(RichEntity.cursor_value(FieldId(0)), None);
}
