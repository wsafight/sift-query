use crate::SummaryValue;

/// A page returned by a list query and its optional aggregate metadata.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Page<T> {
    pub data: Vec<T>,
    pub total_count: i64,
    pub summaries: Vec<SummaryValue>,
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    /// Creates an empty page with the same metadata contract as a successful
    /// zero-row query.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            data: Vec::new(),
            total_count: 0,
            summaries: Vec::new(),
            next_cursor: None,
        }
    }
}

/// A cursor page fetched without executing a count or cluster query.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CursorPage<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

impl<T> CursorPage<T> {
    /// Creates an empty cursor page. An empty page never has a continuation.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            data: Vec::new(),
            next_cursor: None,
            has_more: false,
        }
    }
}
