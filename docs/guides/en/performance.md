# Performance

This English guide summarizes the [complete Chinese contract](../../14-PERFORMANCE-BASELINE.md). Switch the page language to Chinese to read the full specification.

## Collect a reproducible baseline

The repository includes a script that records toolchain, host information and Git revision, then runs Criterion CPU benchmarks:

```bash
bash scripts/collect-performance-baseline.sh /tmp/sift-query-baselines
```

The report includes parsing, planning and SQL rendering. It does not measure network latency, pool waits or database execution. Cache capacity, warm hits, cold starts and eviction-heavy workloads should be measured together.

## Database workload

When `DATABASE_URL` points to a disposable PostgreSQL database, the same script also runs the PostgreSQL workload. It creates and removes a dedicated `sift_benchmark` schema and refuses to overwrite an existing schema.

The workload compares separate/parallel/snapshot execution, window count, offset and cursor paging, streams and EXPLAIN ANALYZE with buffers. Unexpected termination may leave the benchmark schema for an administrator to clean up.

## Interpret results

Existing timings are local reproduction samples, not throughput or latency guarantees. Record samples, warmup, data size/distribution, indexes, concurrency, query mix and database version. Compare implementations under the same inputs; do not combine unrelated machines or runtime moments.

Target-hardware peak memory, concurrency, cancellation, fault injection and supported-version coverage still need deployment checks. SiftQuery never creates or changes indexes as part of normal querying.
