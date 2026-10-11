//! Ordered, multi-value raw parameters with bounded URL encoding.
use crate::{ParamError, QueryLimits};
use sift_query_algorithm::BoundedCounter;
use url::{decode_component, encode_component, encoded_len};
mod url;

/// Decoded, ordered, multi-value query parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawParams {
    entries: Vec<(String, String)>,
    raw_bytes: usize,
    decoded_bytes: usize,
}

impl RawParams {
    pub fn parse(query: &str) -> Result<Self, ParamError> {
        Self::parse_with_limits(query, &QueryLimits::default())
    }

    pub fn parse_with_limits(query: &str, limits: &QueryLimits) -> Result<Self, ParamError> {
        if query.len() > limits.max_raw_query_bytes {
            return Err(ParamError::QueryTooLong);
        }
        if query.is_empty() {
            return Ok(Self::default());
        }

        let mut entries = Vec::new();
        let mut decoded_bytes = BoundedCounter::new(limits.max_decoded_bytes);
        for item in query.split('&') {
            if entries.len() >= limits.max_raw_items {
                return Err(ParamError::TooComplex("raw parameter item count"));
            }
            if item.is_empty() {
                return Err(ParamError::InvalidName(String::new()));
            }
            let (raw_key, raw_value) = item.split_once('=').unwrap_or((item, ""));
            let key = decode_component(raw_key)?;
            let value = decode_component(raw_value)?;
            if key.is_empty() || key.len() > limits.max_key_bytes {
                return Err(ParamError::InvalidName(key));
            }
            if value.len() > limits.max_value_bytes {
                return Err(ParamError::TooComplex("single parameter value length"));
            }
            decoded_bytes
                .try_add(key.len())
                .and_then(|()| decoded_bytes.try_add(value.len()))
                .map_err(|_| ParamError::TooComplex("decoded parameter bytes"))?;
            entries.push((key, value));
        }
        Ok(Self {
            entries,
            raw_bytes: query.len(),
            decoded_bytes: decoded_bytes.used(),
        })
    }

    /// Builds ordered, repeated parameters produced by a trusted normalization
    /// filter. The same byte and item limits as URL parsing are enforced.
    pub fn from_entries(
        entries: impl IntoIterator<Item = (String, String)>,
        limits: &QueryLimits,
    ) -> Result<Self, ParamError> {
        let mut output = Vec::new();
        let mut decoded_bytes = BoundedCounter::new(limits.max_decoded_bytes);
        let mut raw_bytes = 0usize;
        for (key, value) in entries {
            if output.len() >= limits.max_raw_items {
                return Err(ParamError::TooComplex("filtered parameter item count"));
            }
            if key.is_empty() || key.len() > limits.max_key_bytes {
                return Err(ParamError::InvalidName(key.clone()));
            }
            if value.len() > limits.max_value_bytes {
                return Err(ParamError::TooComplex("filtered parameter value length"));
            }
            decoded_bytes
                .try_add(key.len())
                .and_then(|()| decoded_bytes.try_add(value.len()))
                .map_err(|_| ParamError::TooComplex("filtered parameter bytes"))?;
            raw_bytes = raw_bytes
                .saturating_add(encoded_len(&key))
                .saturating_add(encoded_len(&value))
                .saturating_add(if output.is_empty() { 1 } else { 2 });
            if raw_bytes > limits.max_raw_query_bytes {
                return Err(ParamError::QueryTooLong);
            }
            output.push((key, value));
        }
        Ok(Self {
            entries: output,
            raw_bytes,
            decoded_bytes: decoded_bytes.used(),
        })
    }

    #[must_use]
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    /// Encodes this ordered, repeated parameter set as a stable URL query.
    #[must_use]
    pub fn to_query_string(&self) -> String {
        let mut output = String::with_capacity(self.raw_bytes);
        for (index, (key, value)) in self.entries.iter().enumerate() {
            if index > 0 {
                output.push('&');
            }
            encode_component(key, &mut output);
            output.push('=');
            encode_component(value, &mut output);
        }
        output
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub const fn raw_bytes(&self) -> usize {
        self.raw_bytes
    }

    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
}
