# sift-query-sqlite

SQLite backend for SiftQuery using SQLx. It is an independent backend crate,
so PostgreSQL applications do not compile the SQLite driver.

The backend exposes typed and dynamic map pages, `SearchOptions` summaries,
caller-owned connection/transaction entry points, bounded `scan` streams and
CSV export, including grouped summary queries against an existing transaction.
Registered core codecs are applied to dynamic map/scan results.
The scan and export budgets are server owned and reject client paging controls.
