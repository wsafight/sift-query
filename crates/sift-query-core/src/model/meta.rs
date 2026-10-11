use std::fmt;
use std::marker::PhantomData;

use crate::{ConfigError, Operator, SortOrder};

/// Stable, entity-local field identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FieldId(pub u16);

/// Supported logical value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum ValueType {
    Bool,
    I32,
    I64,
    F32,
    F64,
    #[cfg(feature = "decimal")]
    Decimal,
    Text,
    Bytes,
    #[cfg(feature = "chrono")]
    Date,
    #[cfg(feature = "chrono")]
    Time,
    #[cfg(feature = "chrono")]
    DateTime,
    #[cfg(feature = "chrono")]
    Timestamp,
    #[cfg(feature = "uuid")]
    Uuid,
    #[cfg(feature = "json")]
    Json,
    #[cfg(feature = "postgres-types")]
    ArrayI64,
    #[cfg(feature = "postgres-types")]
    ArrayText,
    #[cfg(feature = "postgres-types")]
    Int8Range,
    #[cfg(feature = "postgres-types")]
    Network,
}

/// Types with a defined cursor encoding and ordered comparison contract.
impl ValueType {
    pub fn supports_cursor(self) -> bool {
        match self {
            #[cfg(feature = "json")]
            Self::Json => false,
            #[cfg(feature = "postgres-types")]
            Self::ArrayI64 | Self::ArrayText | Self::Int8Range | Self::Network => false,
            _ => true,
        }
    }
}

/// Determines whether a condition is evaluated before or after grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind {
    Scalar,
    GroupKey,
    Aggregate,
    FilterOnly,
}

/// A compact static operator whitelist.
#[derive(Debug, Clone, Copy)]
pub struct OperatorSet(pub &'static [Operator]);

impl OperatorSet {
    #[must_use]
    pub fn contains(self, operator: Operator) -> bool {
        self.0.contains(&operator)
    }
}

/// Metadata for one query-visible field.
#[derive(Debug)]
pub struct FieldMeta {
    pub id: FieldId,
    pub public_name: &'static str,
    pub column: &'static str,
    pub result_alias: Option<&'static str>,
    pub value_type: ValueType,
    pub kind: FieldKind,
    pub filterable: bool,
    pub selectable: bool,
    pub sortable: bool,
    pub operators: OperatorSet,
}

/// A validated static ordering item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderMeta {
    pub field: FieldId,
    pub direction: SortOrder,
}

/// Per-entity limits. Instance-level limits may only make these stricter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityLimits {
    pub max_page_size: u32,
    pub max_offset: u64,
    pub timeout_ms: u64,
}

impl Default for EntityLimits {
    fn default() -> Self {
        Self {
            max_page_size: 100,
            max_offset: 20_000,
            timeout_ms: 5_000,
        }
    }
}

/// Static metadata generated for a search entity.
#[derive(Debug)]
pub struct EntityMeta {
    pub entity_name: &'static str,
    pub from: &'static str,
    pub base_where: Option<&'static str>,
    pub fixed_having: Option<&'static str>,
    pub distinct: bool,
    pub fields: &'static [FieldMeta],
    pub default_order: &'static [OrderMeta],
    pub stable_order: &'static [OrderMeta],
    pub limits: EntityLimits,
    pub schema_fingerprint: u64,
}

impl EntityMeta {
    #[must_use]
    pub fn field(&'static self, id: FieldId) -> Option<&'static FieldMeta> {
        self.fields.get(usize::from(id.0))
    }

    #[must_use]
    pub fn resolve_field(&'static self, name: &str) -> Option<&'static FieldMeta> {
        self.fields.iter().find(|field| field.public_name == name)
    }

    #[must_use]
    pub fn is_grouped(&self) -> bool {
        self.fields
            .iter()
            .any(|field| matches!(field.kind, FieldKind::GroupKey | FieldKind::Aggregate))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.entity_name.is_empty() || self.from.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "entity_name and from must not be empty".to_owned(),
            ));
        }
        for (index, field) in self.fields.iter().enumerate() {
            if usize::from(field.id.0) != index {
                return Err(ConfigError::Invalid(format!(
                    "field {} has non-contiguous FieldId {}",
                    field.public_name, field.id.0
                )));
            }
        }
        if self.is_grouped()
            && self.fields.iter().any(|field| {
                field.result_alias.is_some() && matches!(field.kind, FieldKind::Scalar)
            })
        {
            return Err(ConfigError::Invalid(
                "grouped entities cannot return Scalar fields".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Implemented by derive-generated query entities.
pub trait SearchEntity: Sized + Send + Sync + 'static {
    /// Server-declared ordering expression using one validated query condition.
    fn condition_order(_field: FieldId) -> Option<ConditionOrder> {
        None
    }
    /// HTTP-only custom operator whitelist; trusted programmatic operators
    /// are independent of this registration boundary.
    fn custom_operator_names(_field: FieldId) -> &'static [&'static str] {
        &[]
    }
    /// Optional named codec binding generated by `#[search_field(codec = "name")]`.
    fn codec_name(_field: FieldId) -> Option<&'static str> {
        None
    }
    /// Declarative `(source field, dictionary key)` label bindings generated
    /// from `#[search_field(label = "...")]`.
    fn label_bindings() -> &'static [(&'static str, &'static str)] {
        &[]
    }
    const META: &'static EntityMeta;

    /// Server assertion that the ordering expression is never SQL NULL.
    /// Defaults to false; Rust types alone do not establish SQL nullability.
    fn cursor_key_non_null(_field: FieldId) -> bool {
        false
    }

    fn cursor_value(&self, _field: FieldId) -> Option<crate::CursorValue> {
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConditionOrder {
    pub source: FieldId,
    pub operator: &'static str,
    pub expression: crate::CustomOperator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionField {
    pub source: FieldId,
    pub result_alias: &'static str,
}

pub trait SearchProjection<E: SearchEntity>: Sized + Send + Sync + 'static {
    const FIELDS: &'static [ProjectionField];

    fn cursor_value(&self, _source: FieldId) -> Option<crate::CursorValue> {
        None
    }
}

/// Typed reference to one entity field.
#[derive(PartialEq, Eq, Hash)]
pub struct FieldRef<E, T, K> {
    id: FieldId,
    _marker: PhantomData<fn() -> (E, T, K)>,
}

impl<E, T, K> FieldRef<E, T, K> {
    #[doc(hidden)]
    pub const fn new(id: FieldId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn id(self) -> FieldId {
        self.id
    }
}

impl<E, T, K> Clone for FieldRef<E, T, K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E, T, K> Copy for FieldRef<E, T, K> {}

impl<E, T, K> fmt::Debug for FieldRef<E, T, K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("FieldRef").field(&self.id).finish()
    }
}

pub enum ScalarKind {}
pub enum GroupKeyKind {}
pub enum AggregateKind {}
pub enum FilterOnlyKind {}

pub trait RowPhaseKind {}
impl RowPhaseKind for ScalarKind {}
impl RowPhaseKind for GroupKeyKind {}
impl RowPhaseKind for FilterOnlyKind {}

pub trait SummableKind {}
impl SummableKind for ScalarKind {}
impl SummableKind for GroupKeyKind {}
impl SummableKind for AggregateKind {}

/// Marker stored in result structs for fields that participate only in filtering.
pub struct FilterOnly<T>(PhantomData<fn() -> T>);

impl<T> Default for FilterOnly<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Clone for FilterOnly<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for FilterOnly<T> {}

impl<T> fmt::Debug for FilterOnly<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FilterOnly")
    }
}
