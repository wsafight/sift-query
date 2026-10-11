# Transport And Cache

This English guide summarizes the [complete Chinese contract](../../09-TRANSPORT-AND-PERFORMANCE.md). Switch the page language to Chinese to read the full specification.

## GET and POST share one protocol

`SearchParams` accepts GET query strings or bounded `application/x-www-form-urlencoded` POST bodies. POST does not permit simultaneous query-string input. Body size and read deadlines are enforced without raising core query budgets.

The TypeScript client can choose GET for short encoded queries and POST for longer ones. This changes transport, not field/operator permissions or query semantics. RPC carries query intent and never a server authorization scope.

## SQL template cache

The cache stores SQL templates and binding instructions, not values or query results. Every hit revalidates current input and rebuilds binds from the current plan. Partition resolution and SQL interceptors still run per request.

PostgreSQL cache capacity defaults to zero, with an explicit bound of 16,384 when enabled. Cache statistics and clearing support observation. Portable backends also use bounded templates. Compare warm hits, cold requests and high-cardinality churn before selecting capacity.

## Performance interpretation

Parser, validation, codec-binding, rendering and CSV-buffer optimizations reduce repeated work. Stable hits can improve CPU costs; cold caches and repeated eviction can add overhead. Network latency, database planning/execution, count queries, pool waiting and row decoding often dominate total latency.

Use no-count cursor pages for sequential paging and list-only APIs when counts are unnecessary. Schema/index design remains the application's responsibility. See [performance baselines](performance.md) for reproducible measurements and limits.
