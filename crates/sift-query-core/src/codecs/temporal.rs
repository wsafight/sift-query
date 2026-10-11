use crate::{Codec, ConfigError, FieldMeta, RowValue, ScalarValue, ValueType};
use chrono::{TimeZone, Utc};
use sift_query_algorithm::StableHasher64;
use std::fmt::{Display, Write};

fn formatted(value: impl Display) -> Result<RowValue, String> {
    let mut output = String::new();
    write!(&mut output, "{value}").map_err(|_| "invalid temporal format".to_owned())?;
    Ok(RowValue::Text(output))
}
fn fingerprint(parts: &[&str]) -> u64 {
    let mut hash = StableHasher64::default();
    for part in parts {
        hash.write_delimited(part);
    }
    hash.finish()
}

#[derive(Debug, Clone, Copy)]
pub struct TimeCodec {
    pub name: &'static str,
    pub field: &'static str,
    pub format: &'static str,
}
impl Codec for TimeCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::Time
    }
    fn parse(&self, _: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        chrono::NaiveTime::parse_from_str(input, self.format)
            .or_else(|_| chrono::NaiveTime::parse_from_str(input, "%H:%M:%S%.f"))
            .map(ScalarValue::Time)
            .map_err(|_| "invalid time".to_owned())
    }
    fn format_result(&self, _: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        match value {
            RowValue::Null => Ok(RowValue::Null),
            RowValue::Time(value) => formatted(value.format(self.format)),
            _ => Err("time result has an incompatible type".to_owned()),
        }
    }
    fn format_input(&self, _: &FieldMeta, value: &ScalarValue) -> Result<String, String> {
        Ok(value.to_external_text())
    }
    fn fingerprint(&self) -> u64 {
        fingerprint(&[self.name, self.field, self.format])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DateTimeCodec {
    pub name: &'static str,
    pub field: &'static str,
    pub format: &'static str,
}
impl Codec for DateTimeCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::DateTime
    }
    fn parse(&self, _: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        chrono::NaiveDateTime::parse_from_str(input, self.format)
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(input, "%Y-%m-%dT%H:%M:%S%.f"))
            .map(ScalarValue::DateTime)
            .map_err(|_| "invalid local date/time".to_owned())
    }
    fn format_result(&self, _: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        match value {
            RowValue::Null => Ok(RowValue::Null),
            RowValue::DateTime(value) => formatted(value.format(self.format)),
            _ => Err("date/time result has an incompatible type".to_owned()),
        }
    }
    fn format_input(&self, _: &FieldMeta, value: &ScalarValue) -> Result<String, String> {
        Ok(value.to_external_text())
    }
    fn fingerprint(&self) -> u64 {
        fingerprint(&[self.name, self.field, self.format])
    }
}

/// Configured local format and IANA timezone, normalized to UTC for binding.
/// Ambiguous and nonexistent local DST times are errors. RFC 3339 and epoch
/// milliseconds are accepted as explicit instants, independent of local zone.
#[derive(Debug, Clone)]
pub struct TimestampCodec {
    name: &'static str,
    field: &'static str,
    format: &'static str,
    timezone: chrono_tz::Tz,
}
impl TimestampCodec {
    pub fn new(
        name: &'static str,
        field: &'static str,
        format: &'static str,
        timezone: &str,
    ) -> Result<Self, ConfigError> {
        if format.is_empty()
            || chrono::format::StrftimeItems::new(format)
                .any(|item| matches!(item, chrono::format::Item::Error))
        {
            return Err(ConfigError::Invalid("invalid timestamp format".to_owned()));
        }
        let timezone = timezone
            .parse()
            .map_err(|_| ConfigError::Invalid("unknown IANA timezone".to_owned()))?;
        Ok(Self {
            name,
            field,
            format,
            timezone,
        })
    }
}
impl Codec for TimestampCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::Timestamp
    }
    fn parse(&self, _: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        if let Ok(value) = chrono::DateTime::parse_from_rfc3339(input) {
            return Ok(ScalarValue::Timestamp(value.with_timezone(&Utc)));
        }
        // Honor an explicit offset in the configured format as an instant.
        if let Ok(value) = chrono::DateTime::parse_from_str(input, self.format) {
            return Ok(ScalarValue::Timestamp(value.with_timezone(&Utc)));
        }
        // Configured compact numeric dates take precedence over the epoch
        // fallback. A parsed but ambiguous local time must still fail closed.
        if let Ok(local) = chrono::NaiveDateTime::parse_from_str(input, self.format) {
            return self
                .timezone
                .from_local_datetime(&local)
                .single()
                .map(|value| ScalarValue::Timestamp(value.with_timezone(&Utc)))
                .ok_or_else(|| "ambiguous or nonexistent local time".to_owned());
        }
        if !input.is_empty()
            && input
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'-')
        {
            let millis = input
                .parse()
                .map_err(|_| "invalid epoch milliseconds".to_owned())?;
            return chrono::DateTime::from_timestamp_millis(millis)
                .map(ScalarValue::Timestamp)
                .ok_or_else(|| "epoch milliseconds out of range".to_owned());
        }
        Err("invalid timestamp".to_owned())
    }
    fn format_result(&self, _: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        match value {
            RowValue::Null => Ok(RowValue::Null),
            RowValue::Timestamp(value) => {
                formatted(value.with_timezone(&self.timezone).format(self.format))
            }
            _ => Err("timestamp result has an incompatible type".to_owned()),
        }
    }
    fn format_input(&self, _: &FieldMeta, value: &ScalarValue) -> Result<String, String> {
        Ok(value.to_external_text())
    }
    fn fingerprint(&self) -> u64 {
        fingerprint(&[self.name, self.field, self.format, self.timezone.name()])
    }
}
