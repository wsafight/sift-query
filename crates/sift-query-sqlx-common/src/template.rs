//! Value-free SQL template caching for the portable renderer.
//!
//! A cache entry contains SQL text and instructions for rebuilding bind
//! values from the current query plan. Request values are never retained in
//! the cache.

use std::any::TypeId;
use std::sync::{Arc, Mutex, OnceLock};

use sift_query_algorithm::BoundedLruMap;
use sift_query_core::{
    AggregateFn, CursorValue, EntityMeta, FieldId, NodeId, Operator, OrderBy, PagingMode,
    PredicateArena, PredicateNode, QueryPlan, ScalarValue, SearchEntity, SearchOptions, TypedList,
    ValueType,
};

use crate::renderer::like_pattern;
use crate::{Dialect, RenderError, RenderedQuery, Renderer};

#[derive(Debug)]
pub(super) enum RenderRequest<'a, E: SearchEntity> {
    List,
    Count,
    Cluster(Option<&'a SearchOptions<E>>),
}

impl<E: SearchEntity> Copy for RenderRequest<'_, E> {}
impl<E: SearchEntity> Clone for RenderRequest<'_, E> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Statement {
    List,
    Count,
    Cluster,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NodeShape {
    Root(Option<NodeId>),
    Condition(NodeId, FieldId, Operator, bool, usize, bool),
    And(NodeId, usize),
    Or(NodeId, usize),
}

/// Full structural identity for one renderer template. The SQL hash alone is
/// insufficient because two entities can emit the same SQL while their node
/// ids point at different bind values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TemplateKey {
    dialect: Dialect,
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
    fn new<E: SearchEntity>(
        dialect: Dialect,
        plan: &QueryPlan<E>,
        request: &RenderRequest<'_, E>,
    ) -> Result<Self, RenderError> {
        let statement = match request {
            RenderRequest::List => Statement::List,
            RenderRequest::Count => Statement::Count,
            RenderRequest::Cluster(_) => Statement::Cluster,
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
            dialect,
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
            condition.values.len(),
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
        PredicateNode::And(children) => {
            output.push(NodeShape::And(id, children.len()));
            for child in children {
                shape_node(arena, *child, output)?;
            }
        }
        PredicateNode::Or(children) => {
            output.push(NodeShape::Or(id, children.len()));
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

#[derive(Debug, Clone, Copy)]
enum BindStep {
    Value(Arena, NodeId, usize),
    Array(Arena, NodeId),
    Like(Arena, NodeId, usize, Operator),
    Cursor(usize),
    OrderInput(FieldId),
    Limit,
    Offset,
}

#[derive(Debug)]
struct Template {
    sql: Arc<str>,
    shape_id: u64,
    steps: Vec<BindStep>,
}

impl Template {
    fn new<E: SearchEntity>(
        plan: &QueryPlan<E>,
        request: &RenderRequest<'_, E>,
        rendered: &RenderedQuery,
    ) -> Result<Self, RenderError> {
        let mut steps = Vec::with_capacity(rendered.binds.len());
        arena_steps(plan, Arena::ServerRow, &mut steps)?;
        arena_steps(plan, Arena::UserRow, &mut steps)?;
        if matches!(request, RenderRequest::List)
            && let PagingMode::Cursor(Some(cursor)) = &plan.paging.mode
        {
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
        if matches!(request, RenderRequest::List) || plan.distinct || plan.meta.is_grouped() {
            arena_steps(plan, Arena::ServerGroup, &mut steps)?;
            arena_steps(plan, Arena::UserGroup, &mut steps)?;
        }
        if matches!(request, RenderRequest::List) {
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
        let bind_count = steps
            .iter()
            .map(|step| match *step {
                BindStep::Array(arena, id) => arena
                    .get(plan)
                    .node(id)
                    .and_then(|node| match node {
                        PredicateNode::Condition(condition) => Some(condition.values.len()),
                        _ => None,
                    })
                    .unwrap_or(0),
                _ => 1,
            })
            .sum::<usize>();
        if bind_count != rendered.binds.len() {
            return Err(RenderError::InvalidCondition(
                "template bind layout does not match renderer".to_owned(),
            ));
        }
        Ok(Self {
            sql: Arc::from(rendered.sql.as_str()),
            shape_id: rendered.shape_id,
            steps,
        })
    }

    fn bind<E: SearchEntity>(&self, plan: &QueryPlan<E>) -> Result<RenderedQuery, RenderError> {
        let mut binds = Vec::with_capacity(self.steps.len());
        for step in &self.steps {
            if let BindStep::Array(arena, id) = *step {
                let condition = condition(plan, arena, id)?;
                let field = plan
                    .meta
                    .field(condition.field)
                    .ok_or(RenderError::UnknownField(condition.field.0))?;
                let list = TypedList::new(field.value_type, condition.values.clone())
                    .map_err(|error| RenderError::InvalidCondition(error.to_string()))?;
                binds.extend(list.values);
                continue;
            }
            let value = match *step {
                BindStep::OrderInput(field) => plan
                    .condition_order_value(field)
                    .map_err(|error| RenderError::InvalidCondition(error.to_string()))?
                    .ok_or_else(missing_node)?
                    .clone(),
                BindStep::Limit => ScalarValue::I64(i64::from(plan.paging.size)),
                BindStep::Offset => ScalarValue::I64(
                    i64::try_from(plan.paging.offset)
                        .map_err(|_| RenderError::InvalidCondition("offset overflow".to_owned()))?,
                ),
                BindStep::Cursor(index) => {
                    let PagingMode::Cursor(Some(cursor)) = &plan.paging.mode else {
                        return Err(missing_node());
                    };
                    let Some(CursorValue::Scalar(value)) = cursor.values().get(index) else {
                        return Err(missing_node());
                    };
                    value.clone()
                }
                BindStep::Value(arena, id, index) => condition(plan, arena, id)?
                    .values
                    .get(index)
                    .ok_or_else(missing_node)?
                    .clone(),
                BindStep::Like(arena, id, index, operator) => {
                    let Some(ScalarValue::Text(value)) =
                        condition(plan, arena, id)?.values.get(index)
                    else {
                        return Err(RenderError::InvalidCondition(
                            "LIKE template needs text".to_owned(),
                        ));
                    };
                    ScalarValue::Text(like_pattern(value, operator))
                }
                BindStep::Array(_, _) => unreachable!("array binds are expanded above"),
            };
            binds.push(value);
        }
        Ok(RenderedQuery {
            sql: self.sql.to_string(),
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
            Operator::IsNull
            | Operator::NotNull
            | Operator::Empty
            | Operator::NotEmpty
            | Operator::AlwaysTrue
            | Operator::AlwaysFalse => {}
            Operator::StartWith | Operator::EndWith | Operator::Contain | Operator::NotLike => {
                output.push(BindStep::Like(source, id, 0, condition.operator));
            }
            Operator::InList | Operator::NotIn => output.push(BindStep::Array(source, id)),
            Operator::OrLike => {
                for index in 0..condition.values.len() {
                    output.push(BindStep::Like(source, id, index, condition.operator));
                }
            }
            _ => {
                for index in 0..condition.values.len() {
                    output.push(BindStep::Value(source, id, index));
                }
            }
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

type Cache = Mutex<BoundedLruMap<TemplateKey, Arc<Template>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BoundedLruMap::new(256).expect("fixed cache capacity")))
}

pub(super) fn render_cached<E, F>(
    renderer: Renderer,
    plan: &QueryPlan<E>,
    request: RenderRequest<'_, E>,
    render: F,
) -> Result<RenderedQuery, RenderError>
where
    E: SearchEntity,
    F: FnOnce() -> Result<RenderedQuery, RenderError>,
{
    let Ok(key) = TemplateKey::new(renderer.dialect, plan, &request) else {
        return render();
    };
    {
        let mut state = cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(template) = state.get(&key).cloned() {
            drop(state);
            return template.bind(plan);
        }
    }
    let output = render()?;
    let template = Arc::new(Template::new(plan, &request, &output)?);
    cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, template);
    Ok(output)
}
