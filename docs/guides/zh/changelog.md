# 变更记录

## 0.2.0 — 2026-11-26

### 破坏性变更

- 在 `sift-query-sqlx-common` 中统一跨后端共享类型（`Page`、`CursorPage`、`RowMap`、`SummaryValue`、`ScanPolicy`、`TimeoutStage`、`QueryObservation`、`QueryObserver`、`ExtensionError`、`LabelEnricher`、`PageEnricher`、`TypedPageEnricher`、`CsvColumn`、`CsvExportPolicy`、`ExportLimiter` 及 Web 提取器），并由 `sift-query` 与 `sift-query-sqlx` 直接重导出；移除 `MySqlPage`、`SqliteScanPolicy`、`mysql_web` 等冗余的后端前缀别名。
- 简化 `sift-query` 的 feature 矩阵：移除冗余的 `mysql-*` / `sqlite-*` 组合 feature；`axum`、`actix-web`、`poem`、`chrono`、`decimal`、`json`、`serde`、`uuid` 现在会自动透传到已启用的任意后端。
- 将 `ScanPolicy::validate` 的返回错误类型统一为 `Result<Self, ParamError>`，并为 `TimeoutStage` 标记 `#[non_exhaustive]`。

### 修复与改进

- **Core**：为嵌套布尔分组强制执行展开后的查询复杂度预算，修复 `ServerScope` 与 `OR` 组合语义，并收紧 cursor 与分桶校验。
- **SQL 渲染**：统一修复 PostgreSQL、MySQL/MariaDB 与 SQLite 上的 cursor `NULL` 绑定泄漏、`ORDER BY` 空值排序对齐以及 `LIKE` 转义漂移。
- **PostgreSQL**：当 statement timeout 复位失败时保留原始查询错误，为分桶与 Facet 强制执行字段访问策略，并让 RLS 扫描遵守行数与时间预算。
- **代码去重**：通过 `sift-query-sqlx-common` 在三个 sqlx 后端间共享 CSV 导出与 Web 提取器实现。
- **文档**：新增中英文双语 Astro 文档站点。

## 0.1.0 — 2026-10-11

- PostgreSQL 类型化与动态只读查询、投影、分组与聚合摘要。
- 严格参数预算、服务端 Scope、签名 keyset cursor 与可选快照执行。
- 参数过滤器、已注册 HTTP 运算符、可复用映射与无损 RPC 编码。
- 时间/列表 Codec、命名连接池与基于可信上下文的连接池路由。
- 有界扫描/CSV 导出、Web 适配器、Page/Label 增强与查询观测。
- 有界 SQL 模板缓存，始终按当前请求值重新绑定参数。
- 可选的 PostgreSQL 原生全文检索/JSONB、`bigint`/`text` 数组、`int8range` 与 `inet`/`cidr` 查询。
- 可选的 `pg_trgm`、`ltree` 与 PostGIS 过滤器、显式能力检查与绑定相似度排序。
- 安全元数据/OpenAPI 片段、有预算的 RSQL 子集与可信事务内 RLS（含扫描与 CSV 支持）。
- 显式 `cursor_non_null` 断言支持索引范围扫描，同时不削弱可空键排序语义。
- 独立的 MySQL/MariaDB 与 SQLite 后端，支持类型化/Map 查询、摘要、游标分页、有界扫描、CSV 导出与 Web 适配器。
- 字段级访问策略、服务端声明的 `EXISTS`、DISTINCT/Facet 语义、基于元数据的 TypeScript 查询构建器、Schema 结果类型校验、查询诊断与时间/数值分桶。

全部 8 个工作区库 crate 均已发布到 crates.io。当前公开 API 为 pre-1.0。
