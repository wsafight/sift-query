use std::time::Duration;

use sift_query_core::{ParamError, QueryLimits, RawParams};

use super::{PublicError, WebError};

/// GET query / POST form parameters, preserving ordered repeated values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchParams(pub RawParams);

/// Framework request configuration. Use the same limits as the query policy.
/// Install as an axum extension, actix app data, or poem endpoint data.
#[derive(Debug, Clone)]
pub struct SearchParamConfig {
    pub limits: QueryLimits,
    pub body_timeout: Duration,
}

impl Default for SearchParamConfig {
    fn default() -> Self {
        Self {
            limits: QueryLimits::default(),
            body_timeout: Duration::from_secs(5),
        }
    }
}

impl SearchParams {
    #[must_use]
    pub fn raw(&self) -> &RawParams {
        &self.0
    }

    /// Validates transport metadata before reading the body. POST query
    /// parameters are rejected rather than merged with form parameters.
    pub fn validate_request(
        method: &str,
        query: Option<&str>,
        content_type: Option<&str>,
    ) -> Result<(), WebError> {
        match method {
            "GET" => Ok(()),
            "POST" => {
                if query.is_some_and(|query| !query.is_empty()) {
                    return Err(error(400, "conflicting_query_body"));
                }
                let mut parts = content_type.unwrap_or_default().split(';');
                if !parts
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case("application/x-www-form-urlencoded")
                    || parts.any(|part| {
                        let Some((name, value)) = part.trim().split_once('=') else {
                            return true;
                        };
                        !name.trim().eq_ignore_ascii_case("charset")
                            || !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
                    })
                {
                    return Err(error(415, "unsupported_query_media_type"));
                }
                Ok(())
            }
            _ => Err(error(405, "query_method_not_allowed")),
        }
    }

    /// Parses an already bounded request body using the same core protocol
    /// and budgets as GET. Does not bypass entity or query validation.
    pub fn parse_request(
        method: &str,
        query: Option<&str>,
        content_type: Option<&str>,
        body: &[u8],
        limits: &QueryLimits,
    ) -> Result<Self, WebError> {
        Self::validate_request(method, query, content_type)?;
        let input = if method == "GET" {
            query.unwrap_or_default()
        } else {
            if body.len() > limits.max_raw_query_bytes {
                return Err(error(413, "query_body_too_large"));
            }
            std::str::from_utf8(body).map_err(|_| WebError::from(ParamError::InvalidEncoding))?
        };
        RawParams::parse_with_limits(input, limits)
            .map(Self)
            .map_err(WebError::from)
    }
}

pub(super) const fn error(status: u16, code: &'static str) -> WebError {
    WebError(PublicError { status, code })
}

#[cfg(any(feature = "axum", feature = "actix-web", feature = "poem", test))]
pub(super) async fn read_body<S, B, E>(
    stream: S,
    config: &SearchParamConfig,
) -> Result<Vec<u8>, WebError>
where
    S: futures_core::Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
{
    use futures_util::StreamExt;
    tokio::time::timeout(config.body_timeout, async {
        futures_util::pin_mut!(stream);
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| error(400, "invalid_query_body"))?;
            let chunk = chunk.as_ref();
            if chunk.len()
                > config
                    .limits
                    .max_raw_query_bytes
                    .saturating_sub(bytes.len())
            {
                return Err(error(413, "query_body_too_large"));
            }
            bytes.extend_from_slice(chunk);
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| error(408, "query_body_timeout"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chunked_bodies_are_bounded_and_pending_readers_expire() {
        let config = SearchParamConfig {
            limits: QueryLimits {
                max_raw_query_bytes: 3,
                ..Default::default()
            },
            body_timeout: Duration::from_millis(5),
        };
        let chunks =
            futures_util::stream::iter([Ok::<_, ()>(b"id".as_slice()), Ok(b"=1".as_slice())]);
        assert_eq!(
            read_body(chunks, &config).await.unwrap_err().0.code,
            "query_body_too_large"
        );
        let pending = futures_util::stream::pending::<Result<Vec<u8>, ()>>();
        assert_eq!(
            read_body(pending, &config).await.unwrap_err().0.code,
            "query_body_timeout"
        );
    }
}
