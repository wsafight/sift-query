use super::typed;
use sift_query_core::{CustomOperator, ValueType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextConfig {
    Simple,
    English,
    German,
    French,
}
impl TextConfig {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Simple => "pg_catalog.simple",
            Self::English => "pg_catalog.english",
            Self::German => "pg_catalog.german",
            Self::French => "pg_catalog.french",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextQuery {
    Plain,
    Phrase,
    Web,
    TsQuery,
}

/// Declare a `FilterOnly<String>` field over a tsvector column/expression.
pub fn full_text(config: TextConfig, mode: TextQuery) -> CustomOperator {
    macro_rules! modes {
        ($config:literal) => {
            match mode {
                TextQuery::Plain => typed(
                    "fts_plain",
                    concat!(
                        "{field} @@ plainto_tsquery('",
                        $config,
                        "'::regconfig, {0})"
                    ),
                    ValueType::Text,
                    ValueType::Text,
                ),
                TextQuery::Phrase => typed(
                    "fts_phrase",
                    concat!(
                        "{field} @@ phraseto_tsquery('",
                        $config,
                        "'::regconfig, {0})"
                    ),
                    ValueType::Text,
                    ValueType::Text,
                ),
                TextQuery::Web => typed(
                    "fts_web",
                    concat!(
                        "{field} @@ websearch_to_tsquery('",
                        $config,
                        "'::regconfig, {0})"
                    ),
                    ValueType::Text,
                    ValueType::Text,
                ),
                TextQuery::TsQuery => typed(
                    "fts_query",
                    concat!("{field} @@ to_tsquery('", $config, "'::regconfig, {0})"),
                    ValueType::Text,
                    ValueType::Text,
                ),
            }
        };
    }
    match config {
        TextConfig::Simple => modes!("pg_catalog.simple"),
        TextConfig::English => modes!("pg_catalog.english"),
        TextConfig::German => modes!("pg_catalog.german"),
        TextConfig::French => modes!("pg_catalog.french"),
    }
}
