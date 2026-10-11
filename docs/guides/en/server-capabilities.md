# Server Capabilities

This English guide summarizes the [complete Chinese contract](../../15-PRODUCT-ENHANCEMENTS.md). Switch the page language to Chinese to read the full specification.

## Backend capabilities

`backend_capabilities(BackendKind)` describes server-declared features for deployment checks and metadata. It does not probe the database on each request or install extensions. Compare [backend support](backend-selection.md) before enabling native operators.

## CSV export policies

Use bounded scans, batch output, total deadlines and concurrency gates. `CsvExportDescriptor` creates validated download names and headers. Column selection and formula-injection protection remain server-controlled.

## Declarative labels

A field can declare `label = "order_status"`. `LabelEnricher::from_entity::<Order>()` reads this metadata; the server still registers dictionary or remote loaders. IDs, lookup deadlines, cache, concurrency and missing-label policies are bounded. HTTP cannot select an arbitrary dictionary.

## Trusted-context routing

`ContextPoolRouter<C>` resolves trusted application context to registered pool names. Concrete PostgreSQL, MySQL and SQLite routers preserve backend types and isolation.

## Converters

`FieldConvertor` is a compatibility name for `Codec`. String, scalar-text and optional JSON conversion still declare applicable fields and value types. Codec fingerprints participate in policy identity.

## Controlled server SQL

`CustomOperator`, `ServerSqlExtension` and `ServerSqlRegistry` let application code register bounded SQL templates at startup. Requests choose an allowed operator name and supply typed values that remain bind parameters.

Templates are checked for length, placeholders, comments, semicolons, NUL bytes and field restrictions. Register extensions with `QueryPolicy::with_server_sql_registry::<Entity>()` to validate the entity allowlist. This supports application-specific predicates and controlled EXISTS/NOT EXISTS without accepting request-supplied SQL. An interceptor cannot replace authorization scopes.
