//! Rendering of the predicate arena into a SQL boolean expression.
//!
//! Split out of [`super`] so that the operator table and the tree walk stay
//! readable; the surrounding query assembly lives in the parent module.

use sift_query_core::{
    Condition, NodeId, Operator, PredicateArena, PredicateNode, ScalarValue, TypedList,
};

use super::RenderState;
use super::types::supports_array;
use crate::RenderError;

fn render_condition(
    meta: &'static sift_query_core::EntityMeta,
    condition: &Condition,
    state: &mut RenderState,
) -> Result<String, RenderError> {
    let field = meta
        .field(condition.field)
        .ok_or(RenderError::UnknownField(condition.field.0))?;
    let column = field.column;
    let one = || {
        condition.values.first().cloned().ok_or_else(|| {
            RenderError::InvalidCondition(format!("{} needs one value", field.public_name))
        })
    };
    let binary = |operator: &str, state: &mut RenderState| -> Result<String, RenderError> {
        let placeholder = state.bind(one()?);
        Ok(format!("{column} {operator} {placeholder}"))
    };
    match condition.operator {
        Operator::Eq if condition.ignore_case => {
            let placeholder = state.bind(one()?);
            Ok(format!("lower({column}) = lower({placeholder})"))
        }
        Operator::Ne if condition.ignore_case => {
            let placeholder = state.bind(one()?);
            Ok(format!("lower({column}) <> lower({placeholder})"))
        }
        Operator::Eq => binary("=", state),
        Operator::Ne => binary("<>", state),
        Operator::Gt => binary(">", state),
        Operator::Ge => binary(">=", state),
        Operator::Lt => binary("<", state),
        Operator::Le => binary("<=", state),
        Operator::Between => {
            if condition.values.len() != 2 {
                return Err(RenderError::InvalidCondition(format!(
                    "{} needs two values",
                    field.public_name
                )));
            }
            let lower = state.bind(condition.values[0].clone());
            let upper = state.bind(condition.values[1].clone());
            Ok(format!("{column} BETWEEN {lower} AND {upper}"))
        }
        Operator::NotBetween => {
            if condition.values.len() != 2 {
                return Err(RenderError::InvalidCondition(format!(
                    "{} needs two values",
                    field.public_name
                )));
            }
            let lower = state.bind(condition.values[0].clone());
            let upper = state.bind(condition.values[1].clone());
            Ok(format!("{column} NOT BETWEEN {lower} AND {upper}"))
        }
        Operator::InList | Operator::NotIn => {
            if !supports_array(field.value_type) {
                return Err(RenderError::UnsupportedBindType(format!(
                    "{:?}[]",
                    field.value_type
                )));
            }
            let list = TypedList::new(field.value_type, condition.values.clone())
                .map_err(|error| RenderError::InvalidCondition(error.to_string()))?;
            let placeholder = state.bind_array(list);
            if condition.operator == Operator::InList {
                Ok(format!("{column} = ANY({placeholder})"))
            } else {
                Ok(format!("{column} <> ALL({placeholder})"))
            }
        }
        Operator::IsNull => Ok(format!("{column} IS NULL")),
        Operator::NotNull => Ok(format!("{column} IS NOT NULL")),
        Operator::Empty => Ok(format!("({column} IS NULL OR {column} = '')")),
        Operator::NotEmpty => Ok(format!("({column} IS NOT NULL AND {column} <> '')")),
        Operator::AlwaysTrue => Ok("1 = 1".to_owned()),
        Operator::AlwaysFalse => Ok("1 = 0".to_owned()),
        Operator::Custom(operator) => {
            let mut rendered = operator.template().replace("{field}", column);
            for (index, value) in condition.values.iter().enumerate() {
                let mut placeholder = state.bind(value.clone());
                if operator.types().is_some() {
                    placeholder.push_str("::");
                    placeholder.push_str(super::types::bind_type(value.value_type())?);
                }
                rendered = rendered.replace(&format!("{{{index}}}"), &placeholder);
            }
            Ok(rendered)
        }
        Operator::StartWith | Operator::EndWith | Operator::Contain => {
            let ScalarValue::Text(value) = one()? else {
                return Err(RenderError::InvalidCondition(format!(
                    "{} needs a text value",
                    field.public_name
                )));
            };
            let pattern = like_pattern(&value, condition.operator);
            let placeholder = state.bind(ScalarValue::Text(pattern));
            let operator = if condition.ignore_case {
                "ILIKE"
            } else {
                "LIKE"
            };
            Ok(format!("{column} {operator} {placeholder} ESCAPE '\\'"))
        }
        Operator::NotLike => {
            let ScalarValue::Text(value) = one()? else {
                return Err(RenderError::InvalidCondition(format!(
                    "{} needs a text pattern",
                    field.public_name
                )));
            };
            let placeholder = state.bind(ScalarValue::Text(value));
            let operator = if condition.ignore_case {
                "NOT ILIKE"
            } else {
                "NOT LIKE"
            };
            Ok(format!("{column} {operator} {placeholder}"))
        }
        Operator::OrLike => {
            let operator = if condition.ignore_case {
                "ILIKE"
            } else {
                "LIKE"
            };
            let mut parts = Vec::with_capacity(condition.values.len());
            for value in &condition.values {
                let ScalarValue::Text(value) = value else {
                    return Err(RenderError::InvalidCondition(format!(
                        "{} needs text patterns",
                        field.public_name
                    )));
                };
                let placeholder = state.bind(ScalarValue::Text(value.clone()));
                parts.push(format!("{column} {operator} {placeholder}"));
            }
            Ok(format!("({})", parts.join(" OR ")))
        }
        _ => Err(RenderError::InvalidCondition(
            "operator is not supported by the PostgreSQL renderer".to_owned(),
        )),
    }
}

pub(crate) fn like_pattern(input: &str, operator: Operator) -> String {
    let mut output = String::with_capacity(input.len() + 2);
    if matches!(operator, Operator::EndWith | Operator::Contain) {
        output.push('%');
    }
    for character in input.chars() {
        if matches!(character, '%' | '_' | '\\') {
            output.push('\\');
        }
        output.push(character);
    }
    if matches!(operator, Operator::StartWith | Operator::Contain) {
        output.push('%');
    }
    output
}

pub(super) fn render_arena(
    meta: &'static sift_query_core::EntityMeta,
    arena: &PredicateArena,
    state: &mut RenderState,
) -> Result<Option<String>, RenderError> {
    arena
        .root()
        .map(|root| render_node(meta, arena, root, state))
        .transpose()
}

fn render_node(
    meta: &'static sift_query_core::EntityMeta,
    arena: &PredicateArena,
    id: NodeId,
    state: &mut RenderState,
) -> Result<String, RenderError> {
    match arena.node(id).ok_or(RenderError::InvalidCondition(
        "predicate node is missing".to_owned(),
    ))? {
        PredicateNode::Condition(condition) => render_condition(meta, condition, state),
        PredicateNode::And(children) | PredicateNode::Or(children) => {
            let separator = if matches!(arena.node(id), Some(PredicateNode::And(_))) {
                " AND "
            } else {
                " OR "
            };
            let mut rendered = Vec::with_capacity(children.len());
            for child in children {
                rendered.push(render_node(meta, arena, *child, state)?);
            }
            Ok(format!("({})", rendered.join(separator)))
        }
    }
}
