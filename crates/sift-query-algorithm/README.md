# sift-query-algorithm

Domain-independent algorithms shared by SiftQuery. This crate has no dependencies.

- `BoundedCounter`: inclusive limits, checked addition and unchanged state on failure.
- `BoundedLruMap`: bounded storage, constant-time promotion/eviction/removal, non-promoting reads and shared-lock MRU reads.
- `TtlLruMap`: lazy expiration using caller-supplied monotonic timestamps. Live reads do not refresh TTL; expired reads remove entries. Untouched expired entries remain bounded by capacity.
- `StableHasher64`: deterministic FNV-1a fingerprints, not cryptographic authentication.
- `is_ascii_name`: the exact `[A-Za-z][A-Za-z0-9_]*` character grammar, independent of SQL rules.

The counter is used by core's raw/parser/builder/scope budgets. LRU is used by policy validation, SQL templates and labels; TTL is used by the optional label cache. The common name grammar is shared by runtime validation and derive macros. Units, query semantics, locks, clocks, loaders and error mapping remain with callers.

API documentation: https://docs.rs/sift-query-algorithm. Licensed under Apache-2.0.
