# Release

This English guide summarizes the [complete Chinese contract](../../11-RELEASE-AND-BACKENDS.md). Switch the page language to Chinese to read the full specification.

## Published release

All eight workspace library crates are published at 0.2.0 on [crates.io](https://crates.io/crates/sift-query), tagged [v0.2.0](https://github.com/wsafight/sift-query/releases/tag/v0.2.0). Rust 1.95.0+ (MSRV) and SQLx 0.9 are required.

The API is pre-1.0. Minor 0.x versions may include documented API changes; patch releases preserve compatibility. Protocol/cursor changes need migration notes, and MSRV changes belong in minor-release notes.

## Dependency isolation

Core owns parameters, values, queries, scopes and budgets without SQLx. Derive generates metadata. `sift-query-sqlx-common` supplies driver-free shared execution contracts; concrete PostgreSQL/MySQL/SQLite crates bring only their selected drivers.

Use `default-features = false` and explicit backend features. Other application dependencies can still enable drivers through Cargo feature unification. `scripts/check-backend-isolation.sh` verifies the library's dependency graphs.

## Validation evidence

Release CI passed PostgreSQL, MySQL, MariaDB, SQLite and TypeScript checks. Registry archives were checked for source commit, README and license; a separate project without path dependencies verified installation and SQLite queries. See [the release CI](https://github.com/wsafight/sift-query/actions/runs/38136340777).

MySQL/MariaDB cancellation and output-failure coverage remains incomplete. Target-hardware throughput, peak memory and failure behavior require deployment validation; the capability matrix does not replace service integration tests.

## Local release gates

Formatting, Clippy, tests, backend isolation, documentation and `bash scripts/check-packages.sh` precede publishing. Publishing follows dependency order: algorithm, core, derive, common, concrete backends, facade. Local check scripts do not publish packages automatically.
