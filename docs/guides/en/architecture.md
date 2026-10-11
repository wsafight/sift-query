# Architecture

This English guide summarizes the [complete Chinese contract](../../05-IMPLEMENTATION.md). Switch the page language to Chinese to read the full specification.

## Crate responsibilities

| Crate | Responsibility |
| --- | --- |
| `sift-query-algorithm` | Domain-independent bounded caches, hashing and budget helpers |
| `sift-query-core` | Metadata, values, parameters, policies, query models and scopes |
| `sift-query-derive` | Compile-time entity and projection declarations |
| `sift-query-sqlx-common` | Driver-free contracts, portable execution helpers, scan/CSV and adapters |
| `sift-query-sqlx` | PostgreSQL SQL rendering and execution |
| `sift-query-mysql` | MySQL/MariaDB execution |
| `sift-query-sqlite` | SQLite execution |
| `sift-query` | Public facade and feature selection |

Core remains independent of database drivers and web frameworks. Concrete backends retain their own dialects and transaction types. Shared algorithms and portable execution infrastructure remove repeated implementation without erasing backend capabilities.

## Validation layers

Unit/property tests cover parsing, normalization, budgets, query planning and value semantics. Compile tests check derive declarations and invalid mappings. Service regressions verify actual PostgreSQL, MySQL, MariaDB and SQLite behavior; driver-isolation checks verify installation graphs.

Release package checks compile extracted archives. Independent registry installations validate published dependencies. Scheduled fuzzing supplements bounded regressions; a finite fuzz run does not establish exhaustive coverage.

## Extension boundaries

Codecs, operators, routing, interceptors and result enrichers are server-configured hooks. Scope enforcement remains in the query plan. SQL caches retain structure and rebuild current values. External connections retain caller ownership.

The [implementation status](status.md) records evidence and remaining deployment checks; [query semantics](semantics.md) defines behavior when implementation notes and contracts differ.
