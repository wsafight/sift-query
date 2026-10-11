# Status

This English guide summarizes the [complete Chinese contract](../../06-STATUS.md). Switch the page language to Chinese to read the full specification.

## Released version

Version 0.1.0 is available for all eight library crates. The release and source tag are [v0.1.0](https://github.com/wsafight/sift-query/releases/tag/v0.1.0). Rust 1.99.0 and SQLx 0.9 are the current toolchain contract; API stability is pre-1.0.

## Implemented capabilities

The core supports strict parameter parsing, declared operators, grouped conditions, row/group scopes, field access, ordering, projection and budgets. Execution includes typed/map pages, summaries, signed no-count cursor pages, bounded scans/CSV, codecs, labels, observers and caller-owned connections.

PostgreSQL additionally supports schema/result-type validation, facets and buckets, EXPLAIN diagnostics, explicit execution strategies, optional native values/search/extensions and trusted RLS transactions. Metadata, bounded RSQL and a TypeScript query builder connect the query contract to application clients.

Shared caches, precomputed validation/codec bindings and allocation reductions are implemented. SQL caches still bind current values and never retain result data.

## Release evidence

Release CI passed PostgreSQL 17, MySQL 8.4, MariaDB 11.4, SQLite and TypeScript checks. Registry archives and independent installations were verified. Read [release evidence](release.md) and [the changelog](../../../CHANGELOG.md).

## Remaining validation

Complete MySQL/MariaDB cancellation and output-failure matrices, target-hardware load/peak-memory validation and production benchmark archives remain deployment work. The current published release does not promise general availability stability or a fully tested fault matrix.

Chinese specifications include detailed historical implementation notes; release evidence supersedes earlier statements that remote CI or publication had not yet happened.
