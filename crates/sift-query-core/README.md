# sift-query-core

Backend-independent query metadata, strict parameter parsing, typed predicates, scopes, paging, budgets and codecs for SiftQuery. It does not depend on sqlx, database drivers or web frameworks. Optional scalar representations are selected through Cargo features.

Use the `sift-query` facade for derive macros and backend execution. API documentation: https://docs.rs/sift-query-core. Licensed under Apache-2.0.

Optional `metadata` and `rsql` expose public query descriptions/OpenAPI fragments and a bounded RSQL subset. `postgres-types` adds structured query values without introducing a driver; execution and native decoding belong to the PostgreSQL backend.
