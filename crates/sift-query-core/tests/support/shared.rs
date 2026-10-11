pub(super) use sift_query_core::{
    EntityLimits, EntityMeta, FieldId, FieldKind, FieldMeta, Operator, OperatorSet, OrderMeta,
    ParamError, ParamParser, QueryPolicy, RawParams, SearchEntity, SortOrder, UserQuery, ValueType,
};

pub const LIMITS: EntityLimits = EntityLimits {
    max_page_size: 100,
    max_offset: 20_000,
    timeout_ms: 5_000,
};

pub(super) const OPS_TEXT: &[Operator] = &[Operator::Eq, Operator::StartWith];

pub(super) const OPS_I64: &[Operator] = &[
    Operator::Eq,
    Operator::Between,
    Operator::InList,
    Operator::IsNull,
];

pub(super) const OPS_I32: &[Operator] = &[
    Operator::Eq,
    Operator::Ge,
    Operator::Between,
    Operator::InList,
];

pub(super) static DEFAULT_ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];

// ---------------------------------------------------------------------------
// Rich scalar entity exercising every operator, flag, and field kind.
// ---------------------------------------------------------------------------

pub(super) const OPS_RICH_I64: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::Gt,
    Operator::Ge,
    Operator::Lt,
    Operator::Le,
    Operator::Between,
    Operator::NotBetween,
    Operator::InList,
    Operator::NotIn,
    Operator::IsNull,
    Operator::NotNull,
    Operator::AlwaysTrue,
    Operator::AlwaysFalse,
];

pub(super) const OPS_RICH_TEXT: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::StartWith,
    Operator::EndWith,
    Operator::Contain,
    Operator::OrLike,
    Operator::NotLike,
    Operator::Empty,
    Operator::NotEmpty,
    Operator::IsNull,
    Operator::NotNull,
];

pub(super) const OPS_RICH_I32: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::Gt,
    Operator::Ge,
    Operator::Lt,
    Operator::Le,
    Operator::Between,
    Operator::NotBetween,
    Operator::InList,
    Operator::NotIn,
    Operator::IsNull,
    Operator::NotNull,
    Operator::Empty,
    Operator::NotEmpty,
    Operator::StartWith,
    Operator::EndWith,
    Operator::Contain,
    Operator::OrLike,
    Operator::NotLike,
    Operator::AlwaysTrue,
    Operator::AlwaysFalse,
];

pub(super) const OPS_BOOL: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::IsNull,
    Operator::NotNull,
    Operator::AlwaysTrue,
    Operator::AlwaysFalse,
];

pub(super) const OPS_F64: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::Gt,
    Operator::Ge,
    Operator::Lt,
    Operator::Le,
    Operator::Between,
    Operator::NotBetween,
];

pub(super) const OPS_EMPTY: &[Operator] = &[];

pub(super) const OPS_BYTES: &[Operator] = &[Operator::Eq, Operator::InList];
