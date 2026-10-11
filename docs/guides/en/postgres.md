# Postgres

This English guide summarizes the [complete Chinese contract](../../12-POSTGRES-CAPABILITIES.md). Switch the page language to Chinese to read the full specification.

## Opt-in native capabilities

PostgreSQL features are disabled by default except for the standard PostgreSQL backend itself. Register capabilities in `PgCapabilities`, apply them to `QueryPolicy`, and allow operator names on specific entity fields. Explicitly call `check(pool)` during application initialization when database extension requirements must be checked.

| Feature | Capability |
| --- | --- |
| `postgres-search` | Full text and PostgreSQL capability registry |
| `json` + `postgres-search` | JSONB containment and keys |
| `postgres-types` | bigint/text arrays, int8range, inet/cidr |
| `pg-trgm` | Similarity filtering and controlled distance ordering |
| `ltree` | Ancestor/descendant predicates and path codec |
| `postgis` | Spatial predicates and EWKT codec |

## Full text and JSONB

Full-text filters include `fts_plain`, `fts_phrase`, `fts_web` and `fts_query`. A trusted text configuration determines the parser; text remains parameter-bound. Filter-only fields can point to a tsvector column/expression without promising native tsvector result decoding.

JSONB operators distinguish JSON null, SQL NULL and missing keys. HTTP structured values use an encoded JSON string; exact numbers must survive RPC conversion. Request-supplied arbitrary paths or expressions are not accepted.

## Native values and extensions

Arrays preserve empty and NULL-element distinctions. `PgInt8Range` distinguishes empty and unbounded ranges. Native array/range/network/JSON values are not supported as cursor keys. ltree and geometry initially use filter-only representations; application constraints define valid paths and SRIDs.

Similarity sorting uses server-declared expressions and corresponding filter inputs. Grouped, DISTINCT and cursor combinations have explicit restrictions. The library never sets arbitrary SQL from a request.

## Schema, plans and deployment

Use `validate_schema::<E>()` for startup/CI result-column validation. `diagnose` obtains `EXPLAIN (FORMAT JSON)` for authorized queries and conservative index suggestions; it does not execute EXPLAIN ANALYZE or create indexes. Facets and buckets use declared fields and query budgets.

Tables, constraints, indexes, database extensions, roles and RLS policies belong to application migrations. Ordinary queries do not automatically probe capabilities or start transactions. Native feature flags cannot substitute for deployment checks.
