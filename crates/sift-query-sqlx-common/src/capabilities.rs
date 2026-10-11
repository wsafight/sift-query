//! Explicit capability declarations shared by all `SQLx` backends.
//!
//! The matrix is intentionally descriptive: it does not probe a database or
//! silently enable a feature. Applications can use it to build metadata and
//! deployment checks before accepting traffic.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind {
    PostgreSql,
    MySql,
    MariaDb,
    Sqlite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[repr(u8)]
pub enum BackendCapability {
    TypedSearch,
    DynamicMap,
    CountAndSummaries,
    CursorPaging,
    StreamingScan,
    CsvExport,
    LabelEnrichment,
    CallerOwnedConnection,
    CustomOperators,
    ServerScope,
    JsonValues,
    Arrays,
    FullText,
    Spatial,
    Transactions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilitySet {
    pub backend: BackendKind,
    capabilities: &'static [BackendCapability],
}

impl BackendCapabilitySet {
    #[must_use]
    pub const fn supports(self, capability: BackendCapability) -> bool {
        let mut index = 0;
        while index < self.capabilities.len() {
            if self.capabilities[index] as u8 == capability as u8 {
                return true;
            }
            index += 1;
        }
        false
    }

    #[must_use]
    pub const fn all(self) -> &'static [BackendCapability] {
        self.capabilities
    }
}

const MYSQL: &[BackendCapability] = &[
    BackendCapability::TypedSearch,
    BackendCapability::DynamicMap,
    BackendCapability::CountAndSummaries,
    BackendCapability::CursorPaging,
    BackendCapability::StreamingScan,
    BackendCapability::CsvExport,
    BackendCapability::LabelEnrichment,
    BackendCapability::CallerOwnedConnection,
    BackendCapability::CustomOperators,
    BackendCapability::ServerScope,
    BackendCapability::Transactions,
];
const SQLITE: &[BackendCapability] = &[
    BackendCapability::TypedSearch,
    BackendCapability::DynamicMap,
    BackendCapability::CountAndSummaries,
    BackendCapability::CursorPaging,
    BackendCapability::StreamingScan,
    BackendCapability::CsvExport,
    BackendCapability::LabelEnrichment,
    BackendCapability::CallerOwnedConnection,
    BackendCapability::CustomOperators,
    BackendCapability::ServerScope,
    BackendCapability::Transactions,
];
const POSTGRES: &[BackendCapability] = &[
    BackendCapability::TypedSearch,
    BackendCapability::DynamicMap,
    BackendCapability::CountAndSummaries,
    BackendCapability::CursorPaging,
    BackendCapability::StreamingScan,
    BackendCapability::CsvExport,
    BackendCapability::LabelEnrichment,
    BackendCapability::CallerOwnedConnection,
    BackendCapability::CustomOperators,
    BackendCapability::ServerScope,
    BackendCapability::JsonValues,
    BackendCapability::Arrays,
    BackendCapability::FullText,
    BackendCapability::Spatial,
    BackendCapability::Transactions,
];

#[must_use]
pub const fn backend_capabilities(backend: BackendKind) -> BackendCapabilitySet {
    BackendCapabilitySet {
        backend,
        capabilities: match backend {
            BackendKind::PostgreSql => POSTGRES,
            BackendKind::MySql | BackendKind::MariaDb => MYSQL,
            BackendKind::Sqlite => SQLITE,
        },
    }
}
