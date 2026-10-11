use super::{Cursor, CursorValue, invalid_cursor};
use crate::{ConfigError, OrderBy, ParamError, ScalarValue, SearchEntity};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fmt;
type HmacSha256 = Hmac<Sha256>;
const VERSION: u8 = 1;
const SIGNATURE_BYTES: usize = 32;

#[derive(Clone)]
pub struct CursorCodec {
    key: Vec<u8>,
}

impl fmt::Debug for CursorCodec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CursorCodec")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl CursorCodec {
    pub fn new(key: impl AsRef<[u8]>) -> Result<Self, ConfigError> {
        let key = key.as_ref();
        if key.len() < 32 {
            return Err(ConfigError::Invalid(
                "cursor signing key must contain at least 32 bytes".to_owned(),
            ));
        }
        Ok(Self { key: key.to_vec() })
    }

    pub fn encode<E: SearchEntity>(
        &self,
        order: &[OrderBy],
        values: &[CursorValue],
    ) -> Result<String, ParamError> {
        if order.len() != values.len() || order.len() > usize::from(u8::MAX) {
            return Err(invalid_cursor("cursor value count does not match ordering"));
        }
        let mut payload = Vec::new();
        payload.push(VERSION);
        payload.extend_from_slice(&E::META.schema_fingerprint.to_be_bytes());
        payload.push(u8::try_from(order.len()).map_err(|_| invalid_cursor("too many keys"))?);
        for (item, value) in order.iter().zip(values) {
            if E::cursor_key_non_null(item.field) && matches!(value, CursorValue::Null) {
                return Err(invalid_cursor("non-null cursor key cannot contain NULL"));
            }
            payload.extend_from_slice(&item.field.0.to_be_bytes());
            payload.push(match item.direction {
                crate::SortOrder::Asc => 0,
                crate::SortOrder::Desc => 1,
            });
            payload.push(match item.nulls {
                crate::NullOrder::First => 0,
                crate::NullOrder::Last => 1,
            });
            encode_value(value, &mut payload)?;
        }
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|_| invalid_cursor("invalid signing key"))?;
        mac.update(&payload);
        payload.extend_from_slice(&mac.finalize().into_bytes());
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload))
    }

    pub fn decode<E: SearchEntity>(
        &self,
        token: &str,
        order: &[OrderBy],
    ) -> Result<Cursor, ParamError> {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token)
            .map_err(|_| invalid_cursor("cursor is not valid base64url"))?;
        if bytes.len() <= SIGNATURE_BYTES {
            return Err(invalid_cursor("cursor is truncated"));
        }
        let split = bytes.len() - SIGNATURE_BYTES;
        let (payload, signature) = bytes.split_at(split);
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|_| invalid_cursor("invalid signing key"))?;
        mac.update(payload);
        mac.verify_slice(signature)
            .map_err(|_| invalid_cursor("cursor signature is invalid"))?;
        let mut reader = Reader::new(payload);
        if reader.u8()? != VERSION {
            return Err(invalid_cursor("unsupported cursor version"));
        }
        if reader.u64()? != E::META.schema_fingerprint {
            return Err(invalid_cursor("cursor belongs to another entity schema"));
        }
        let count = usize::from(reader.u8()?);
        if count != order.len() {
            return Err(invalid_cursor("cursor ordering length changed"));
        }
        let mut values = Vec::with_capacity(count);
        for item in order {
            if reader.u16()? != item.field.0
                || reader.u8()?
                    != match item.direction {
                        crate::SortOrder::Asc => 0,
                        crate::SortOrder::Desc => 1,
                    }
                || reader.u8()?
                    != match item.nulls {
                        crate::NullOrder::First => 0,
                        crate::NullOrder::Last => 1,
                    }
            {
                return Err(invalid_cursor("cursor ordering changed"));
            }
            let value = decode_value(&mut reader)?;
            if E::cursor_key_non_null(item.field) && matches!(value, CursorValue::Null) {
                return Err(invalid_cursor("non-null cursor key cannot contain NULL"));
            }
            values.push(value);
        }
        if !reader.is_empty() {
            return Err(invalid_cursor("cursor contains trailing data"));
        }
        Ok(Cursor { values })
    }
}

fn encode_value(value: &CursorValue, output: &mut Vec<u8>) -> Result<(), ParamError> {
    match value {
        CursorValue::Null => output.push(0),
        CursorValue::Scalar(value) => match value {
            ScalarValue::Bool(value) => output.extend_from_slice(&[1, u8::from(*value)]),
            ScalarValue::I32(value) => {
                output.push(2);
                output.extend_from_slice(&value.to_be_bytes());
            }
            ScalarValue::I64(value) => {
                output.push(3);
                output.extend_from_slice(&value.to_be_bytes());
            }
            ScalarValue::F32(value) => {
                output.push(4);
                output.extend_from_slice(&value.to_bits().to_be_bytes());
            }
            ScalarValue::F64(value) => {
                output.push(5);
                output.extend_from_slice(&value.to_bits().to_be_bytes());
            }
            #[cfg(feature = "decimal")]
            ScalarValue::Decimal(value) => encode_bytes(6, value.to_string().as_bytes(), output)?,
            ScalarValue::Text(value) => encode_bytes(7, value.as_bytes(), output)?,
            ScalarValue::Bytes(value) => encode_bytes(8, value, output)?,
            #[cfg(feature = "chrono")]
            ScalarValue::Date(value) => encode_bytes(9, value.to_string().as_bytes(), output)?,
            #[cfg(feature = "chrono")]
            ScalarValue::Time(value) => encode_bytes(10, value.to_string().as_bytes(), output)?,
            #[cfg(feature = "chrono")]
            ScalarValue::DateTime(value) => encode_bytes(
                11,
                value.format("%Y-%m-%dT%H:%M:%S%.f").to_string().as_bytes(),
                output,
            )?,
            #[cfg(feature = "chrono")]
            ScalarValue::Timestamp(value) => {
                encode_bytes(12, value.to_rfc3339().as_bytes(), output)?;
            }
            #[cfg(feature = "uuid")]
            ScalarValue::Uuid(value) => encode_bytes(13, value.as_bytes(), output)?,
            #[cfg(feature = "json")]
            ScalarValue::Json(_) => return Err(invalid_cursor("JSON cannot be a cursor key")),
            #[cfg(feature = "postgres-types")]
            ScalarValue::ArrayI64(_)
            | ScalarValue::ArrayText(_)
            | ScalarValue::Int8Range(_)
            | ScalarValue::Network(_) => {
                return Err(invalid_cursor("native types cannot be cursor keys"));
            }
        },
    }
    Ok(())
}

fn encode_bytes(tag: u8, value: &[u8], output: &mut Vec<u8>) -> Result<(), ParamError> {
    let length = u32::try_from(value.len()).map_err(|_| invalid_cursor("cursor value too long"))?;
    output.push(tag);
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
    Ok(())
}

fn decode_value(reader: &mut Reader<'_>) -> Result<CursorValue, ParamError> {
    let tag = reader.u8()?;
    let value = match tag {
        0 => return Ok(CursorValue::Null),
        1 => ScalarValue::Bool(reader.u8()? != 0),
        2 => ScalarValue::I32(i32::from_be_bytes(reader.array()?)),
        3 => ScalarValue::I64(i64::from_be_bytes(reader.array()?)),
        4 => ScalarValue::F32(f32::from_bits(u32::from_be_bytes(reader.array()?))),
        5 => ScalarValue::F64(f64::from_bits(u64::from_be_bytes(reader.array()?))),
        #[cfg(feature = "decimal")]
        6 => ScalarValue::Decimal(
            reader
                .string()?
                .parse()
                .map_err(|_| invalid_cursor("invalid decimal cursor value"))?,
        ),
        7 => ScalarValue::Text(reader.string()?.to_owned()),
        8 => ScalarValue::Bytes(reader.bytes()?.to_vec()),
        #[cfg(feature = "chrono")]
        9 => ScalarValue::Date(
            reader
                .string()?
                .parse()
                .map_err(|_| invalid_cursor("invalid date cursor value"))?,
        ),
        #[cfg(feature = "chrono")]
        10 => ScalarValue::Time(
            reader
                .string()?
                .parse()
                .map_err(|_| invalid_cursor("invalid time cursor value"))?,
        ),
        #[cfg(feature = "chrono")]
        11 => ScalarValue::DateTime(
            // `NaiveDateTime`'s own `Display`/`FromStr` pair is not symmetric,
            // so the value is written and read with one explicit format.
            chrono::NaiveDateTime::parse_from_str(reader.string()?, "%Y-%m-%dT%H:%M:%S%.f")
                .map_err(|_| invalid_cursor("invalid datetime cursor value"))?,
        ),
        #[cfg(feature = "chrono")]
        12 => ScalarValue::Timestamp(
            chrono::DateTime::parse_from_rfc3339(reader.string()?)
                .map_err(|_| invalid_cursor("invalid timestamp cursor value"))?
                .to_utc(),
        ),
        #[cfg(feature = "uuid")]
        13 => ScalarValue::Uuid(
            uuid::Uuid::from_slice(reader.bytes()?)
                .map_err(|_| invalid_cursor("invalid UUID cursor value"))?,
        ),
        _ => return Err(invalid_cursor("unknown cursor value type")),
    };
    Ok(CursorValue::Scalar(value))
}

struct Reader<'input> {
    input: &'input [u8],
    position: usize,
}

impl<'input> Reader<'input> {
    const fn new(input: &'input [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn is_empty(&self) -> bool {
        self.position == self.input.len()
    }

    fn u8(&mut self) -> Result<u8, ParamError> {
        let value = *self
            .input
            .get(self.position)
            .ok_or_else(|| invalid_cursor("cursor is truncated"))?;
        self.position += 1;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, ParamError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ParamError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ParamError> {
        let end = self
            .position
            .checked_add(N)
            .ok_or_else(|| invalid_cursor("cursor is truncated"))?;
        let slice = self
            .input
            .get(self.position..end)
            .ok_or_else(|| invalid_cursor("cursor is truncated"))?;
        self.position = end;
        slice
            .try_into()
            .map_err(|_| invalid_cursor("cursor is truncated"))
    }

    fn bytes(&mut self) -> Result<&'input [u8], ParamError> {
        let length = usize::try_from(u32::from_be_bytes(self.array()?))
            .map_err(|_| invalid_cursor("cursor value is too long"))?;
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| invalid_cursor("cursor is truncated"))?;
        let value = self
            .input
            .get(self.position..end)
            .ok_or_else(|| invalid_cursor("cursor is truncated"))?;
        self.position = end;
        Ok(value)
    }

    fn string(&mut self) -> Result<&'input str, ParamError> {
        std::str::from_utf8(self.bytes()?).map_err(|_| invalid_cursor("cursor value is not UTF-8"))
    }
}
