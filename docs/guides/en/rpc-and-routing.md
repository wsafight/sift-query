# Rpc And Routing

This English guide summarizes the [complete Chinese contract](../../10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md). Switch the page language to Chinese to read the full specification.

## Query RPC

`UserQuery::to_rpc` and `QueryBuilder::build_for_rpc` encode the complete user query through the established raw-parameter protocol. Named variants provide custom parameter naming. RPC includes filters, operators, logical grouping, paging, order and projection; it does not carry authorization scopes or raw SQL.

Codecs provide lossless business-value representation. JSON values preserve exact numeric encoding where enabled. GET and POST adapters consume the same query contract and limits.

## Custom operators

Register allowed operator names and typed templates on the server, then allow those names on entity fields. Unknown names, type mismatches, arity errors and registration-budget failures are rejected. No silent fallback to an arbitrary operator occurs.

## Reusable mappings

`search_entities!` composes shared field fragments and naming presets at compile time. Explicit SQLx aliases align database output and Rust decoding. This provides controlled reuse without Java reflection or runtime inheritance.

## Pool routing

Named pool routers map trusted context to registered pool names. Unknown, duplicate and invalid names fail validation. Each selected backend remains concrete, with its own SQL dialect, connection pool, execution configuration and cache.

Never derive privileged routes from unchecked query parameters. Routing chooses a registered data source; `ServerScope` still enforces row authorization.

## Temporal and list codecs

Time/date/timezone codecs declare their formats and timezone rules. List result conversion is distinct from SQL array binding or list predicates. For conversion and exports, see [query extensions](extensions.md).
