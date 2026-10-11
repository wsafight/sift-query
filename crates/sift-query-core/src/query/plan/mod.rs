use std::marker::PhantomData;
use std::sync::Arc;

use crate::ParamParser;
use crate::{
    Cursor, FieldAccessPolicy, FieldId, ParamError, PlanError, PredicateArena, QueryPolicy,
    RawParams, SearchEntity, SearchProjection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "asc" => Some(Self::Asc),
            "desc" => Some(Self::Desc),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NullOrder {
    First,
    Last,
}

impl From<SortOrder> for NullOrder {
    fn from(value: SortOrder) -> Self {
        match value {
            SortOrder::Asc => Self::Last,
            SortOrder::Desc => Self::First,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OrderBy {
    pub field: FieldId,
    pub direction: SortOrder,
    pub nulls: NullOrder,
}

impl From<crate::OrderMeta> for OrderBy {
    fn from(value: crate::OrderMeta) -> Self {
        Self {
            field: value.field,
            direction: value.direction,
            nulls: value.direction.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Projection {
    All,
    Fields(Vec<FieldId>),
}

impl Projection {
    #[must_use]
    pub const fn is_dynamic(&self) -> bool {
        matches!(self, Self::Fields(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PagingMode {
    Page { page: u64 },
    Offset,
    Cursor(Option<Cursor>),
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Paging {
    pub mode: PagingMode,
    pub size: u32,
    pub offset: u64,
    pub explicit_page: bool,
    pub explicit_size: bool,
    pub explicit_offset: bool,
    pub explicit_cursor: bool,
}

impl Default for Paging {
    fn default() -> Self {
        Self {
            mode: PagingMode::Page { page: 1 },
            size: 15,
            offset: 0,
            explicit_page: false,
            explicit_size: false,
            explicit_offset: false,
            explicit_cursor: false,
        }
    }
}

/// Validated, entity-bound user query.
#[derive(Debug)]
struct UserQueryData {
    predicate: PredicateArena,
    projection: Projection,
    order: Vec<OrderBy>,
    paging: Paging,
    schema_fingerprint: u64,
    policy_fingerprint: u64,
    force_empty: bool,
}

#[derive(Debug)]
pub struct UserQuery<E> {
    inner: Arc<UserQueryData>,
    _entity: PhantomData<fn() -> E>,
}

impl<E> Clone for UserQuery<E> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _entity: PhantomData,
        }
    }
}

impl<E> UserQuery<E> {
    pub(crate) fn from_parts(
        predicate: PredicateArena,
        projection: Projection,
        order: Vec<OrderBy>,
        paging: Paging,
        schema_fingerprint: u64,
        policy_fingerprint: u64,
        force_empty: bool,
    ) -> Self {
        Self {
            inner: Arc::new(UserQueryData {
                predicate,
                projection,
                order,
                paging,
                schema_fingerprint,
                policy_fingerprint,
                force_empty,
            }),
            _entity: PhantomData,
        }
    }

    #[must_use]
    pub fn predicate(&self) -> &PredicateArena {
        &self.inner.predicate
    }

    #[must_use]
    pub fn projection(&self) -> &Projection {
        &self.inner.projection
    }

    #[must_use]
    pub fn order(&self) -> &[OrderBy] {
        &self.inner.order
    }

    #[must_use]
    pub fn paging(&self) -> Paging {
        self.inner.paging.clone()
    }

    #[must_use]
    pub fn is_forced_empty(&self) -> bool {
        self.inner.force_empty
    }
}

/// Service-controlled row-level predicate.
#[derive(Debug, Clone)]
pub struct ServerScope<E: SearchEntity> {
    pub(crate) predicate: PredicateArena,
    pub(crate) schema_fingerprint: u64,
    pub(crate) field_access: FieldAccessPolicy<E>,
    pub(crate) _entity: PhantomData<fn() -> E>,
}

/// Service-controlled predicate evaluated after `GROUP BY`.
#[derive(Debug, Clone)]
pub struct GroupScope<E> {
    pub(crate) predicate: PredicateArena,
    pub(crate) schema_fingerprint: u64,
    pub(crate) _entity: PhantomData<fn() -> E>,
}

impl<E: SearchEntity> GroupScope<E> {
    #[must_use]
    pub fn unrestricted() -> Self {
        Self {
            predicate: PredicateArena::default(),
            schema_fingerprint: E::META.schema_fingerprint,
            _entity: PhantomData,
        }
    }

    #[must_use]
    pub fn predicate(&self) -> &PredicateArena {
        &self.predicate
    }
}

impl<E: SearchEntity> ServerScope<E> {
    #[must_use]
    pub fn unrestricted() -> Self {
        Self {
            predicate: PredicateArena::default(),
            schema_fingerprint: E::META.schema_fingerprint,
            field_access: FieldAccessPolicy::unrestricted(),
            _entity: PhantomData,
        }
    }

    /// Attaches a server-owned field policy to this row scope.
    pub fn with_field_access(
        mut self,
        field_access: FieldAccessPolicy<E>,
    ) -> Result<Self, ParamError> {
        if field_access.schema_fingerprint() != E::META.schema_fingerprint {
            return Err(ParamError::UnknownField(
                "field access entity schema mismatch".to_owned(),
            ));
        }
        self.field_access = field_access;
        Ok(self)
    }

    #[must_use]
    pub fn field_access(&self) -> &FieldAccessPolicy<E> {
        &self.field_access
    }

    #[must_use]
    pub fn predicate(&self) -> &PredicateArena {
        &self.predicate
    }
}

pub trait QueryInput<E: SearchEntity>: private::Sealed<E> {
    fn into_user_query(self, policy: &QueryPolicy) -> Result<UserQuery<E>, ParamError>;
}

impl<E: SearchEntity> QueryInput<E> for &RawParams {
    fn into_user_query(self, policy: &QueryPolicy) -> Result<UserQuery<E>, ParamError> {
        ParamParser::new(policy)?.parse(self)
    }
}

impl<E: SearchEntity> QueryInput<E> for &UserQuery<E> {
    fn into_user_query(self, policy: &QueryPolicy) -> Result<UserQuery<E>, ParamError> {
        if self.inner.schema_fingerprint != E::META.schema_fingerprint {
            return Err(ParamError::UnknownField(
                "entity schema mismatch".to_owned(),
            ));
        }
        if self.inner.policy_fingerprint != policy.fingerprint() {
            return Err(ParamError::ConflictingParameters(
                "query policy fingerprint mismatch".to_owned(),
            ));
        }
        Ok(self.clone())
    }
}

mod private {
    use super::*;

    pub trait Sealed<E: SearchEntity> {}
    impl<E: SearchEntity> Sealed<E> for &RawParams {}
    impl<E: SearchEntity> Sealed<E> for &UserQuery<E> {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryTarget {
    TypedPage,
    MapPage,
    TypedList,
    First,
    Count,
    Sum,
    StaticPage,
}

/// Immutable input to a backend renderer.
#[derive(Debug)]
pub struct QueryPlan<E: SearchEntity> {
    pub meta: &'static crate::EntityMeta,
    pub server_predicate: PredicateArena,
    pub server_group_predicate: PredicateArena,
    pub row_predicate: PredicateArena,
    pub group_predicate: PredicateArena,
    pub projection: Vec<FieldId>,
    pub projection_aliases: Option<Vec<&'static str>>,
    pub order: Vec<OrderBy>,
    pub paging: Paging,
    pub target: QueryTarget,
    pub distinct: bool,
    pub force_empty: bool,
    field_access: FieldAccessPolicy<E>,
    policy_fingerprint: u64,
    _entity: PhantomData<fn() -> E>,
}

mod construct;
#[cfg(test)]
mod tests;
