# sift-query-sqlx

PostgreSQL renderer and sqlx executor for SiftQuery. It implements parameterized SQL, typed/dynamic queries, counts and summaries, keyset paging, bounded streaming, named pools, schema contract checks, authorized Scope providers, server declared facets and optional web integration.

This crate uses sqlx's PostgreSQL driver only. Other database drivers and dialects are not selected implicitly. Backend-specific operators, extension checks and RLS belong to the PostgreSQL backend. The shared query model lives in `sift-query-core`.

Use the `sift-query` facade for convenient imports. API documentation: https://docs.rs/sift-query-sqlx. Licensed under Apache-2.0.

Opt-in features: `postgres-search` (typed operator catalog/full text), `postgres-types` (bigint/text arrays, int8range, inet/cidr), `pg-trgm`, `ltree`, `postgis` and `rls`. Extensions are checked explicitly at initialization and never installed automatically. RLS is an explicit transaction entry point; ordinary queries do not begin transactions or probe extensions.
