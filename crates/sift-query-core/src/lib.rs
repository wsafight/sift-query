//! Backend-independent query model and strict parameter protocol for `SiftQuery`.

mod codecs;
mod cursor;
mod error;
mod extensions;
#[cfg(feature = "metadata")]
pub mod metadata;
mod model;
mod policy;
mod protocol;
mod query;
#[cfg(feature = "postgres-types")]
pub use model::native::Int8Range;
#[cfg(feature = "rsql")]
pub use protocol::rsql;

#[cfg(feature = "json")]
pub use codecs::JsonValueCodec;
pub use codecs::{
    BooleanCodec, Codec, CodecRegistry, EnumCodec, FieldConvertor, ListResultCodec,
    ScalarTextCodec, StringCodec,
};
#[cfg(feature = "chrono")]
pub use codecs::{DateCodec, DateTimeCodec, TimeCodec, TimestampCodec};
pub use cursor::{Cursor, CursorCodec, CursorValue, IntoCursorValue};
pub use error::{ConfigError, ParamError, PlanError};
pub use extensions::{
    DefaultParamFilter, IndexedArrayParamFilter, JsonArrayParamFilter, RawParamFilter,
    SuffixOperatorParamFilter,
};
pub use model::meta::{
    AggregateKind, ConditionOrder, EntityLimits, EntityMeta, FieldId, FieldKind, FieldMeta,
    FieldRef, FilterOnly, FilterOnlyKind, GroupKeyKind, OperatorSet, OrderMeta, ProjectionField,
    RowPhaseKind, ScalarKind, SearchEntity, SearchProjection, SummableKind, ValueType,
};
pub use model::value::{IntoScalarValue, RowValue, ScalarValue, TypedList};
pub use policy::{CompatibilityPolicy, ParamNames, QueryHardLimits, QueryLimits, QueryPolicy};
pub use protocol::parser::ParamParser;
pub use protocol::raw::RawParams;
pub use query::bucket::{
    BucketEntry, BucketRequest, BucketResult, NumericBucketRequest, TimeBucketRequest,
    TimeBucketUnit, numeric_label as numeric_bucket_label,
};
pub use query::builder::QueryBuilder;
pub use query::facet::{FacetBucket, FacetRequest, FacetResult};
pub use query::filter::FilterExpr;
pub use query::plan::{
    GroupScope, NullOrder, OrderBy, Paging, PagingMode, Projection, QueryInput, QueryPlan,
    QueryTarget, ServerScope, SortOrder, UserQuery,
};
pub use query::predicate::{
    Condition, CustomOperator, NodeId, Operator, PredicateArena, PredicateNode, PredicatePhase,
    RegisteredOperator,
};
pub use query::scope::ScopeProvider;
pub use query::scope::{
    FieldAccessPolicy, FieldAccessPolicyBuilder, GroupScopeBuilder, ScopeLimits, ServerScopeBuilder,
};
pub use query::sql::{ServerSqlExtension, ServerSqlRegistry};
pub use query::summary::{AggregateFn, CustomAggregate, SearchOptions, SummaryRequest};

/// Items used by generated code. This module is public for macro expansion but
/// is not a separate stability surface.
#[doc(hidden)]
pub mod __private {
    pub use crate::{
        AggregateKind, ConditionOrder, CustomOperator, EntityLimits, EntityMeta, FieldId,
        FieldKind, FieldMeta, FieldRef, FilterOnlyKind, GroupKeyKind, Operator, OperatorSet,
        OrderMeta, ProjectionField, ScalarKind, SearchEntity, SearchProjection, SortOrder,
        ValueType,
    };
}
