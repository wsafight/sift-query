//! Canonical, value-only RPC parameters. Authorization scopes are not inputs.
use crate::{
    NodeId, PagingMode, ParamError, ParamNames, ParamParser, PredicateArena, PredicateNode,
    Projection, QueryBuilder, QueryInput, QueryPolicy, RawParams, SearchEntity, SortOrder,
    UserQuery,
};

impl<E: SearchEntity> UserQuery<E> {
    pub fn to_rpc(&self, policy: &QueryPolicy) -> Result<RawParams, ParamError> {
        self.to_rpc_with_names(policy, policy.names())
    }

    /// Same query semantics, using the remote service's control names.
    pub fn to_rpc_with_names(
        &self,
        policy: &QueryPolicy,
        names: &ParamNames,
    ) -> Result<RawParams, ParamError> {
        self.into_user_query(policy)?;
        let target = policy
            .clone()
            .with_param_names(names.clone())
            .map_err(|error| ParamError::ConflictingParameters(error.to_string()))?;
        let mut entries = Vec::new();
        if let Some(root) = self.predicate().root() {
            let expression = encode_node::<E>(self.predicate(), root, policy, &mut entries, None)?;
            entries.push((names.group_expr.clone(), expression));
        }
        if let Projection::Fields(fields) = self.projection() {
            entries.push((
                names.only_select.clone(),
                fields
                    .iter()
                    .map(|id| E::META.field(*id).expect("validated field").public_name)
                    .collect::<Vec<_>>()
                    .join(","),
            ));
        }
        // Emit the shortest prefix that reconstructs the final stable order.
        // Appended stable keys do not consume the explicit-sort budget.
        for length in 0..=self.order().len() {
            let mut order = if length == 0 {
                E::META
                    .default_order
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect::<Vec<_>>()
            } else {
                self.order()[..length].to_vec()
            };
            for stable in E::META.stable_order {
                if !order.iter().any(|item| item.field == stable.field) {
                    order.push((*stable).into());
                }
            }
            if order == self.order() {
                if length > 0 {
                    entries.push((
                        names.order_by.clone(),
                        self.order()[..length]
                            .iter()
                            .map(|order| {
                                format!(
                                    "{}:{}",
                                    E::META
                                        .field(order.field)
                                        .expect("validated order")
                                        .public_name,
                                    if order.direction == SortOrder::Asc {
                                        "asc"
                                    } else {
                                        "desc"
                                    }
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(","),
                    ));
                }
                break;
            }
        }
        let paging = self.paging();
        if self.is_forced_empty() {
            entries.push((
                names.size.clone(),
                (u64::from(
                    policy
                        .limits()
                        .max_page_size
                        .min(E::META.limits.max_page_size),
                ) + 1)
                    .to_string(),
            ));
        } else {
            if paging.explicit_size || paging.size != 15 {
                entries.push((names.size.clone(), paging.size.to_string()));
            }
            match &paging.mode {
                PagingMode::Page { page } if paging.explicit_page => {
                    entries.push((names.page.clone(), page.to_string()));
                }
                PagingMode::Offset => {
                    entries.push((names.offset.clone(), paging.offset.to_string()));
                }
                PagingMode::Cursor(cursor) => {
                    let token = match cursor {
                        None => String::new(),
                        Some(cursor) => policy
                            .cursor_codec()
                            .ok_or_else(|| invalid("cursor codec missing"))?
                            .encode::<E>(self.order(), cursor.values())?,
                    };
                    entries.push((names.cursor.clone(), token));
                }
                PagingMode::Page { .. } => {}
            }
        }
        let raw = RawParams::from_entries(entries, target.limits())?;
        // Full parser/normalizer validation also catches filters that would
        // change canonical RPC values. Never silently send a different query.
        let decoded = ParamParser::new(&target)?.parse::<E>(&raw)?;
        if signature(self.predicate()) != signature(decoded.predicate())
            || self.order() != decoded.order()
            || self.projection() != decoded.projection()
            || self.paging() != decoded.paging()
            || self.is_forced_empty() != decoded.is_forced_empty()
        {
            return Err(invalid("RPC normalization changes query semantics"));
        }
        Ok(raw)
    }
}

impl<E: SearchEntity> QueryBuilder<'_, E> {
    pub fn build_for_rpc(self) -> Result<RawParams, ParamError> {
        let policy = self.policy;
        self.build()?.to_rpc(policy)
    }
    pub fn build_for_rpc_with_names(self, names: &ParamNames) -> Result<RawParams, ParamError> {
        let policy = self.policy;
        self.build()?.to_rpc_with_names(policy, names)
    }
}

fn encode_node<E: SearchEntity>(
    arena: &PredicateArena,
    id: NodeId,
    policy: &QueryPolicy,
    entries: &mut Vec<(String, String)>,
    parent_is_and: Option<bool>,
) -> Result<String, ParamError> {
    match arena.node(id).expect("validated predicate") {
        PredicateNode::Condition(condition) => {
            let group = format!("q{}", id.0);
            let field = E::META.field(condition.field).expect("validated field");
            let key = format!("{group}.{}", field.public_name);
            entries.push((
                format!("{key}-op"),
                condition.operator.short_name().to_owned(),
            ));
            if condition.ignore_case {
                entries.push((format!("{key}-ic"), "true".to_owned()));
            }
            for value in &condition.values {
                let input_field = crate::FieldMeta {
                    value_type: condition.operator.input_type(field.value_type),
                    ..*field
                };
                let text = if let Some(codec) = policy.codec_for::<E>(field) {
                    let text = codec.format_input(&input_field, value).map_err(|reason| {
                        ParamError::InvalidValue {
                            field: field.public_name.to_owned(),
                            reason,
                        }
                    })?;
                    if codec.parse(&input_field, &text).ok().as_ref() != Some(value) {
                        return Err(invalid("codec has no lossless RPC representation"));
                    }
                    text
                } else {
                    value.to_external_text()
                };
                entries.push((key.clone(), text));
            }
            Ok(group)
        }
        PredicateNode::And(children) | PredicateNode::Or(children) => {
            let and = matches!(arena.node(id), Some(PredicateNode::And(_)));
            let separator = if and { "&" } else { "|" };
            let expressions = children
                .iter()
                .map(|child| encode_node::<E>(arena, *child, policy, entries, Some(and)))
                .collect::<Result<Vec<_>, _>>()?;
            let expression = expressions.join(separator);
            // AND has higher precedence. Extra parentheses would consume the
            // remote nesting budget without adding query semantics.
            Ok(if !and && parent_is_and == Some(true) {
                format!("({expression})")
            } else {
                expression
            })
        }
    }
}

#[derive(PartialEq)]
enum Expression {
    Condition(crate::Condition),
    Junction(bool, Vec<Self>),
}
fn signature(arena: &PredicateArena) -> Option<Expression> {
    fn node(arena: &PredicateArena, id: NodeId) -> Expression {
        match arena.node(id).expect("validated predicate") {
            PredicateNode::Condition(condition) => Expression::Condition(condition.clone()),
            PredicateNode::And(children) | PredicateNode::Or(children) => {
                let and = matches!(arena.node(id), Some(PredicateNode::And(_)));
                let mut output = Vec::new();
                for child in children {
                    match node(arena, *child) {
                        Expression::Junction(kind, nested) if kind == and => output.extend(nested),
                        child => output.push(child),
                    }
                }
                if output.len() == 1 {
                    output.remove(0)
                } else {
                    Expression::Junction(and, output)
                }
            }
        }
    }
    arena.root().map(|root| node(arena, root))
}
fn invalid(reason: &str) -> ParamError {
    ParamError::InvalidValue {
        field: "rpc".to_owned(),
        reason: reason.to_owned(),
    }
}
