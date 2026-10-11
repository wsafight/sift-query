use super::{
    DefaultParamFilter, IndexedArrayParamFilter, JsonArrayParamFilter, RawParamFilter,
    SuffixOperatorParamFilter,
};
use crate::{EntityMeta, Operator, ParamError, QueryLimits, RawParams};
use sift_query_algorithm::StableHasher64;

impl RawParamFilter for IndexedArrayParamFilter {
    fn filter(
        &self,
        entity: &'static EntityMeta,
        input: &RawParams,
    ) -> Result<RawParams, ParamError> {
        self.filter_with_limits(entity, input, &QueryLimits::default())
    }

    fn filter_with_limits(
        &self,
        _entity: &'static EntityMeta,
        input: &RawParams,
        limits: &QueryLimits,
    ) -> Result<RawParams, ParamError> {
        let mut next_indices = std::collections::HashMap::<String, usize>::new();
        let mut entries = Vec::with_capacity(input.len());
        for (key, value) in input.entries() {
            if let Some((field, index)) = parse_indexed_key(key) {
                entries.push((format!("{field}-{index}"), value.clone()));
                continue;
            }
            if let Some(field) = key.strip_suffix("[]") {
                let index = next_indices.entry(field.to_owned()).or_default();
                entries.push((format!("{field}-{index}"), value.clone()));
                *index = index.saturating_add(1);
            } else {
                entries.push((key.clone(), value.clone()));
            }
        }
        RawParams::from_entries(entries, limits)
    }
}

impl RawParamFilter for JsonArrayParamFilter {
    fn filter(
        &self,
        entity: &'static EntityMeta,
        input: &RawParams,
    ) -> Result<RawParams, ParamError> {
        self.filter_with_limits(entity, input, &QueryLimits::default())
    }

    fn filter_with_limits(
        &self,
        entity: &'static EntityMeta,
        input: &RawParams,
        limits: &QueryLimits,
    ) -> Result<RawParams, ParamError> {
        let mut entries = Vec::with_capacity(input.len());
        for (key, value) in input.entries() {
            // Control parameters and unrecognized fields keep strict parsing.
            if entity
                .resolve_field(key.rsplit('.').next().unwrap_or(key))
                .is_none()
            {
                entries.push((key.clone(), value.clone()));
                continue;
            }
            #[cfg(feature = "json")]
            let array = if value.trim_start().starts_with('[') {
                Some(
                    serde_json::from_str::<Vec<serde_json::Value>>(value).map_err(|_| {
                        ParamError::InvalidValue {
                            field: key.clone(),
                            reason: "expected a JSON scalar array".to_owned(),
                        }
                    })?,
                )
            } else {
                None
            };
            #[cfg(not(feature = "json"))]
            let array: Option<Vec<()>> = None;

            if let Some(array) = array {
                if array.is_empty() {
                    return Err(ParamError::InvalidValue {
                        field: key.clone(),
                        reason: "array must not be empty".to_owned(),
                    });
                }
                #[cfg(feature = "json")]
                for (index, item) in array.into_iter().enumerate() {
                    let value = match item {
                        serde_json::Value::String(value) => value,
                        other @ (serde_json::Value::Bool(_) | serde_json::Value::Number(_)) => {
                            other.to_string()
                        }
                        _ => {
                            return Err(ParamError::InvalidValue {
                                field: key.clone(),
                                reason: "array elements must be non-null scalars".to_owned(),
                            });
                        }
                    };
                    entries.push((format!("{key}-{index}"), value));
                }
                continue;
            }
            entries.push((key.clone(), value.clone()));
        }
        RawParams::from_entries(entries, limits)
    }
}

impl RawParamFilter for SuffixOperatorParamFilter {
    fn filter(
        &self,
        entity: &'static EntityMeta,
        input: &RawParams,
    ) -> Result<RawParams, ParamError> {
        self.filter_with_limits(entity, input, &QueryLimits::default())
    }

    fn filter_with_limits(
        &self,
        _entity: &'static EntityMeta,
        input: &RawParams,
        limits: &QueryLimits,
    ) -> Result<RawParams, ParamError> {
        let mut entries = Vec::with_capacity(input.len() + 2);
        let mut emitted = std::collections::HashMap::new();
        for (key, value) in input.entries() {
            let Some((field, suffix)) = key.split_once('-') else {
                entries.push((key.clone(), value.clone()));
                continue;
            };
            let (operator, ignore_case) = suffix
                .split_once('-')
                .map_or((suffix, false), |(operator, flag)| (operator, flag == "ic"));
            if field.is_empty() || suffix.split_once('-').is_some_and(|(_, flag)| flag != "ic") {
                entries.push((key.clone(), value.clone()));
                continue;
            }
            let Some(parsed_operator) = Operator::parse(operator) else {
                entries.push((key.clone(), value.clone()));
                continue;
            };
            let first = if let Some(previous) = emitted.get(field) {
                if *previous != (parsed_operator, ignore_case) {
                    return Err(ParamError::ConflictingParameters(field.to_owned()));
                }
                false
            } else {
                emitted.insert(field, (parsed_operator, ignore_case));
                true
            };
            if !parsed_operator.accepts_arity(0) || !value.is_empty() {
                entries.push((field.to_owned(), value.clone()));
            }
            if first {
                entries.push((format!("{field}-op"), operator.to_owned()));
                if ignore_case {
                    entries.push((format!("{field}-ic"), "true".to_owned()));
                }
            }
        }
        RawParams::from_entries(entries, limits)
    }
}

impl DefaultParamFilter {
    #[must_use]
    pub fn new(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }
}

impl RawParamFilter for DefaultParamFilter {
    fn filter(
        &self,
        entity: &'static EntityMeta,
        input: &RawParams,
    ) -> Result<RawParams, ParamError> {
        self.filter_with_limits(entity, input, &QueryLimits::default())
    }

    fn filter_with_limits(
        &self,
        _entity: &'static EntityMeta,
        input: &RawParams,
        limits: &QueryLimits,
    ) -> Result<RawParams, ParamError> {
        let mut entries = self
            .entries
            .iter()
            .filter(|(key, _)| !input.entries().iter().any(|(provided, _)| provided == key))
            .cloned()
            .collect::<Vec<_>>();
        entries.extend(input.entries().iter().cloned());
        RawParams::from_entries(entries, limits)
    }

    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        hash.write_delimited("default-params-v1");
        for (key, value) in &self.entries {
            hash.write_delimited(key);
            hash.write_delimited(value);
        }
        hash.finish()
    }
}

fn parse_indexed_key(key: &str) -> Option<(&str, usize)> {
    let (field, suffix) = key.strip_suffix(']')?.rsplit_once('[')?;
    if field.is_empty() || suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((field, suffix.parse().ok()?))
}
