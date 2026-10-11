# Product

This English guide summarizes the [complete Chinese contract](../../01-PRODUCT.md). Switch the page language to Chinese to read the full specification.

## Purpose and boundaries

SiftQuery turns declared entity metadata and validated query intent into parameterized, read-only SQL. It targets list/search APIs, reporting read models, controlled projections and bounded data export.

The application owns tables, migrations, authentication, authorization and connection pools. SiftQuery owns query parsing, semantic validation, SQL planning/rendering and bounded execution. Requests cannot provide raw SQL, identifiers or authorization scopes.

## Principles

- Treat unknown fields, disallowed operators, duplicate control parameters and invalid values as errors.
- Keep `UserQuery` separate from trusted `ServerScope` and `GroupScope`.
- Bind runtime values and derive identifiers from trusted metadata.
- Preserve query semantics before optimizing SQL shape or allocations.
- Make optional capabilities explicit; ordinary queries do not require a transaction.

## Compatibility

Bean Searcher is a product reference, not a promise to reproduce Java reflection or permissive fallback. Compatibility is limited by the explicit [conformance matrix](conformance.md). PostgreSQL is the most complete backend; [portable backend coverage](backend-selection.md) is documented separately.
