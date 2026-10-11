# Ecosystem

This English guide summarizes the [complete Chinese contract](../../13-ECOSYSTEM.md). Switch the page language to Chinese to read the full specification.

## Safe metadata and OpenAPI

Enable `metadata` and call `metadata::describe::<E>(&policy)` for public field names, value types, operators, limits and fingerprints. The description excludes table names, column expressions, aliases and fixed SQL predicates.

`metadata::openapi::<E>(&policy)` creates OpenAPI 3.1 parameter/request-body fragments for an application to include in its own paths. It does not discover database schemas or serve a complete OpenAPI service. Metadata and RSQL are available without a database driver.

## Bounded RSQL

Enable `rsql` to parse a bounded subset into the same `UserQuery<E>`. Supported comparisons include `==`, `!=`, `=gt=`, `=ge=`, `=lt=`, `=le=`, `=in=` and `=out=`. Semicolons mean AND, commas mean OR, and parentheses change grouping.

Fields, operators, codecs, expression depth and phase rules remain governed by the existing query policy. Unsupported operations, wildcards, empty lists and invalid fields are errors. This is not a full OData service.

## Trusted PostgreSQL RLS context

Enable `rls`, declare allowed setting keys and roles with `RlsConfig`, and construct contexts from authenticated identity. The managed RLS entry points use a read-only transaction with bound transaction-local settings. `finish()` rolls back local state before returning the connection; error, cancellation or unfinished drop closes it.

RLS adds intentional transaction cost and does not replace `ServerScope`. Application migrations own tables, RLS policies, roles and grants. Use a role constrained by RLS, rather than a superuser or BYPASSRLS role.

## Streams and caller-owned transactions

RLS scan/CSV entry points retain row, deadline and concurrency budgets. `context.apply_to(&mut transaction)` supports caller-owned transactions, whose success/failure/cancellation cleanup remains the application's responsibility. Use separate transactions for separate authorization contexts.
