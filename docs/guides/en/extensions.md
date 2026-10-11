# Extensions

This English guide summarizes the [complete Chinese contract](../../08-P0-EXTENSIONS.md). Switch the page language to Chinese to read the full specification.

## Input and value conversion

Parameter filters run within strict input budgets. Registered `Codec` implementations give fields a typed business representation without changing field allowlists or authorization. Input bind values remain separate from result/CSV formatting; cursors use database ordering values.

## Bounded scan and CSV

Scan streams use a single bounded query with row, deadline and concurrency limits. CSV export applies declared columns, batching, conversion/enrichment, formula-injection protection and writer deadlines. A failed writer may already have emitted partial output; retrying the same response stream is inappropriate.

## Enrichment and observation

`PageEnricher` augments an entire page. Labels have explicit dictionaries, missing-value behavior, budgets, caching and concurrency limits. A declared label name does not make a client-provided dictionary trustworthy.

`QueryObserver` and tracing report query/stream lifecycle events. They are hooks for telemetry, not authorization. Result filtering cannot replace `ServerScope` and does not rewrite the database count.

## External connections

Caller-owned connection and transaction methods let an application compose queries. The library does not commit or roll back the caller's transaction or silently change its ownership. External entry points reject incompatible snapshot/parallel combinations. The caller must clean up after success, failure and cancellation.

## PostgreSQL production APIs

`validate_schema::<E>()` validates planning and result-column contracts. Server-declared facets and time/numeric buckets reuse filtering, scopes, field permissions and budgets. For consistent list/facet results, use an explicit caller-owned read-only transaction with the corresponding `_on` API.
