use std::collections::BTreeSet;
use std::marker::PhantomData;

use crate::{
    Condition, Cursor, FieldId, FieldKind, FieldRef, FilterExpr, NullOrder, Operator, OrderBy,
    Paging, PagingMode, ParamError, PredicateArena, PredicatePhase, Projection, QueryPolicy,
    ScalarValue, SearchEntity, SortOrder, UserQuery,
};

mod operators;

enum ProjectionBuilder {
    All,
    Only(Vec<FieldId>),
    Exclude(Vec<FieldId>),
}

/// Programmatic builder for a validated [`UserQuery`].
pub struct QueryBuilder<'policy, E: SearchEntity> {
    pub(crate) policy: &'policy QueryPolicy,
    conditions: Vec<Condition>,
    expressions: Vec<FilterExpr<E>>,
    projection: ProjectionBuilder,
    order: Vec<OrderBy>,
    paging: Paging,
    error: Option<ParamError>,
    _entity: PhantomData<fn() -> E>,
}

impl<'policy, E: SearchEntity> QueryBuilder<'policy, E> {
    #[must_use]
    pub fn new(policy: &'policy QueryPolicy) -> Self {
        Self {
            policy,
            conditions: Vec::new(),
            expressions: Vec::new(),
            projection: ProjectionBuilder::All,
            order: Vec::new(),
            paging: Paging::default(),
            error: None,
            _entity: PhantomData,
        }
    }

    /// Adds a pre-built nested predicate. Multiple calls are combined with AND.
    #[must_use]
    pub fn filter(mut self, expression: FilterExpr<E>) -> Self {
        if self.error.is_none() {
            self.expressions.push(expression);
        }
        self
    }

    #[must_use]
    pub fn only_select<T, K>(mut self, field: FieldRef<E, T, K>) -> Self {
        self.push_projection(field.id(), false);
        self
    }

    #[must_use]
    pub fn exclude<T, K>(mut self, field: FieldRef<E, T, K>) -> Self {
        self.push_projection(field.id(), true);
        self
    }

    #[must_use]
    pub fn order_by<T, K>(mut self, field: FieldRef<E, T, K>, direction: SortOrder) -> Self {
        if self.error.is_some() {
            return self;
        }
        let Some(meta) = E::META.field(field.id()) else {
            self.error = Some(ParamError::UnknownField(format!("#{}", field.id().0)));
            return self;
        };
        if !meta.sortable {
            self.error = Some(ParamError::NotSortable(meta.public_name.to_owned()));
        } else if self.order.iter().any(|item| item.field == field.id()) {
            self.error = Some(ParamError::DuplicateParameter(format!(
                "sort {}",
                meta.public_name
            )));
        } else {
            self.order.push(OrderBy {
                field: field.id(),
                direction,
                nulls: NullOrder::from(direction),
            });
        }
        self
    }

    #[must_use]
    pub fn page(mut self, page: u64, size: u32) -> Self {
        if self.error.is_some() {
            return self;
        }
        let Some(offset) = page
            .checked_sub(1)
            .and_then(|page| page.checked_mul(u64::from(size)))
        else {
            self.error = Some(ParamError::PagingOutOfRange("page".to_owned()));
            return self;
        };
        self.set_paging(PagingMode::Page { page }, size, offset, true, false);
        self
    }

    #[must_use]
    pub fn offset(mut self, offset: u64, size: u32) -> Self {
        if self.error.is_some() {
            return self;
        }
        self.set_paging(PagingMode::Offset, size, offset, false, true);
        self
    }

    #[must_use]
    pub fn cursor(mut self, cursor: Cursor, size: u32) -> Self {
        if self.error.is_some() {
            return self;
        }
        let max_size = self
            .policy
            .limits()
            .max_page_size
            .min(E::META.limits.max_page_size);
        if size > max_size || cursor.values().len() != self.final_order_len() {
            self.error = Some(ParamError::PagingOutOfRange("cursor".to_owned()));
        } else {
            self.paging = Paging {
                mode: PagingMode::Cursor(Some(cursor)),
                size,
                offset: 0,
                explicit_page: false,
                explicit_size: true,
                explicit_offset: false,
                explicit_cursor: true,
            };
        }
        self
    }

    pub fn build(mut self) -> Result<UserQuery<E>, ParamError> {
        self.policy.validate_codecs::<E>()?;
        self.policy.validate_operators::<E>()?;
        if let Some(error) = self.error {
            return Err(error);
        }
        crate::policy::budget::bounded_sum(
            std::iter::once(self.conditions.len()).chain(
                self.expressions
                    .iter()
                    .map(|expression| expression.condition_count),
            ),
            self.policy.limits().max_conditions,
            "condition count",
        )?;
        crate::policy::budget::bounded_sum(
            self.conditions
                .iter()
                .flat_map(|condition| &condition.values)
                .map(ScalarValue::logical_weight)
                .chain(
                    self.expressions
                        .iter()
                        .map(|expression| expression.logical_values),
                ),
            self.policy.limits().max_logical_values,
            "logical value count",
        )?;
        if self.order.len() > self.policy.limits().max_sort_fields {
            return Err(ParamError::TooComplex("sort field count"));
        }
        if self.order.is_empty() {
            self.order
                .extend(E::META.default_order.iter().copied().map(OrderBy::from));
        }
        if E::META.stable_order.is_empty() {
            return Err(ParamError::UnstablePagination);
        }
        for stable in E::META.stable_order {
            if !self.order.iter().any(|item| item.field == stable.field) {
                self.order.push((*stable).into());
            }
        }
        let predicate = PredicateArena::combine_and(
            self.conditions
                .into_iter()
                .map(PredicateArena::from_condition)
                .chain(
                    self.expressions
                        .into_iter()
                        .map(|expression| expression.arena),
                ),
        );
        predicate.partition()?;
        crate::protocol::parser::validate_order_bind_budget::<E>(
            &predicate,
            &self.order,
            self.policy.limits(),
        )?;
        for node in predicate.nodes() {
            if let crate::PredicateNode::Condition(condition) = node
                && condition
                    .values
                    .iter()
                    .map(ScalarValue::logical_weight)
                    .sum::<usize>()
                    > self.policy.limits().max_values_per_condition
            {
                return Err(ParamError::TooComplex("values per condition"));
            }
        }
        let projection = resolve_projection::<E>(self.projection)?;
        if let PagingMode::Cursor(Some(cursor)) = &self.paging.mode {
            for (order, value) in self.order.iter().zip(cursor.values()) {
                if E::cursor_key_non_null(order.field) && matches!(value, crate::CursorValue::Null)
                {
                    return Err(ParamError::InvalidValue {
                        field: "cursor".to_owned(),
                        reason: "non-null cursor key cannot contain NULL".to_owned(),
                    });
                }
            }
        }
        Ok(UserQuery::from_parts(
            predicate,
            projection,
            self.order,
            self.paging,
            E::META.schema_fingerprint,
            self.policy.fingerprint(),
            false,
        ))
    }

    fn push_condition(
        mut self,
        field: FieldId,
        operator: Operator,
        values: Vec<ScalarValue>,
        ignore_case: bool,
    ) -> Self {
        if self.error.is_some() {
            return self;
        }
        let Some(meta) = E::META.field(field) else {
            self.error = Some(ParamError::UnknownField(format!("#{}", field.0)));
            return self;
        };
        if !meta.filterable {
            self.error = Some(ParamError::NotFilterable(meta.public_name.to_owned()));
        } else if !matches!(operator, Operator::Custom(_)) && !meta.operators.contains(operator) {
            self.error = Some(ParamError::OperatorNotAllowed {
                field: meta.public_name.to_owned(),
                operator: operator.short_name(),
            });
        } else if !operator.accepts_arity(values.len()) {
            self.error = Some(ParamError::WrongArity {
                field: meta.public_name.to_owned(),
                operator: operator.short_name(),
                expected: operator.expected_arity(),
                actual: values.len(),
            });
        } else if values
            .iter()
            .map(ScalarValue::logical_weight)
            .sum::<usize>()
            > self.policy.limits().max_values_per_condition
        {
            self.error = Some(ParamError::TooComplex("values per condition"));
        } else if !operator.accepts_field_type(meta.value_type)
            || values
                .iter()
                .any(|value| value.value_type() != operator.input_type(meta.value_type))
        {
            self.error = Some(ParamError::InvalidValue {
                field: meta.public_name.to_owned(),
                reason: "programmatic value type does not match field".to_owned(),
            });
        } else if ignore_case
            && (meta.value_type != crate::ValueType::Text || !operator.supports_ignore_case())
        {
            self.error = Some(ParamError::IgnoreCaseNotAllowed(
                meta.public_name.to_owned(),
            ));
        } else {
            if let Some(error) = values.iter().find_map(|value| value.validate().err()) {
                self.error = Some(error);
                return self;
            }
            self.conditions.push(Condition {
                field,
                operator,
                values,
                ignore_case,
                phase: if meta.kind == FieldKind::Aggregate {
                    PredicatePhase::Group
                } else {
                    PredicatePhase::Row
                },
            });
        }
        self
    }

    fn push_projection(&mut self, field: FieldId, excluding: bool) {
        if self.error.is_some() {
            return;
        }
        let Some(meta) = E::META.field(field) else {
            self.error = Some(ParamError::UnknownField(format!("#{}", field.0)));
            return;
        };
        if !meta.selectable || meta.result_alias.is_none() {
            self.error = Some(ParamError::NotSelectable(meta.public_name.to_owned()));
            return;
        }
        // The first call picks the projection mode; later calls append to it.
        // Resolving `All` before taking a mutable borrow keeps the match below
        // down to one reachable arm plus the conflict arm, with no
        // `unreachable!` to justify.
        if matches!(self.projection, ProjectionBuilder::All) {
            self.projection = if excluding {
                ProjectionBuilder::Exclude(Vec::new())
            } else {
                ProjectionBuilder::Only(Vec::new())
            };
        }
        let ((ProjectionBuilder::Only(fields), false) | (ProjectionBuilder::Exclude(fields), true)) =
            (&mut self.projection, excluding)
        else {
            self.error = Some(ParamError::ConflictingParameters(
                "only-select and exclude projection".to_owned(),
            ));
            return;
        };
        if fields.contains(&field) {
            self.error = Some(ParamError::DuplicateParameter(format!(
                "projection {}",
                meta.public_name
            )));
        } else {
            fields.push(field);
        }
    }

    fn set_paging(
        &mut self,
        mode: PagingMode,
        size: u32,
        offset: u64,
        explicit_page: bool,
        explicit_offset: bool,
    ) {
        let max_size = self
            .policy
            .limits()
            .max_page_size
            .min(E::META.limits.max_page_size);
        let max_offset = self
            .policy
            .limits()
            .max_offset
            .min(E::META.limits.max_offset);
        if size > max_size || offset > max_offset {
            self.error = Some(ParamError::PagingOutOfRange("page".to_owned()));
        } else {
            self.paging = Paging {
                mode,
                size,
                offset,
                explicit_page,
                explicit_size: true,
                explicit_offset,
                explicit_cursor: false,
            };
        }
    }

    fn final_order_len(&self) -> usize {
        let requested = if self.order.is_empty() {
            E::META
                .default_order
                .iter()
                .map(|item| item.field)
                .collect()
        } else {
            self.order.iter().map(|item| item.field).collect::<Vec<_>>()
        };
        let missing_stable = E::META
            .stable_order
            .iter()
            .filter(|stable| !requested.contains(&stable.field))
            .count();
        requested.len() + missing_stable
    }
}

fn resolve_projection<E: SearchEntity>(
    projection: ProjectionBuilder,
) -> Result<Projection, ParamError> {
    let fields = match projection {
        ProjectionBuilder::All => return Ok(Projection::All),
        ProjectionBuilder::Only(fields) => {
            let included = fields.into_iter().collect::<BTreeSet<_>>();
            E::META
                .fields
                .iter()
                .filter(|field| included.contains(&field.id))
                .map(|field| field.id)
                .collect::<Vec<_>>()
        }
        ProjectionBuilder::Exclude(fields) => {
            let excluded = fields.into_iter().collect::<BTreeSet<_>>();
            E::META
                .fields
                .iter()
                .filter(|field| {
                    field.selectable
                        && field.result_alias.is_some()
                        && !excluded.contains(&field.id)
                })
                .map(|field| field.id)
                .collect::<Vec<_>>()
        }
    };
    if fields.is_empty() {
        Err(ParamError::EmptyProjection)
    } else {
        Ok(Projection::Fields(fields))
    }
}
