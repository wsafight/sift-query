use sift_query_core::{
    Condition, NodeId, Operator, PredicateArena, PredicateNode, ScalarValue, TypedList,
};

use super::{Dialect, RenderError, State};

pub(super) fn render_arena(
    meta: &'static sift_query_core::EntityMeta,
    arena: &PredicateArena,
    state: &mut State,
    dialect: Dialect,
) -> Result<Option<String>, RenderError> {
    arena
        .root()
        .map(|root| render_node(meta, arena, root, state, dialect))
        .transpose()
}

fn render_node(
    meta: &'static sift_query_core::EntityMeta,
    arena: &PredicateArena,
    id: NodeId,
    state: &mut State,
    dialect: Dialect,
) -> Result<String, RenderError> {
    match arena.node(id).ok_or(RenderError::InvalidCondition(
        "predicate node is missing".to_owned(),
    ))? {
        PredicateNode::Condition(condition) => render_condition(meta, condition, state, dialect),
        PredicateNode::And(children) | PredicateNode::Or(children) => {
            let separator = if matches!(arena.node(id), Some(PredicateNode::And(_))) {
                " AND "
            } else {
                " OR "
            };
            let mut output = String::new();
            output.push('(');
            for (index, child) in children.iter().enumerate() {
                if index > 0 {
                    output.push_str(separator);
                }
                output.push_str(&render_node(meta, arena, *child, state, dialect)?);
            }
            output.push(')');
            Ok(output)
        }
    }
}

fn render_condition(
    meta: &'static sift_query_core::EntityMeta,
    condition: &Condition,
    state: &mut State,
    _dialect: Dialect,
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
    let binary = |operator: &str, state: &mut State| -> Result<String, RenderError> {
        Ok(format!("{column} {operator} {}", state.bind(one()?)))
    };
    match condition.operator {
        Operator::Eq if condition.ignore_case => {
            Ok(format!("LOWER({column}) = LOWER({})", state.bind(one()?)))
        }
        Operator::Ne if condition.ignore_case => {
            Ok(format!("LOWER({column}) <> LOWER({})", state.bind(one()?)))
        }
        Operator::Eq => binary("=", state),
        Operator::Ne => binary("<>", state),
        Operator::Gt => binary(">", state),
        Operator::Ge => binary(">=", state),
        Operator::Lt => binary("<", state),
        Operator::Le => binary("<=", state),
        Operator::Between | Operator::NotBetween => {
            if condition.values.len() != 2 {
                return Err(RenderError::InvalidCondition(format!(
                    "{} needs two values",
                    field.public_name
                )));
            }
            let left = state.bind(condition.values[0].clone());
            let right = state.bind(condition.values[1].clone());
            let op = if condition.operator == Operator::Between {
                "BETWEEN"
            } else {
                "NOT BETWEEN"
            };
            Ok(format!("{column} {op} {left} AND {right}"))
        }
        Operator::InList | Operator::NotIn => {
            let list = TypedList::new(field.value_type, condition.values.clone())
                .map_err(|e| RenderError::InvalidCondition(e.to_string()))?;
            let values = state.bind_list(list);
            let op = if condition.operator == Operator::InList {
                "IN"
            } else {
                "NOT IN"
            };
            Ok(format!("{column} {op} ({values})"))
        }
        Operator::IsNull => Ok(format!("{column} IS NULL")),
        Operator::NotNull => Ok(format!("{column} IS NOT NULL")),
        Operator::Empty => Ok(format!("({column} IS NULL OR {column} = '')")),
        Operator::NotEmpty => Ok(format!("({column} IS NOT NULL AND {column} <> '')")),
        Operator::AlwaysTrue => Ok("1 = 1".to_owned()),
        Operator::AlwaysFalse => Ok("1 = 0".to_owned()),
        Operator::Custom(operator) => {
            let mut output = operator.template().replace("{field}", column);
            for (index, value) in condition.values.iter().enumerate() {
                output = output.replace(&format!("{{{index}}}"), state.bind(value.clone()));
            }
            Ok(output)
        }
        Operator::StartWith
        | Operator::EndWith
        | Operator::Contain
        | Operator::NotLike
        | Operator::OrLike => {
            let values =
                if condition.operator == Operator::OrLike {
                    &condition.values[..]
                } else {
                    std::slice::from_ref(condition.values.first().ok_or_else(|| {
                        RenderError::InvalidCondition("missing pattern".to_owned())
                    })?)
                };
            let mut parts = Vec::new();
            for value in values {
                let ScalarValue::Text(value) = value else {
                    return Err(RenderError::InvalidCondition(format!(
                        "{} needs text",
                        field.public_name
                    )));
                };
                let pattern = match condition.operator {
                    Operator::StartWith => format!("{}%", escape_like(value)),
                    Operator::EndWith => format!("%{}", escape_like(value)),
                    Operator::Contain => format!("%{}%", escape_like(value)),
                    _ => escape_like(value),
                };
                let placeholder = state.bind(ScalarValue::Text(pattern));
                let like = if condition.operator == Operator::NotLike {
                    "NOT LIKE"
                } else {
                    "LIKE"
                };
                let mut op = if condition.ignore_case {
                    format!("LOWER({column}) {like} LOWER({placeholder})")
                } else {
                    format!("{column} {like} {placeholder}")
                };
                // A single ASCII escape character avoids dependence on
                // MySQL's NO_BACKSLASH_ESCAPES SQL mode.
                op.push_str(" ESCAPE '!'");
                parts.push(op);
            }
            if condition.operator == Operator::OrLike {
                Ok(format!("({})", parts.join(" OR ")))
            } else {
                Ok(parts.remove(0))
            }
        }
        _ => Err(RenderError::InvalidCondition(
            "operator is not supported by this SQL dialect".to_owned(),
        )),
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('!', "!!")
        .replace('%', "!%")
        .replace('_', "!_")
}

pub(crate) fn like_pattern(value: &str, operator: Operator) -> String {
    match operator {
        Operator::StartWith => format!("{}%", escape_like(value)),
        Operator::EndWith => format!("%{}", escape_like(value)),
        Operator::Contain => format!("%{}%", escape_like(value)),
        _ => escape_like(value),
    }
}
