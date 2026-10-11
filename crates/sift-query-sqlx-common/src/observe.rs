use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

use crate::SearchError;
use futures_core::Stream;
use sift_query_core::SearchEntity;
use tracing::Instrument;

#[derive(Debug, Clone, Copy)]
pub struct QueryObservation {
    pub entity: &'static str,
    pub operation: &'static str,
    pub duration: Duration,
    pub rows_returned: usize,
    pub outcome: &'static str,
    pub slow: bool,
}

pub trait QueryObserver: Send + Sync {
    fn completed(&self, observation: &QueryObservation);
}

#[derive(Clone)]
pub struct Observers {
    observer: Option<Arc<dyn QueryObserver>>,
    slow_threshold: Option<Duration>,
    entity_thresholds: HashMap<&'static str, Duration>,
    backend: &'static str,
}

impl Default for Observers {
    fn default() -> Self {
        Self {
            observer: None,
            slow_threshold: None,
            entity_thresholds: HashMap::new(),
            backend: "portable",
        }
    }
}

impl std::fmt::Debug for Observers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Observers")
            .field("configured", &self.observer.is_some())
            .field("slow_threshold", &self.slow_threshold)
            .finish_non_exhaustive()
    }
}

impl Observers {
    #[must_use]
    pub fn with_query_observer<O: QueryObserver + 'static>(mut self, observer: O) -> Self {
        self.observer = Some(Arc::new(observer));
        self
    }

    #[must_use]
    pub fn with_backend(mut self, backend: &'static str) -> Self {
        self.backend = backend;
        self
    }

    #[must_use]
    pub fn with_slow_query_threshold(mut self, threshold: Duration) -> Self {
        self.slow_threshold = Some(threshold);
        self
    }

    #[must_use]
    pub fn with_entity_slow_query_threshold<E: SearchEntity>(
        mut self,
        threshold: Duration,
    ) -> Self {
        self.entity_thresholds
            .insert(E::META.entity_name, threshold);
        self
    }

    pub fn begin(&self, entity: &'static str, operation: &'static str) -> ObservationGuard {
        ObservationGuard {
            span: tracing::info_span!(
                "sift_query",
                entity,
                backend = self.backend,
                operation,
                rows_returned = tracing::field::Empty,
                duration_us = tracing::field::Empty,
                outcome = tracing::field::Empty,
            ),
            started: Instant::now(),
            entity,
            operation,
            rows: 0,
            outcome: "cancelled",
            observer: self.observer.clone(),
            threshold: self
                .entity_thresholds
                .get(entity)
                .copied()
                .or(self.slow_threshold),
        }
    }

    pub async fn observe<T>(
        &self,
        entity: &'static str,
        operation: &'static str,
        future: impl Future<Output = Result<T, SearchError>>,
        rows: impl FnOnce(&T) -> usize,
    ) -> Result<T, SearchError> {
        let mut guard = self.begin(entity, operation);
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

    /// Wrap a fallible stream in one completion observation. The guard is
    /// created before the stream is built, so planning and construction
    /// errors are reported as well as errors yielded later by the stream.
    pub fn observe_stream<T: Send + 'static>(
        &self,
        entity: &'static str,
        operation: &'static str,
        build: impl FnOnce() -> Result<
            Pin<Box<dyn Stream<Item = Result<T, SearchError>> + Send + 'static>>,
            SearchError,
        >,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<T, SearchError>> + Send + 'static>>, SearchError>
    {
        let mut guard = self.begin(entity, operation);
        let mut stream = match guard.span.in_scope(build) {
            Ok(stream) => stream,
            Err(error) => {
                guard.outcome = error_class(&error);
                return Err(error);
            }
        };
        Ok(Box::pin(futures_util::stream::poll_fn(move |cx| {
            let _entered = guard.span.enter();
            match stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(row))) => {
                    guard.rows = guard.rows.saturating_add(1);
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

fn error_class(error: &SearchError) -> &'static str {
    match error {
        SearchError::Param(_)
        | SearchError::Plan(_)
        | SearchError::Render(_)
        | SearchError::Bind(_) => "parameter",
        SearchError::Timeout(_) => "timeout",
        SearchError::Sql(_) => "database",
        SearchError::Decode { .. } => "decode",
        SearchError::Extension(_) => "extension",
        SearchError::ScanLimitExceeded { .. } | SearchError::ScanQueueFull => "budget",
        SearchError::ExportIo(_) => "output",
    }
}

pub struct ObservationGuard {
    span: tracing::Span,
    started: Instant,
    entity: &'static str,
    operation: &'static str,
    rows: usize,
    outcome: &'static str,
    observer: Option<Arc<dyn QueryObserver>>,
    threshold: Option<Duration>,
}

impl ObservationGuard {
    pub fn set_rows(&mut self, rows: usize) {
        self.rows = rows;
    }
    pub fn set_outcome(&mut self, outcome: &'static str) {
        self.outcome = outcome;
    }
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
        tracing::info!(parent: &self.span,
            outcome = self.outcome,
            rows_returned = self.rows,
            slow,
            duration_us = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX),
            "SiftQuery operation completed"
        );
        if let Some(observer) = &self.observer {
            let observation = QueryObservation {
                entity: self.entity,
                operation: self.operation,
                duration,
                rows_returned: self.rows,
                outcome: self.outcome,
                slow,
            };
            if std::panic::catch_unwind(AssertUnwindSafe(|| observer.completed(&observation)))
                .is_err()
            {
                tracing::warn!(parent: &self.span, "query observer panicked");
            }
        }
    }
}
