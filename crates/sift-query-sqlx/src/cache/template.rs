//! Value-free template identity and instructions for rebuilding current binds.

use std::any::TypeId;
use std::sync::Arc;

use sift_query_core::{
    AggregateFn, CursorValue, EntityMeta, FieldId, NodeId, Operator, OrderBy, PagingMode,
    PredicateArena, PredicateNode, QueryPlan, ScalarValue, SearchEntity, SearchOptions, TypedList,
    ValueType,
};

use crate::{PgBindValue, PgRenderer, QueryShapeId, RenderError, RenderedPgQuery};

#[derive(Debug)]
pub(crate) enum RenderRequest<'a, E: SearchEntity> {
    List,
    Window,
    Cluster(Option<&'a SearchOptions<E>>),
    Sum(FieldId),
}

impl<E: SearchEntity> Copy for RenderRequest<'_, E> {}
impl<E: SearchEntity> Clone for RenderRequest<'_, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: SearchEntity> RenderRequest<'_, E> {
    pub(crate) fn render(
        &self,
        renderer: &PgRenderer,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedPgQuery, RenderError> {
        match self {
            Self::List => renderer.render_list(plan),
            Self::Window => renderer.render_window_list(plan),
            Self::Cluster(options) => renderer.render_cluster(plan, *options),
            Self::Sum(field) => renderer.render_sum(plan, *field),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Statement {
    List,
    Window,
    Cluster,
    Sum(FieldId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NodeShape {
    Root(Option<NodeId>),
    Condition(NodeId, FieldId, Operator, bool, usize, bool),
    And(NodeId, usize),
    Or(NodeId, usize),
}

/// Full equality, rather than a 64-bit fingerprint alone, determines a hit.
/// Metadata is static; its address and Rust type separate handwritten schemas
/// even if they accidentally declare identical schema fingerprints.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TemplateKey {
    entity: TypeId,
    meta: usize,
    seed: u64,
    target: u8,
    statement: Statement,
    distinct: bool,
    projection: Vec<FieldId>,
    aliases: Option<Vec<&'static str>>,
    order: Vec<OrderBy>,
    paging: u8,
    cursor: Option<Vec<bool>>,
    predicates: [Vec<NodeShape>; 4],
    summaries: Vec<(FieldId, AggregateFn, ValueType)>,
}

impl TemplateKey {
    pub(super) fn new<E: SearchEntity>(
        plan: &QueryPlan<E>,
        request: &RenderRequest<'_, E>,
    ) -> Result<Self, RenderError> {
        let statement = match request {
            RenderRequest::List => Statement::List,
            RenderRequest::Window => Statement::Window,
            RenderRequest::Cluster(_) => Statement::Cluster,
            RenderRequest::Sum(field) => Statement::Sum(*field),
        };
        let (paging, cursor) = match &plan.paging.mode {
            PagingMode::Page { .. } => (0, None),
            PagingMode::Offset => (1, None),
            PagingMode::Cursor(None) => (2, None),
            PagingMode::Cursor(Some(cursor)) => (
                3,
                Some(
                    cursor
                        .values()
                        .iter()
                        .map(|value| matches!(value, CursorValue::Null))
                        .collect(),
                ),
            ),
        };
        let summaries = match request {
            RenderRequest::Cluster(Some(options)) => options
                .requests()
                .iter()
                .map(|request| (request.field, request.function, request.input_type))
                .collect(),
            _ => Vec::new(),
        };
        Ok(Self {
            entity: TypeId::of::<E>(),
            meta: std::ptr::from_ref::<EntityMeta>(plan.meta) as usize,
            seed: plan.shape_fingerprint_seed(),
            target: plan.target as u8,
            statement,
            distinct: plan.distinct,
            projection: plan.projection.clone(),
            aliases: plan.projection_aliases.clone(),
            order: plan.order.clone(),
            paging,
            cursor,
            predicates: [
                arena_shape(&plan.server_predicate)?,
                arena_shape(&plan.row_predicate)?,
                arena_shape(&plan.server_group_predicate)?,
                arena_shape(&plan.group_predicate)?,
            ],
            summaries,
        })
    }
}

fn arena_shape(arena: &PredicateArena) -> Result<Vec<NodeShape>, RenderError> {
    let Some(root) = arena.root() else {
        return Ok(Vec::new());
    };
    let mut shape = Vec::with_capacity(arena.nodes().len() + 1);
    shape.push(NodeShape::Root(Some(root)));
    shape_node(arena, root, &mut shape)?;
    Ok(shape)
}

fn shape_node(
    arena: &PredicateArena,
    id: NodeId,
    output: &mut Vec<NodeShape>,
) -> Result<(), RenderError> {
    match arena.node(id).ok_or_else(missing_node)? {
        PredicateNode::Condition(condition) => output.push(NodeShape::Condition(
            id,
            condition.field,
            condition.operator,
            condition.ignore_case,
            if matches!(condition.operator, Operator::InList | Operator::NotIn) {
                0
            } else {
                condition.values.len()
            },
            !matches!(
                condition.operator,
                Operator::StartWith
                    | Operator::EndWith
                    | Operator::Contain
                    | Operator::NotLike
                    | Operator::OrLike
            ) || condition
                .values
                .iter()
                .all(|value| matches!(value, ScalarValue::Text(_))),
        )),
        PredicateNode::And(children) | PredicateNode::Or(children) => {
            output.push(if matches!(arena.node(id), Some(PredicateNode::And(_))) {
                NodeShape::And(id, children.len())
            } else {
                NodeShape::Or(id, children.len())
            });
            for child in children {
                shape_node(arena, *child, output)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Arena {
    ServerRow,
    UserRow,
    ServerGroup,
    UserGroup,
}

impl Arena {
    fn get<E: SearchEntity>(self, plan: &QueryPlan<E>) -> &PredicateArena {
        match self {
            Self::ServerRow => &plan.server_predicate,
            Self::UserRow => &plan.row_predicate,
            Self::ServerGroup => &plan.server_group_predicate,
            Self::UserGroup => &plan.group_predicate,
        }
    }
}

#[derive(Debug)]
enum BindStep {
    Value(Arena, NodeId, usize),
    Array(Arena, NodeId),
    Like(Arena, NodeId, Operator),
    Cursor(usize),
    OrderInput(FieldId),
    Limit,
    Offset,
}

#[derive(Debug)]
pub(super) struct Template {
    sql: Arc<str>,
    shape_id: QueryShapeId,
    steps: Vec<BindStep>,
}

impl Template {
    pub(super) fn new<E: SearchEntity>(
        plan: &QueryPlan<E>,
        request: &RenderRequest<'_, E>,
        rendered: &RenderedPgQuery,
    ) -> Result<Self, RenderError> {
        let mut steps = Vec::with_capacity(rendered.binds.len());
        for arena in [Arena::ServerRow, Arena::UserRow] {
            arena_steps(plan, arena, &mut steps)?;
        }
        if matches!(request, RenderRequest::List)
            && let PagingMode::Cursor(Some(cursor)) = &plan.paging.mode
        {
            // Preserve the renderer's exact placeholder order, including
            // equality slots in cursor alternatives which simplify away.
            for index in 0..plan.order.len() {
                for (previous, value) in cursor.values().iter().take(index).enumerate() {
                    if matches!(value, CursorValue::Scalar(_)) {
                        steps.push(BindStep::Cursor(previous));
                    }
                }
                if matches!(cursor.values().get(index), Some(CursorValue::Scalar(_))) {
                    steps.push(BindStep::Cursor(index));
                }
            }
        }
        if !matches!(request, RenderRequest::Window)
            && (matches!(request, RenderRequest::List) || plan.distinct || plan.meta.is_grouped())
        {
            for arena in [Arena::ServerGroup, Arena::UserGroup] {
                arena_steps(plan, arena, &mut steps)?;
            }
        }
        if matches!(request, RenderRequest::List | RenderRequest::Window) {
            for order in &plan.order {
                if E::condition_order(order.field).is_some() {
                    steps.push(BindStep::OrderInput(order.field));
                }
            }
            steps.push(BindStep::Limit);
            if !matches!(plan.paging.mode, PagingMode::Cursor(_)) {
                steps.push(BindStep::Offset);
            }
        }
        if steps.len() != rendered.binds.len() {
            return Err(RenderError::InvalidCondition(
                "template bind layout does not match renderer".to_owned(),
            ));
        }
        Ok(Self {
            sql: Arc::clone(&rendered.sql),
            shape_id: rendered.shape_id,
            steps,
        })
    }

    pub(super) fn bind<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedPgQuery, RenderError> {
        let mut binds = Vec::with_capacity(self.steps.len());
        for step in &self.steps {
            let bind = match *step {
                BindStep::OrderInput(field) => PgBindValue::Scalar(
                    plan.condition_order_value(field)
                        .map_err(|error| RenderError::InvalidCondition(error.to_string()))?
                        .ok_or_else(missing_node)?
                        .clone(),
                ),
                BindStep::Limit => {
                    PgBindValue::Scalar(ScalarValue::I64(i64::from(plan.paging.size)))
                }
                BindStep::Offset => PgBindValue::Scalar(ScalarValue::I64(
                    i64::try_from(plan.paging.offset)
                        .map_err(|_| RenderError::InvalidCondition("offset overflow".to_owned()))?,
                )),
                BindStep::Cursor(index) => {
                    let PagingMode::Cursor(Some(cursor)) = &plan.paging.mode else {
                        return Err(missing_node());
                    };
                    let Some(CursorValue::Scalar(value)) = cursor.values().get(index) else {
                        return Err(missing_node());
                    };
                    PgBindValue::Scalar(value.clone())
                }
                BindStep::Value(arena, id, index) => PgBindValue::Scalar(
                    condition(plan, arena, id)?
                        .values
                        .get(index)
                        .ok_or_else(missing_node)?
                        .clone(),
                ),
                BindStep::Array(arena, id) => {
                    let condition = condition(plan, arena, id)?;
                    let field = plan
                        .meta
                        .field(condition.field)
                        .ok_or(RenderError::UnknownField(condition.field.0))?;
                    let list = TypedList::new(field.value_type, condition.values.clone())
                        .map_err(|error| RenderError::InvalidCondition(error.to_string()))?;
                    PgBindValue::Array(list)
                }
                BindStep::Like(arena, id, operator) => {
                    let condition = condition(plan, arena, id)?;
                    let Some(ScalarValue::Text(value)) = condition.values.first() else {
                        return Err(RenderError::InvalidCondition(
                            "LIKE template needs text".to_owned(),
                        ));
                    };
                    PgBindValue::Scalar(ScalarValue::Text(
                        crate::renderer::predicate::like_pattern(value, operator),
                    ))
                }
            };
            binds.push(bind);
        }
        Ok(RenderedPgQuery {
            sql: Arc::clone(&self.sql),
            shape_id: self.shape_id,
            binds,
        })
    }
}

fn condition<E: SearchEntity>(
    plan: &QueryPlan<E>,
    arena: Arena,
    id: NodeId,
) -> Result<&sift_query_core::Condition, RenderError> {
    let Some(PredicateNode::Condition(condition)) = arena.get(plan).node(id) else {
        return Err(missing_node());
    };
    Ok(condition)
}

fn arena_steps<E: SearchEntity>(
    plan: &QueryPlan<E>,
    arena: Arena,
    output: &mut Vec<BindStep>,
) -> Result<(), RenderError> {
    if let Some(root) = arena.get(plan).root() {
        node_steps(arena.get(plan), arena, root, output)?;
    }
    Ok(())
}

fn node_steps(
    arena: &PredicateArena,
    source: Arena,
    id: NodeId,
    output: &mut Vec<BindStep>,
) -> Result<(), RenderError> {
    match arena.node(id).ok_or_else(missing_node)? {
        PredicateNode::Condition(condition) => match condition.operator {
            Operator::InList | Operator::NotIn => output.push(BindStep::Array(source, id)),
            Operator::StartWith | Operator::EndWith | Operator::Contain => {
                output.push(BindStep::Like(source, id, condition.operator));
            }
            Operator::IsNull
            | Operator::NotNull
            | Operator::Empty
            | Operator::NotEmpty
            | Operator::AlwaysTrue
            | Operator::AlwaysFalse => {}
            Operator::OrLike | Operator::Between | Operator::NotBetween | Operator::Custom(_) => {
                for index in 0..condition.values.len() {
                    output.push(BindStep::Value(source, id, index));
                }
            }
            _ => output.push(BindStep::Value(source, id, 0)),
        },
        PredicateNode::And(children) | PredicateNode::Or(children) => {
            for child in children {
                node_steps(arena, source, *child, output)?;
            }
        }
    }
    Ok(())
}

fn missing_node() -> RenderError {
    RenderError::InvalidCondition("template predicate or bind value is missing".to_owned())
}
