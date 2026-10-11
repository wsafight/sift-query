# Overview

SiftQuery is a typed, parameter-driven, read-only query library for Rust and SQLx. Version 0.1.0 is available on crates.io; its API is pre-1.0.

## The query pipeline

Declare a `SearchEntity`, accept a raw query string or construct a typed query, and supply a trusted `ServerScope`. SiftQuery validates field names, operator capabilities, values, ordering and budgets before rendering parameterized SQL.

The query model supports nested conditions, typed rows, dynamic maps, summaries, signed cursors, bounded scans and CSV exports. SQL template caches retain query structure and rebuild binds from current values; they do not cache result data.

## Database support

PostgreSQL, MySQL/MariaDB and SQLite have separate backend crates. PostgreSQL offers additional opt-in native types, full-text search, extensions and RLS. Portable features and backend-specific capabilities are described in [Choose a backend](backend-selection.md).

## Application responsibilities

Your application owns the schema, migrations, authentication and authorization. It must construct scopes from trusted identity. HTTP inputs cannot provide SQL, table names, column names or authorization scopes. Ordinary queries do not require transactions; external transaction entry points keep transaction ownership with the caller.

SiftQuery does not create tables, choose indexes or generate an application UI.

## Start here

1. [Install the crate and choose features](installation.md).
2. [Run a complete SQLite example](quick-start.md).
3. [Try filtering and paging examples](query-cookbook.md).
4. Read the [Rust API guide](api.md) and [parameter protocol](protocol.md).

The English reference pages are concise guides to the existing contracts. Switch to Chinese for the full specifications and implementation records.
