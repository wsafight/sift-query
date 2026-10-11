# Semantics

This English guide summarizes the [complete Chinese contract](../../03-QUERY-SEMANTICS.md). Switch the page language to Chinese to read the full specification.

## Field roles and phases

Scalar and filter-only fields produce row predicates in WHERE. Group keys also determine GROUP BY. Aggregate fields produce group predicates in HAVING. AND expressions can be split by phase; an OR expression crossing those phases is rejected.

Final row predicates combine entity constraints, server scope and user conditions. Group predicates similarly combine fixed HAVING, trusted group scope and user aggregate conditions. A user query cannot widen authorization.

## Projection and DISTINCT

Typed `search` decodes the complete entity. `search_map` supports selectable-field projections. Static projections use their compile-time mapping. Field access policy restricts filters, ordering, returned fields and aggregates.

DISTINCT applies to the requested read model. Hidden ordering columns must not silently change deduplication. Grouped queries derive keys from metadata rather than accepting a request-supplied GROUP BY fragment.

## Ordering and paging

The application declares a unique, stable final order. Offset budgets include calculated offsets. Signed cursors carry ordering values and schema/order identity; keyset conditions affect the list without changing the full-query count or summaries.

`search_cursor` and `search_map_cursor` return a `CursorPage` using at most `size + 1` rows and no COUNT. PostgreSQL also supports `search_as_cursor`. Nullable ordering preserves explicit NULL semantics. `cursor_non_null` is an application assertion about the database expression.

## Counts and aggregates

`search` returns rows and count; `search_with` additionally requests summaries. `search_list` needs only rows, while `count` does not render ordering or paging into its count SQL. SUM output types may differ from input types; PostgreSQL bigint SUM uses a numeric/Decimal result. Check backend decoding contracts rather than assuming all aggregates return the field's Rust type.

## Consistency and extensions

Ordinary separate queries are best effort. PostgreSQL can explicitly select parallel or snapshot strategies. Caller-owned transactions remain the caller's responsibility. Result filters and enrichers cannot replace database authorization scopes.
