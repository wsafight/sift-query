//! Input/result conversion contracts, registry and built-in codecs.
use crate::{FieldMeta, RowValue, ScalarValue};
use sift_query_algorithm::StableHasher64;
use std::fmt;
use std::sync::Arc;

mod business;
mod list;
#[cfg(feature = "chrono")]
mod temporal;
#[cfg(feature = "chrono")]
pub use business::DateCodec;
#[cfg(feature = "json")]
pub use business::JsonValueCodec;
pub use business::{BooleanCodec, EnumCodec, ScalarTextCodec, StringCodec};
pub use list::ListResultCodec;
#[cfg(feature = "chrono")]
pub use temporal::{DateTimeCodec, TimeCodec, TimestampCodec};

/// A server-owned input/result conversion codec.
pub trait Codec: Send + Sync + fmt::Debug {
    /// Stable name used in schema and policy fingerprints.
    fn name(&self) -> &'static str;

    /// Whether this codec handles the field.
    fn supports(&self, field: &FieldMeta) -> bool;

    /// Convert one external value into the field's validated query value.
    fn parse(&self, field: &FieldMeta, input: &str) -> Result<ScalarValue, String>;

    /// Canonical RPC representation. Encoders verify that parsing this text
    /// recovers the original database value; lossy codecs must override it.
    fn format_input(&self, field: &FieldMeta, value: &ScalarValue) -> Result<String, String> {
        self.format_result(field, value.clone().into())
            .map(|value| value.to_external_text())
    }

    /// Convert one decoded result value into its public representation.
    fn format_result(&self, _field: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        Ok(value)
    }

    /// Stable identity including codec configuration.
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        hash.write_delimited(self.name());
        hash.finish()
    }
}

/// Compatibility name for reflection-based field converters. Implementations
/// use the typed [`Codec`] contract and cannot bypass bind typing.
pub trait FieldConvertor: Codec {}
impl<T: Codec + ?Sized> FieldConvertor for T {}

/// A small, ordered collection of codecs.
#[derive(Clone, Default)]
pub struct CodecRegistry {
    codecs: Vec<Arc<dyn Codec>>,
}

impl fmt::Debug for CodecRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entries(self.codecs.iter().map(|codec| codec.name()))
            .finish()
    }
}

impl CodecRegistry {
    #[must_use]
    pub fn with_codec<C: Codec + 'static>(mut self, codec: C) -> Self {
        self.codecs.push(Arc::new(codec));
        self
    }

    pub fn push<C: Codec + 'static>(&mut self, codec: C) {
        self.codecs.push(Arc::new(codec));
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.codecs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Codec>> {
        self.codecs.iter()
    }

    pub(crate) fn find(&self, field: &FieldMeta) -> Option<&dyn Codec> {
        self.codecs
            .iter()
            .find(|codec| codec.supports(field))
            .map(Arc::as_ref)
    }

    pub(crate) fn find_arc(&self, field: &FieldMeta) -> Option<Arc<dyn Codec>> {
        self.codecs
            .iter()
            .find(|codec| codec.supports(field))
            .cloned()
    }

    pub(crate) fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        for codec in &self.codecs {
            hash.write(codec.fingerprint().to_le_bytes());
        }
        hash.finish()
    }
}
