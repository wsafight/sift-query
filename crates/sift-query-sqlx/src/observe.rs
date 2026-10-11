//! PostgreSQL query observation and SDK-independent completion callbacks.
//!
//! The PostgreSQL observer adds execution-specific span fields. Portable
//! backends use `sift-query-sqlx-common::Observers`, which records the same
//! low-sensitivity outcome contract without PostgreSQL execution metadata.
use std::future::Future;
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

use crate::{PgSearchError, PgSiftQuery, SearchStream};
use sift_query_core::SearchEntity;
use tracing::Instrument;

#[derive(Debug, Clone, Copy)]
pub struct QueryObservation {
    pub entity: &'static str,
    pub operation: &'static str,
    pub duration: Duration,
    pub rows_returned: usize,
    /// `success`, `cancelled`, or a low-cardinality error category.
    pub outcome: &'static str,
    pub slow: bool,
}

/// Metrics/event bridge. Called once per completed or cancelled operation;
/// panics are isolated. The callback must be nonblocking and cannot see SQL,
/// binds, raw input, tenant identifiers, or cursor tokens.
pub trait QueryObserver: Send + Sync {
    fn completed(&self, observation: &QueryObservation);
}

#[derive(Clone, Default)]
pub(crate) struct Observers {
    observer: Option<Arc<dyn QueryObserver>>,
    slow_threshold: Option<Duration>,
    entity_thresholds: std::collections::HashMap<u64, Duration>,
}
impl std::fmt::Debug for Observers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Observers")
            .field("configured", &self.observer.is_some())
            .field("slow_threshold", &self.slow_threshold)
            .finish_non_exhaustive()
    }
}

impl PgSiftQuery {
    #[must_use]
    pub fn with_query_observer<O: QueryObserver + 'static>(mut self, observer: O) -> Self {
        self.observers.observer = Some(Arc::new(observer));
        self
    }
    #[must_use]
    pub fn with_slow_query_threshold(mut self, threshold: Duration) -> Self {
        self.observers.slow_threshold = Some(threshold);
        self
    }
    #[must_use]
    pub fn with_entity_slow_query_threshold<E: SearchEntity>(
        mut self,
        threshold: Duration,
    ) -> Self {
        self.observers
            .entity_thresholds
            .insert(E::META.schema_fingerprint, threshold);
        self
    }

    pub(crate) fn observation<E: SearchEntity>(&self, operation: &'static str) -> ObservationGuard {
        let span = tracing::info_span!("sift_query", entity = E::META.entity_name, backend = "postgres", operation,
            execution_strategy = ?self.execution.strategy, count_strategy = ?self.execution.count_strategy,
            rows_returned = tracing::field::Empty, duration_us = tracing::field::Empty,
            outcome = tracing::field::Empty, query_shape_id = tracing::field::Empty, rows_written = tracing::field::Empty);
        ObservationGuard {
            span,
            started: Instant::now(),
            entity: E::META.entity_name,
            operation,
            rows: 0,
            outcome: "cancelled",
            observer: self.observers.observer.clone(),
            threshold: self
                .observers
                .entity_thresholds
                .get(&E::META.schema_fingerprint)
                .copied()
                .or(self.observers.slow_threshold),
        }
    }

    pub(crate) async fn observe<E: SearchEntity, T>(
        &self,
        operation: &'static str,
        future: impl Future<Output = Result<T, PgSearchError>>,
        rows: impl FnOnce(&T) -> usize,
    ) -> Result<T, PgSearchError> {
        let mut guard = self.observation::<E>(operation);
        let result = future.instrument(guard.span.clone()).await;
        match &result {
            Ok(value) => {
                guard.rows = rows(value);
                guard.outcome = "success";
            }
            Err(error) => guard.outcome = error_class(error),
        }
        result
    }

    pub(crate) fn observe_stream<E: SearchEntity, T: Send + 'static>(
        &self,
        operation: &'static str,
        build: impl FnOnce() -> Result<SearchStream<T>, PgSearchError>,
    ) -> Result<SearchStream<T>, PgSearchError> {
        let mut guard = self.observation::<E>(operation);
        let result = guard.span.in_scope(build);
        let mut stream = match result {
            Ok(stream) => stream,
            Err(error) => {
                guard.outcome = error_class(&error);
                return Err(error);
            }
        };
        Ok(Box::pin(futures_util::stream::poll_fn(move |cx| {
            let span = guard.span.clone();
            let _entered = span.enter();
            match stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(row))) => {
                    guard.rows += 1;
                    Poll::Ready(Some(Ok(row)))
                }
                Poll::Ready(Some(Err(error))) => {
                    guard.outcome = error_class(&error);
                    Poll::Ready(Some(Err(error)))
                }
                Poll::Ready(None) => {
                    if guard.outcome == "cancelled" {
                        guard.outcome = "success";
                    }
                    Poll::Ready(None)
                }
                Poll::Pending => Poll::Pending,
            }
        })))
    }
}

pub(crate) struct ObservationGuard {
    pub(crate) span: tracing::Span,
    started: Instant,
    entity: &'static str,
    operation: &'static str,
    rows: usize,
    outcome: &'static str,
    observer: Option<Arc<dyn QueryObserver>>,
    threshold: Option<Duration>,
}
impl Drop for ObservationGuard {
    fn drop(&mut self) {
        let duration = self.started.elapsed();
        let slow = self
            .threshold
            .is_some_and(|threshold| duration >= threshold);
        self.span.record("rows_returned", self.rows);
        self.span.record(
            "duration_us",
            u64::try_from(duration.as_micros()).unwrap_or(u64::MAX),
        );
        self.span.record("outcome", self.outcome);
        tracing::info!(parent: &self.span, outcome = self.outcome, rows_returned = self.rows, slow, "SiftQuery operation completed");
        if slow {
            tracing::warn!(parent: &self.span, "slow SiftQuery operation");
        }
        if let Some(observer) = &self.observer {
            let observation = QueryObservation {
                entity: self.entity,
                operation: self.operation,
                duration,
                rows_returned: self.rows,
                outcome: self.outcome,
                slow,
            };
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                observer.completed(&observation);
            }))
            .is_err()
            {
                tracing::warn!(parent: &self.span, "query observer panicked");
            }
        }
    }
}

pub(crate) fn error_class(error: &PgSearchError) -> &'static str {
    match error {
        PgSearchError::Param(_) | PgSearchError::Plan(_) => "parameter",
        PgSearchError::Timeout(_) => "timeout",
        PgSearchError::Sql(_) => "database",
        PgSearchError::Decode(_) => "decode",
        PgSearchError::Extension(_) => "extension",
        PgSearchError::ScanLimitExceeded { .. } | PgSearchError::ScanQueueFull => "budget",
        PgSearchError::ExportIo(_) => "output",
        _ => "configuration",
    }
}
