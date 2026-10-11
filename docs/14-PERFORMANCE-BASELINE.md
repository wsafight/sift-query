# 性能基线采集

仓库中的性能数字用于复现和比较实现，不构成数据库吞吐、延迟或 SLO 承诺。目标部署的 CPU、内存、操作系统、PostgreSQL 版本、连接池和数据分布会改变结果，因此发布前仍须在实际硬件上归档报告。

## 采集命令

只运行 CPU 路径的 Criterion 基准：

```bash
bash scripts/collect-performance-baseline.sh
```

可以把输出根目录作为第一个参数，或设置 `SIFT_QUERY_BASELINE_DIR`：

```bash
bash scripts/collect-performance-baseline.sh /tmp/sift-query-baselines
SIFT_QUERY_BASELINE_DIR=/tmp/sift-query-baselines bash scripts/collect-performance-baseline.sh
```

脚本会创建带 UTC 时间戳的目录，保存 `environment.txt`、`core_pipeline.txt` 和 `postgres_workload.txt`。环境记录包含 Rust/Cargo 版本、操作系统、内核、架构、CPU、内存和 Git revision；不记录 `DATABASE_URL` 的值。默认输出目录已加入 `.gitignore`，需要归档时由发布流程显式保存。

设置一次性、可丢弃的 PostgreSQL `DATABASE_URL` 后，脚本还会运行数据库工作负载：

```bash
DATABASE_URL=postgres://sift:sift@127.0.0.1:5432/sift_query_test \
  bash scripts/collect-performance-baseline.sh
```

`postgres_workload` 会创建并删除专用的 `sift_benchmark` schema，拒绝覆盖已存在的 schema。它覆盖执行策略、快照、window count、offset/cursor 深分页、stream 和实际 SQL 的 `EXPLAIN (ANALYZE, BUFFERS)`；异常终止后应由数据库管理员清理该 schema。不要把业务数据库或包含重要数据的账号用于此基准。

## 解读边界

`core_pipeline` 测量参数解析、计划和 PostgreSQL SQL 模板渲染；它不包含网络、数据库执行、连接池等待或 Row 解码。模板缓存只缓存 SQL 模板，命中时仍从当前请求重建绑定值，不缓存查询结果。缓存容量、冷启动、稳定命中和高基数污染应一起比较。

`postgres_workload` 的输出是单个本地配置的可复现样本，不能替代目标硬件上的峰值内存、并发吞吐、取消成本、故障注入和支持版本矩阵。报告应同时记录样本数、预热、数据量、索引、并发、查询分布和数据库版本；跨机器比较前先确认这些输入一致。
