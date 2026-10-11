use super::queryplan_new::plan_for;
use super::*;

// ---------------------------------------------------------------------------
// QueryPlan::for_projection
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(super) struct Summary;

impl sift_query_core::SearchProjection<RichEntity> for Summary {
    const FIELDS: &'static [sift_query_core::ProjectionField] = &[
        sift_query_core::ProjectionField {
            source: FieldId(0),
            result_alias: "id",
        },
        sift_query_core::ProjectionField {
            source: FieldId(1),
            result_alias: "display",
        },
    ];
}

#[derive(Debug)]
struct EmptySummary;

impl sift_query_core::SearchProjection<RichEntity> for EmptySummary {
    const FIELDS: &'static [sift_query_core::ProjectionField] = &[];
}

#[derive(Debug)]
struct DuplicateSummary;

impl sift_query_core::SearchProjection<RichEntity> for DuplicateSummary {
    const FIELDS: &'static [sift_query_core::ProjectionField] = &[
        sift_query_core::ProjectionField {
            source: FieldId(0),
            result_alias: "a",
        },
        sift_query_core::ProjectionField {
            source: FieldId(0),
            result_alias: "b",
        },
    ];
}

#[derive(Debug)]
struct UnknownSummary;

impl sift_query_core::SearchProjection<RichEntity> for UnknownSummary {
    const FIELDS: &'static [sift_query_core::ProjectionField] =
        &[sift_query_core::ProjectionField {
            source: FieldId(99),
            result_alias: "nope",
        }];
}

#[derive(Debug)]
struct UnaliasedSummary;

impl sift_query_core::SearchProjection<RichEntity> for UnaliasedSummary {
    const FIELDS: &'static [sift_query_core::ProjectionField] =
        &[sift_query_core::ProjectionField {
            source: FieldId(5),
            result_alias: "secret",
        }];
}

fn projection_plan<P: sift_query_core::SearchProjection<RichEntity>>(
    query: &str,
) -> Result<QueryPlan<RichEntity>, PlanError> {
    let raw = RawParams::parse(query).unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<RichEntity>(&raw)
        .unwrap();
    QueryPlan::for_projection::<P>(&query, &ServerScope::unrestricted())
}

#[test]
fn static_projections_replace_the_derived_projection_and_aliases() {
    let plan = projection_plan::<Summary>("").unwrap();
    assert_eq!(plan.target, QueryTarget::StaticPage);
    assert_eq!(plan.projection, vec![FieldId(0), FieldId(1)]);
    assert_eq!(plan.projection_aliases, Some(vec!["id", "display"]));
}

#[test]
fn static_projections_reject_empty_duplicate_unknown_and_unaliased_fields() {
    assert_eq!(
        projection_plan::<EmptySummary>("").unwrap_err(),
        PlanError::Param(ParamError::EmptyProjection)
    );
    assert_eq!(
        projection_plan::<DuplicateSummary>("").unwrap_err(),
        PlanError::Param(ParamError::DuplicateParameter("id".to_owned()))
    );
    assert_eq!(
        projection_plan::<UnknownSummary>("").unwrap_err(),
        PlanError::Param(ParamError::UnknownField("#99".to_owned()))
    );
    assert_eq!(
        projection_plan::<UnaliasedSummary>("").unwrap_err(),
        PlanError::Param(ParamError::NotSelectable("secret".to_owned()))
    );
}

#[test]
fn static_projections_reject_a_dynamic_user_projection() {
    assert_eq!(
        projection_plan::<Summary>("onlySelect=name").unwrap_err(),
        PlanError::Param(ParamError::ProjectionNotAllowed)
    );
}

#[test]
fn static_projections_keep_distinct_ordering_fields_in_view() {
    // `DistinctEntity` orders by `id`, which `Summary` does not select.
    let raw = RawParams::parse("").unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<DistinctEntity>(&raw)
        .unwrap();

    assert_eq!(
        QueryPlan::for_projection::<NameOnly>(&query, &ServerScope::unrestricted()).unwrap_err(),
        PlanError::Param(ParamError::DistinctOrderNotProjected("id".to_owned()))
    );
}

/// Selects only `name`, omitting the distinct ordering field.
#[derive(Debug)]
struct NameOnly;

impl sift_query_core::SearchProjection<DistinctEntity> for NameOnly {
    const FIELDS: &'static [sift_query_core::ProjectionField] =
        &[sift_query_core::ProjectionField {
            source: FieldId(1),
            result_alias: "name",
        }];
}

#[test]
fn unstable_entities_reject_every_parse() {
    let raw = RawParams::parse("").unwrap();
    assert_eq!(
        ParamParser::new(&QueryPolicy::default())
            .unwrap()
            .parse::<UnstableEntity>(&raw)
            .unwrap_err(),
        ParamError::UnstablePagination
    );
}

#[test]
fn query_projection_is_dynamic_only_for_explicit_field_lists() {
    assert!(!Projection::All.is_dynamic());
    assert!(Projection::Fields(vec![FieldId(0)]).is_dynamic());
}

#[test]
fn order_meta_direction_survives_the_plan() {
    let plan = plan_for::<RichEntity>("sort=name&order=desc", QueryTarget::TypedPage).unwrap();
    assert_eq!(plan.order[0].direction, SortOrder::Desc);
    assert_eq!(plan.order.last().unwrap().field, FieldId(0));
    let _ = OrderMeta {
        field: FieldId(0),
        direction: SortOrder::Asc,
    };
}
