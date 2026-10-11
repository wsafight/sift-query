use thiserror::Error;

/// Errors caused by untrusted query parameters or programmatic user queries.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParamError {
    #[error("query string exceeds the configured limit")]
    QueryTooLong,
    #[error("query string contains invalid form encoding")]
    InvalidEncoding,
    #[error("invalid parameter name: {0}")]
    InvalidName(String),
    #[error("unknown parameter: {0}")]
    UnknownParameter(String),
    #[error("unknown field: {0}")]
    UnknownField(String),
    #[error("duplicate parameter: {0}")]
    DuplicateParameter(String),
    #[error("conflicting parameters: {0}")]
    ConflictingParameters(String),
    #[error("indexed values for {field} are not contiguous at index {expected}")]
    NonContiguousIndex { field: String, expected: usize },
    #[error("operator {operator} for {field} expects {expected} value(s), got {actual}")]
    WrongArity {
        field: String,
        operator: &'static str,
        expected: &'static str,
        actual: usize,
    },
    #[error("invalid value for field {field}: {reason}")]
    InvalidValue { field: String, reason: String },
    #[error("field is not filterable: {0}")]
    NotFilterable(String),
    #[error("field is not sortable: {0}")]
    NotSortable(String),
    #[error("field is not selectable: {0}")]
    NotSelectable(String),
    #[error("field access denied: {0}")]
    FieldAccessDenied(String),
    #[error("operator {operator} is not allowed for field {field}")]
    OperatorNotAllowed {
        field: String,
        operator: &'static str,
    },
    #[error("ignore-case is not allowed for field {0}")]
    IgnoreCaseNotAllowed(String),
    #[error("dynamic projection is not allowed for this operation")]
    ProjectionNotAllowed,
    #[error("dynamic projection cannot be empty")]
    EmptyProjection,
    #[error("paging value is outside the configured range: {0}")]
    PagingOutOfRange(String),
    #[error("paging parameters are not allowed for this operation")]
    PagingNotAllowed,
    #[error("invalid group expression: {0}")]
    InvalidGroupExpr(String),
    #[error("group expression references an undefined group: {0}")]
    UndefinedGroup(String),
    #[error("defined group is not used by gexpr: {0}")]
    UnusedGroup(String),
    #[error("OR cannot combine WHERE and HAVING predicates")]
    MixedPredicatePhase,
    #[error("pagination requires a stable entity order")]
    UnstablePagination,
    #[error("DISTINCT ordering field is not part of the final projection: {0}")]
    DistinctOrderNotProjected(String),
    #[error("query exceeds the configured complexity budget: {0}")]
    TooComplex(&'static str),
}

/// Errors in static entity or policy configuration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

/// Errors while combining a validated user query with server policy.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum PlanError {
    #[error(transparent)]
    Param(#[from] ParamError),
    #[error("query was built for a different entity schema")]
    SchemaMismatch,
}
