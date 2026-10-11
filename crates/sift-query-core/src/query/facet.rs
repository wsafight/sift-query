use crate::{FieldId, FieldRef, RowValue, SearchEntity};

/// A server-declared facet over one entity field.
///
/// The field is validated against the entity metadata before SQL is rendered;
/// this type carries no client-provided SQL identifier or expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FacetRequest {
    field: FieldId,
    schema_fingerprint: u64,
    limit: u32,
    include_null: bool,
    exclude_self_filter: bool,
}

impl FacetRequest {
    #[must_use]
    pub const fn new<E: SearchEntity, T, K>(field: FieldRef<E, T, K>, limit: u32) -> Self {
        Self {
            field: field.id(),
            schema_fingerprint: E::META.schema_fingerprint,
            limit,
            include_null: true,
            exclude_self_filter: false,
        }
    }

    #[must_use]
    pub const fn excluding_nulls(self) -> Self {
        Self {
            include_null: false,
            ..self
        }
    }

    /// Ignores user conditions on this field while computing this facet.
    /// Server-owned scope predicates are always retained.
    #[must_use]
    pub const fn excluding_self_filter(self) -> Self {
        Self {
            exclude_self_filter: true,
            ..self
        }
    }

    #[must_use]
    pub const fn field_id(self) -> FieldId {
        self.field
    }

    #[must_use]
    pub const fn schema_fingerprint(self) -> u64 {
        self.schema_fingerprint
    }

    #[must_use]
    pub const fn limit(self) -> u32 {
        self.limit
    }

    #[must_use]
    pub const fn includes_null(self) -> bool {
        self.include_null
    }

    #[must_use]
    pub const fn excludes_self_filter(self) -> bool {
        self.exclude_self_filter
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FacetBucket {
    pub value: RowValue,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FacetResult {
    pub field: &'static str,
    pub buckets: Vec<FacetBucket>,
    pub other_count: i64,
}

impl FacetResult {
    #[must_use]
    pub fn empty(field: &'static str) -> Self {
        Self {
            field,
            buckets: Vec::new(),
            other_count: 0,
        }
    }
}
