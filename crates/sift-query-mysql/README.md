# sift-query-mysql

MySQL and MariaDB backend for SiftQuery using SQLx. It is an independent
backend crate, so PostgreSQL applications do not compile the MySQL driver.

The backend exposes typed and dynamic map pages, `SearchOptions` summaries,
caller-owned connection/transaction entry points, bounded `scan` streams and
CSV export. Registered core codecs are applied to dynamic map/scan results.
MySQL `DECIMAL` results from integer `SUM`/`AVG` are decoded into
the portable summary value types. The scan and export budgets are server owned
and reject client paging controls.
