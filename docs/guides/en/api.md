# Api

This English guide summarizes the [complete Chinese contract](../../04-RUST-API.md). Switch the page language to Chinese to read the full specification.

## Entity declarations

Derive `SearchEntity` to define static query metadata and `sqlx::FromRow` to decode rows. Entity attributes declare the FROM source, stable/default ordering and trusted fixed predicates. Field attributes define public names, database expressions, allowed operators, filter/select/sort capabilities, roles, codecs and labels.

Use the [complete SQLite quick start](quick-start.md) as a runnable declaration. Mapping SQL and identifiers are server-owned; request parameters use only public field names.

## Query input

`RawParams` preserves raw parameter semantics. `search.parse::<E>` returns a typed `UserQuery<E>` after policy validation. `QueryBuilder<E>` constructs the same query model programmatically. Typed field references prevent arbitrary field names from entering Rust-built queries.

## Scope and authorization

`ServerScope<E>` carries trusted row authorization; `GroupScope<E>` handles aggregate constraints. Applications can implement `ScopeProvider<C, E>` and use authorized execution entry points to derive a scope from trusted context. PostgreSQL, MySQL and SQLite expose authorized search/map/count entry points.

`FieldAccessPolicy` restricts field use and selection. Scope intersection narrows access. An unrestricted scope is appropriate only when the application intends unrestricted access.

## Execution entry points

| Intent | Entry point |
| --- | --- |
| Typed page and count | `search` |
| Dynamic map page | `search_map` |
| Page with summaries | `search_with`, `search_map_with` |
| Rows without count | `search_list` |
| Count only | `count` |
| Cursor page without count | `search_cursor`, `search_map_cursor` |
| Caller-owned connection | `search_on`, `search_map_on` |
| Bounded map scan | `scan_map_stream` |

Check [backend selection](backend-selection.md) for native and PostgreSQL-specific APIs, including static projections, schema validation, facets and diagnostics. The generated [docs.rs API](https://docs.rs/sift-query) is the reference for exact signatures and feature gates.

## Configuration and errors

`QueryPolicy` configures parsing and planning budgets, codecs and registered operators. Backend execution policy controls timeouts, strategies and template cache capacity. Parsing, planning, database, budget and cancellation errors remain distinguishable. Do not expose private SQL or bind data through public HTTP errors.
