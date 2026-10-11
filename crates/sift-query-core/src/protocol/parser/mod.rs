use std::collections::BTreeMap;

use indexmap::IndexMap;
use indexmap::map::Entry;
use sift_query_algorithm::BoundedCounter;

mod group_expr;
mod options;

use crate::policy::names::is_name;
use crate::{
    CompatibilityPolicy, Condition, FieldKind, FieldMeta, Operator, Paging, ParamError,
    PredicateArena, PredicatePhase, QueryPolicy, RawParams, ScalarValue, SearchEntity, UserQuery,
    ValueType,
};
use group_expr::parse_group_expression;
use options::{parse_order, parse_paging, parse_projection};

/// Strict parser from [`RawParams`] to an entity-bound query.
pub struct ParamParser<'policy> {
    policy: &'policy QueryPolicy,
}

impl<'policy> ParamParser<'policy> {
    pub fn new(policy: &'policy QueryPolicy) -> Result<Self, ParamError> {
        Ok(Self { policy })
    }

    pub fn parse<E: SearchEntity>(&self, raw: &RawParams) -> Result<UserQuery<E>, ParamError> {
        validate_raw_limits(raw, self.policy)?;
        self.policy.validate_codecs::<E>()?;
        self.policy.validate_operators::<E>()?;
        let raw = self.policy.apply_raw_filters::<E>(raw)?;
        validate_raw_limits(&raw, self.policy)?;
        self.policy.validate_entity_names::<E>()?;
        let mut controls = Controls::default();
        let mut fields: IndexMap<ConditionKey<'_>, FieldAccumulator<'_>> = IndexMap::new();

        for (key, value) in raw.entries() {
            if controls.capture(self.policy, key, value)? {
                continue;
            }
            let parsed = parse_field_key(key)?;
            let Some(meta) = E::META.resolve_field(parsed.field) else {
                if self.policy.compatibility() == CompatibilityPolicy::BeanSearcher4812 {
                    continue;
                }
                return Err(ParamError::UnknownField(parsed.field.to_owned()));
            };
            let condition_key = ConditionKey {
                group: parsed.group,
                field: parsed.field,
            };
            let accumulator = match fields.entry(condition_key) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    let key = *entry.key();
                    entry.insert(FieldAccumulator::new(meta, key.group, key.field))
                }
            };
            accumulator.capture(parsed.component, value, self.policy)?;
        }

        let predicate = self.build_predicate::<E>(fields, controls.group_expr.as_deref())?;
        let projection = parse_projection::<E>(
            controls.only_select.as_deref(),
            controls.select_exclude.as_deref(),
        )?;
        let order = parse_order::<E>(
            self.policy,
            controls.sort.as_deref(),
            controls.order.as_deref(),
            controls.order_by.as_deref(),
        )?;
        let paging = parse_paging::<E>(
            self.policy,
            controls.page.as_deref(),
            controls.size.as_deref(),
            controls.offset.as_deref(),
            controls.cursor.as_deref(),
            &order,
        );
        validate_order_bind_budget::<E>(&predicate, &order, self.policy.limits())?;
        let (paging, force_empty) = match paging {
            Ok(paging) => (paging, false),
            Err(ParamError::PagingOutOfRange(_))
                if self.policy.compatibility() == CompatibilityPolicy::BeanSearcher4812 =>
            {
                (Paging::default(), true)
            }
            Err(error) => return Err(error),
        };

        Ok(UserQuery::from_parts(
            predicate,
            projection,
            order,
            paging,
            E::META.schema_fingerprint,
            self.policy.fingerprint(),
            force_empty,
        ))
    }

    fn build_predicate<'a, E: SearchEntity>(
        &self,
        accumulators: IndexMap<ConditionKey<'a>, FieldAccumulator<'a>>,
        group_expr: Option<&str>,
    ) -> Result<PredicateArena, ParamError> {
        let limits = self.policy.limits();
        if accumulators.len() > limits.max_conditions {
            return Err(ParamError::TooComplex("condition count"));
        }

        let mut logical_values = BoundedCounter::new(limits.max_logical_values);
        let mut bind_slots = 0usize;
        let mut root_conditions = Vec::new();
        let mut grouped_conditions: BTreeMap<String, Vec<Condition>> = BTreeMap::new();
        for (key, accumulator) in accumulators {
            let condition =
                accumulator.finish::<E>(limits.max_values_per_condition, self.policy)?;
            for value in &condition.values {
                logical_values
                    .try_add(value.logical_weight())
                    .map_err(|_| ParamError::TooComplex("logical value count"))?;
            }
            bind_slots += match condition.operator {
                Operator::IsNull
                | Operator::NotNull
                | Operator::Empty
                | Operator::NotEmpty
                | Operator::AlwaysTrue
                | Operator::AlwaysFalse => 0,
                Operator::InList | Operator::NotIn => 1,
                _ => condition.values.len(),
            };
            if bind_slots > limits.max_bind_slots {
                return Err(ParamError::TooComplex("database bind slot count"));
            }
            match key.group {
                Some(group) => grouped_conditions
                    .entry(group.to_owned())
                    .or_default()
                    .push(condition),
                None => root_conditions.push(condition),
            }
        }

        root_conditions.sort_by_key(|condition| condition.field);
        for conditions in grouped_conditions.values_mut() {
            conditions.sort_by_key(|condition| condition.field);
        }

        let root = conditions_to_and(root_conditions);
        let named = grouped_conditions
            .into_iter()
            .map(|(name, conditions)| (name, conditions_to_and(conditions)))
            .collect::<BTreeMap<_, _>>();

        let grouped = match group_expr {
            None => PredicateArena::combine_and(named.into_values()),
            Some(expression) => {
                if expression.len() > limits.max_group_expr_bytes {
                    return Err(ParamError::TooComplex("gexpr length"));
                }
                parse_group_expression(
                    expression,
                    &named,
                    limits.max_group_depth,
                    limits.max_group_nodes,
                )?
            }
        };

        let predicate = PredicateArena::combine_and([root, grouped]);
        predicate.partition()?;
        Ok(predicate)
    }
}

pub(crate) fn validate_order_bind_budget<E: SearchEntity>(
    predicate: &PredicateArena,
    order: &[crate::OrderBy],
    limits: &crate::QueryLimits,
) -> Result<(), ParamError> {
    let extra = order
        .iter()
        .filter(|order| E::condition_order(order.field).is_some())
        .count();
    if extra == 0 {
        return Ok(());
    }
    let binds = predicate
        .nodes()
        .iter()
        .filter_map(|node| match node {
            crate::PredicateNode::Condition(condition) => Some(
                if matches!(condition.operator, Operator::InList | Operator::NotIn) {
                    1
                } else {
                    condition.values.len()
                },
            ),
            _ => None,
        })
        .sum::<usize>();
    if binds.saturating_add(extra) > limits.max_bind_slots {
        return Err(ParamError::TooComplex("database bind slot count"));
    }
    Ok(())
}

fn validate_raw_limits(raw: &RawParams, policy: &QueryPolicy) -> Result<(), ParamError> {
    let limits = policy.limits();
    if raw.raw_bytes() > limits.max_raw_query_bytes {
        return Err(ParamError::QueryTooLong);
    }
    if raw.len() > limits.max_raw_items {
        return Err(ParamError::TooComplex("raw parameter item count"));
    }
    if raw.decoded_bytes() > limits.max_decoded_bytes {
        return Err(ParamError::TooComplex("decoded parameter bytes"));
    }
    if raw.entries().iter().any(|(key, value)| {
        key.len() > limits.max_key_bytes || value.len() > limits.max_value_bytes
    }) {
        return Err(ParamError::TooComplex("parameter key or value length"));
    }
    Ok(())
}

pub(crate) fn validate_entity_names<E: SearchEntity>(
    policy: &QueryPolicy,
) -> Result<(), ParamError> {
    for field in E::META.fields {
        if !is_name(field.public_name) || field.public_name.len() > 64 {
            return Err(ParamError::InvalidName(field.public_name.to_owned()));
        }
        if policy.names().all().contains(&field.public_name) {
            return Err(ParamError::ConflictingParameters(format!(
                "field {} conflicts with a control parameter",
                field.public_name
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct Controls {
    page: Option<String>,
    size: Option<String>,
    offset: Option<String>,
    cursor: Option<String>,
    sort: Option<String>,
    order: Option<String>,
    order_by: Option<String>,
    group_expr: Option<String>,
    only_select: Option<String>,
    select_exclude: Option<String>,
}

impl Controls {
    fn capture(
        &mut self,
        policy: &QueryPolicy,
        key: &str,
        value: &str,
    ) -> Result<bool, ParamError> {
        let names = policy.names();
        let target = if key == names.page {
            Some(&mut self.page)
        } else if key == names.size {
            Some(&mut self.size)
        } else if key == names.offset {
            Some(&mut self.offset)
        } else if key == names.cursor {
            Some(&mut self.cursor)
        } else if key == names.sort {
            Some(&mut self.sort)
        } else if key == names.order {
            Some(&mut self.order)
        } else if key == names.order_by {
            Some(&mut self.order_by)
        } else if key == names.group_expr {
            Some(&mut self.group_expr)
        } else if key == names.only_select {
            Some(&mut self.only_select)
        } else if key == names.select_exclude {
            Some(&mut self.select_exclude)
        } else {
            None
        };
        let Some(target) = target else {
            return Ok(false);
        };
        if target.is_some() {
            return Err(ParamError::DuplicateParameter(key.to_owned()));
        }
        *target = Some(value.to_owned());
        Ok(true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ConditionKey<'a> {
    group: Option<&'a str>,
    field: &'a str,
}

#[derive(Clone, Copy)]
enum FieldComponent {
    Value,
    Indexed(usize),
    Operator,
    IgnoreCase,
}

struct ParsedFieldKey<'a> {
    group: Option<&'a str>,
    field: &'a str,
    component: FieldComponent,
}

fn parse_field_key(key: &str) -> Result<ParsedFieldKey<'_>, ParamError> {
    let (base, component) = if let Some(base) = key.strip_suffix("-op") {
        (base, FieldComponent::Operator)
    } else if let Some(base) = key.strip_suffix("-ic") {
        (base, FieldComponent::IgnoreCase)
    } else if let Some((base, suffix)) = key.rsplit_once('-') {
        if suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            let index = suffix
                .parse()
                .map_err(|_| ParamError::InvalidName(key.to_owned()))?;
            if suffix.len() > 1 && suffix.starts_with('0') {
                return Err(ParamError::InvalidName(key.to_owned()));
            }
            (base, FieldComponent::Indexed(index))
        } else {
            return Err(ParamError::UnknownParameter(key.to_owned()));
        }
    } else {
        (key, FieldComponent::Value)
    };

    let (group, field) = if let Some((group, field)) = base.split_once('.') {
        if field.contains('.') {
            return Err(ParamError::InvalidName(key.to_owned()));
        }
        (Some(group), field)
    } else {
        (None, base)
    };
    if field.len() > 64
        || !is_name(field)
        || group
            .as_ref()
            .is_some_and(|name| name.len() > 64 || !is_name(name))
    {
        return Err(ParamError::InvalidName(key.to_owned()));
    }
    Ok(ParsedFieldKey {
        group,
        field,
        component,
    })
}

struct FieldAccumulator<'a> {
    meta: &'static FieldMeta,
    group: Option<&'a str>,
    field: &'a str,
    repeated: Vec<String>,
    indexed: BTreeMap<usize, String>,
    operator: Option<Operator>,
    ignore_case: Option<bool>,
}

impl<'a> FieldAccumulator<'a> {
    fn new(meta: &'static FieldMeta, group: Option<&'a str>, field: &'a str) -> Self {
        Self {
            meta,
            group,
            field,
            repeated: Vec::new(),
            indexed: BTreeMap::new(),
            operator: None,
            ignore_case: None,
        }
    }

    fn display(&self) -> String {
        format_display(self.group, self.field)
    }

    fn capture(
        &mut self,
        component: FieldComponent,
        value: &str,
        policy: &QueryPolicy,
    ) -> Result<(), ParamError> {
        match component {
            FieldComponent::Value => {
                if !self.indexed.is_empty() {
                    return Err(ParamError::ConflictingParameters(self.display()));
                }
                self.repeated.push(value.to_owned());
            }
            FieldComponent::Indexed(index) => {
                if !self.repeated.is_empty() {
                    return Err(ParamError::ConflictingParameters(self.display()));
                }
                if self.indexed.insert(index, value.to_owned()).is_some() {
                    return Err(ParamError::DuplicateParameter(format!(
                        "{}-{index}",
                        self.display()
                    )));
                }
            }
            FieldComponent::Operator => {
                let operator = Operator::parse(value)
                    .or_else(|| {
                        policy
                            .registered_operator(value)
                            .map(|registration| Operator::Custom(registration.operator))
                    })
                    .ok_or_else(|| ParamError::InvalidValue {
                        field: self.display(),
                        reason: format!("unknown operator {value}"),
                    })?;
                if self.operator.replace(operator).is_some() {
                    return Err(ParamError::DuplicateParameter(format!(
                        "{}-op",
                        self.display()
                    )));
                }
            }
            FieldComponent::IgnoreCase => {
                let ignore_case = match value {
                    "true" => true,
                    "false" => false,
                    _ => {
                        return Err(ParamError::InvalidValue {
                            field: self.display(),
                            reason: "ignore-case must be true or false".to_owned(),
                        });
                    }
                };
                if self.ignore_case.replace(ignore_case).is_some() {
                    return Err(ParamError::DuplicateParameter(format!(
                        "{}-ic",
                        self.display()
                    )));
                }
            }
        }
        Ok(())
    }

    fn finish<E: SearchEntity>(
        self,
        max_values: usize,
        policy: &QueryPolicy,
    ) -> Result<Condition, ParamError> {
        let group = self.group;
        let field = self.field;
        if !self.meta.filterable {
            return Err(ParamError::NotFilterable(format_display(group, field)));
        }
        let operator = self.operator.unwrap_or(Operator::Eq);
        let allowed = match operator {
            Operator::Custom(custom) => {
                E::custom_operator_names(self.meta.id).contains(&custom.name())
                    && policy
                        .registered_operator(custom.name())
                        .is_some_and(|item| {
                            item.operator == custom && item.field_type == self.meta.value_type
                        })
            }
            builtin => self.meta.operators.contains(builtin),
        };
        if !allowed {
            return Err(ParamError::OperatorNotAllowed {
                field: format_display(group, field),
                operator: operator.short_name(),
            });
        }
        let raw_values = if self.indexed.is_empty() {
            self.repeated
        } else {
            let mut values = Vec::with_capacity(self.indexed.len());
            for (expected, (actual, value)) in self.indexed.into_iter().enumerate() {
                if actual != expected {
                    return Err(ParamError::NonContiguousIndex {
                        field: format_display(group, field),
                        expected,
                    });
                }
                values.push(value);
            }
            values
        };
        if raw_values.len() > max_values {
            return Err(ParamError::TooComplex("values per condition"));
        }
        if !operator.accepts_arity(raw_values.len()) {
            return Err(ParamError::WrongArity {
                field: format_display(group, field),
                operator: operator.short_name(),
                expected: operator.expected_arity(),
                actual: raw_values.len(),
            });
        }
        let ignore_case = self.ignore_case.unwrap_or(false);
        if ignore_case
            && (self.meta.value_type != ValueType::Text || !operator.supports_ignore_case())
        {
            return Err(ParamError::IgnoreCaseNotAllowed(format_display(
                group, field,
            )));
        }
        let input_type = operator.input_type(self.meta.value_type);
        let input_field = FieldMeta {
            value_type: input_type,
            ..*self.meta
        };
        let values = raw_values
            .iter()
            .map(|value| {
                if let Some(codec) = policy.codec_for::<E>(self.meta) {
                    let parsed = codec.parse(&input_field, value).map_err(|reason| {
                        ParamError::InvalidValue {
                            field: format_display(group, field),
                            reason,
                        }
                    })?;
                    if parsed.value_type() != input_type {
                        return Err(ParamError::InvalidValue {
                            field: format_display(group, field),
                            reason: "codec returned a value with the wrong field type".to_owned(),
                        });
                    }
                    if matches!(&parsed, ScalarValue::F32(value) if !value.is_finite())
                        || matches!(&parsed, ScalarValue::F64(value) if !value.is_finite())
                    {
                        return Err(ParamError::InvalidValue {
                            field: format_display(group, field),
                            reason: "codec returned a non-finite value".to_owned(),
                        });
                    }
                    Ok(parsed)
                } else {
                    ScalarValue::parse(input_type, value, self.field).map_err(|error| match error {
                        ParamError::InvalidValue { reason, .. } => ParamError::InvalidValue {
                            field: format_display(group, field),
                            reason,
                        },
                        other => other,
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values
            .iter()
            .map(ScalarValue::logical_weight)
            .sum::<usize>()
            > max_values
        {
            return Err(ParamError::TooComplex("values per condition"));
        }
        for value in &values {
            value.validate()?;
        }
        Ok(Condition {
            field: self.meta.id,
            operator,
            values,
            ignore_case,
            phase: if self.meta.kind == FieldKind::Aggregate {
                PredicatePhase::Group
            } else {
                PredicatePhase::Row
            },
        })
    }
}

fn format_display(group: Option<&str>, field: &str) -> String {
    group.map_or_else(|| field.to_owned(), |group| format!("{group}.{field}"))
}

fn conditions_to_and(conditions: Vec<Condition>) -> PredicateArena {
    PredicateArena::combine_and(conditions.into_iter().map(PredicateArena::from_condition))
}
