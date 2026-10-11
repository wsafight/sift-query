# SiftQuery 实现状态

更新时间：2026-10-11

工具链：Rust 1.99.0

版本阶段：PostgreSQL M2/P0 功能与 P1/P2 首版已实现，发布验证进行中

## P0 四组能力交付

已补齐参数过滤、命名 Codec 与结果转换、阶段安全的 GroupScope；单查询有界 scan 与分批 CSV 流式写出；三框架可复用 Web adapter；整页异步增强与显式类型化 Label；查询/流生命周期 span、慢查询阈值和 QueryObserver；调用方拥有连接及事务的高级执行入口。

普通 query 不要求事务。外部入口不改变调用方的提交、回滚或 session 配置，并拒绝 Snapshot/Parallel 组合。具体合同、预算和能力边界见 [08-P0-EXTENSIONS.md](./08-P0-EXTENSIONS.md)。

本轮新增协议、Label/缓存/失败、CSV 安全与 writer deadline、框架一致性、subscriber 生命周期及 PostgreSQL 执行合同测试。全量测试包含原有真实 PostgreSQL 回归，使用本机 PostgreSQL 17.11 的独立临时测试数据库，与业务数据库隔离。仓库已提供 PostgreSQL/MySQL/MariaDB/SQLite 的 CI 配置和可复现性能基线采集脚本；目标硬件性能、完整故障矩阵、远端 CI 执行和正式发布门禁仍需外部环境验证。

## 当前范围

前 6 项产品增强已落地：三驱动能力矩阵与导出门禁、生产级 CSV 响应描述、derive 声明式 Label、后端无关上下文 PoolRouter、扩展 Codec/FieldConvertor，以及受控 `ServerSqlExtension` 注册合同。详细使用方式见 [15-PRODUCT-ENHANCEMENTS.md](./15-PRODUCT-ENHANCEMENTS.md)。

已对照 Bean Searcher 4.8.12 补齐产品排序 1、2、6、7、8：完整 QueryBuilder/UserQuery RPC、HTTP 运算符注册与字段白名单、编译期映射复用/命名策略、命名 pool/可信上下文路由、时间/时区/列表 Codec。API 与有意差异见 [10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md](./10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md)。真实 PostgreSQL 用例验证双 pool 数据/配置/缓存隔离、GET/POST RPC 等价、typed/RowMap、count/scan/CSV；P0 产品能力已完成，剩余项是发布、目标环境和 MariaDB 故障门禁。

P1 已实现 typed field/input 能力模型、显式初始化检测、四类全文过滤、JSONB、bigint[]/text[]、int8range、inet/cidr，以及可选 pg_trgm/ltree/PostGIS 与受控距离排序。P2 首版提供安全 metadata/OpenAPI、RSQL 子集、可信事务内 RLS 和 scan/CSV/external transaction 组合。全部是默认关闭的独立 feature；普通查询不自动开事务或访问能力目录。范围、编码和索引限制见 [12-POSTGRES-CAPABILITIES.md](./12-POSTGRES-CAPABILITIES.md) 与 [13-ECOSYSTEM.md](./13-ECOSYSTEM.md)。

发布元数据、包内 README/LICENSE、根 CHANGELOG/SECURITY、支持版本/SemVer 文档与本地打包已具备。依赖图检查确认 core/facade-core 不含 sqlx，选择单一后端时只包含对应 SQLx 驱动；其他数据库不进入 PG 运行时分支。

GET/POST 同协议入口、TS 自动选择传输 helper、渲染前模板缓存以及重复分配优化已实现。新增 `web_transport` / `template_cache` 回归，并启用缓存执行真实 PostgreSQL P0 合同；当前本地全量测试、Clippy、独立 feature 检查、文档构建和 TS 类型/运行测试通过。接口及性能边界见 [09-TRANSPORT-AND-PERFORMANCE.md](./09-TRANSPORT-AND-PERFORMANCE.md)，基线采集命令见 [14-PERFORMANCE-BASELINE.md](./14-PERFORMANCE-BASELINE.md)。远端 CI 尚未执行。

M1 全部能力继续保留。本轮完成 `01-PRODUCT.md` 定义的 M2 PostgreSQL 生产增强，并完成 P0.16 的 MySQL/MariaDB、SQLite 独立核心查询后端；MariaDB 真实服务的取消、输出失败、连接复用故障矩阵及远端执行仍按独立后端门槛推进。

## M2 已实现

- 独立、无领域依赖的 `sift-query-algorithm` crate，承载 `BoundedLruMap<K, V>` 和 `StableHasher64`；core、derive、sqlx 已实际复用。
- 21 个公开内置运算符：`eq/ne/gt/ge/lt/le/bt/nb/il/ni/nl/nn/ey/ny/sw/ew/ct/ol/nk/at/af`。
- 服务端受控 `CustomOperator`、命名 HTTP 注册表及字段白名单；HTTP 不能注入 SQL 模板。
- `#[derive(SearchProjection)]` 与 `search_as<E, P>` 静态投影。
- HMAC-SHA256、base64url-no-padding 的签名 cursor，以及 PostgreSQL keyset SQL。
- `ParallelBestEffort` 和只读事务 `Snapshot(RepeatableRead | Serializable)`。
- `CountStrategy::WindowWhenPossible`，以及 offset 空页的独立 count 回退。
- 容量上限 16384、O(1) LRU 淘汰、完整结构相等比较的渲染前模板缓存；保留原有 SQL 碰撞校验入口，默认容量 0。
- `CompatibilityPolicy::BeanSearcher4812` 命名兼容策略。
- 结构化 `PartitionName`/`PartitionResolver`、不暴露 binds 的 `SqlInterceptor`、RowMap `ResultFilter`。
- 基于 sqlx row stream 的 `search_stream`；完整消费恢复会话配置，提前 drop 时关闭连接。
- `SUM/AVG/MIN/MAX` 与服务端受控 `CustomAggregate` summaries。
- axum、actix-web、poem 三个 PostgreSQL 示例，均为 workspace member。

## 本轮八项高价值查询能力

以下能力已落到公开 API，并通过 core/sqlx 回归覆盖：

1. **字段级权限**：`FieldAccessPolicy` 可挂到 `ServerScope`；多个 Scope 合并取权限交集。Map 全投影会自动剔除不可见字段，显式投影、过滤、排序、summary、sum、facet 和分桶访问不可见字段会在渲染前拒绝。
2. **无 COUNT cursor 分页**：PostgreSQL、MySQL/MariaDB、SQLite 提供 `search_cursor` / `search_map_cursor`，PostgreSQL 另有 `search_as_cursor`。每次只取 `size + 1` 行生成 `CursorPage`，不执行 count 或 cluster 查询。
3. **`EXISTS / NOT EXISTS`**：`CustomOperator::exists`、`CustomOperator::not_exists` 和 `ServerScopeBuilder` helper 只接受服务端静态模板；模板必须以对应的 `EXISTS (SELECT ...)` 开始，值仍然绑定为参数。
4. **Facet 语义**：Facet 限制为服务端声明的字段，支持 NULL 选择、top-N 和 `other_count`；DISTINCT read model 先去重再分桶，并复用查询过滤和 Scope。
5. **TypeScript SDK**：`clients/sift-query.ts` 的 metadata 驱动 `QueryBuilder` 覆盖过滤、投影、排序、分页和 cursor，并输出既有 `RawParams` 协议，自动选择 GET/POST。
6. **Schema 结果类型校验**：`validate_schema::<E>()` 同时检查可计划性、结果列数量、alias、数据库类型和声明的 `ValueType`，失败返回逐列诊断。
7. **查询诊断与索引建议**：`PgSiftQuery::diagnose` 对授权后的 list/cluster SQL 执行 `EXPLAIN (FORMAT JSON)`，返回 shape、参数化 SQL、计划 JSON 和保守建议。它适合管理员、CI 和慢查询排查，不执行 `EXPLAIN ANALYZE`，也不自动建索引。
8. **时间序列与数值分桶**：PostgreSQL `buckets` 支持 hour/day/week/month 和严格递增的数值边界，NULL 进入显式 bucket，查询仍受同一过滤、Scope、字段权限、超时和预算约束。

这些能力以 PostgreSQL 为完整实现目标；cursor 的基础执行路径已在 MySQL/MariaDB 与 SQLite 对齐。默认 Facet convenience API 分别执行列表与聚合；需要强一致快照时使用 `search_map_with_facets_on`，在调用方开启外部只读事务。

## 安全与一致性约束

- `page`、`offset`、`cursor` 互斥；cursor token 最大 1024 字节，密钥至少 32 字节。
- cursor 绑定 entity schema 指纹与完整排序；分组实体的 cursor 在当前 M2 被拒绝。
- cursor keyset 条件只进入 list，不缩小 count/summaries。
- Snapshot 在同一连接、显式只读事务中执行；失败连接不会带着未完成事务返回池。
- SQL 扩展全部是服务端注册能力；分区 schema/table/alias 必须通过标识符校验。
- ResultFilter 不能替代 ServerScope；丢弃 RowMap 行不会改写数据库 total_count。
- 扩展、流、导出、观测和外部执行分别有独立模块，测试位于各 crate 的 `tests/` 及模块测试目录。

## 当前验证结果

已在 `rustc 1.99.0 (b940084d7 2026-09-28)` 下执行：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo check --workspace --no-default-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features -- --include-ignored
cargo test -p sift-query-core --no-default-features
cargo check -p sift-query --no-default-features
cargo check -p sift-query-sqlx --no-default-features
cargo doc --workspace --all-features --no-deps
node --test clients/sift-query.test.ts
npx --yes --package typescript tsc --noEmit --target es2022 --lib dom,es2022 --module nodenext clients/sift-query.ts
```

早期 M2 数据库验证使用本机 Homebrew PostgreSQL 18.6 的 `/tmp` 隔离实例。当前 P0 回归使用 PostgreSQL 17.11 的独立测试库，同样不接触业务库。覆盖完整类型、RowMap、静态投影、21 运算符代表用例、签名 cursor、窗口 count 空页回退、并行与两档 Snapshot、扩展钩子、summaries、流消费/取消、类型矩阵及超时；测试结束后清理本轮创建的测试数据。

测试数量随回归覆盖扩展；trybuild 覆盖映射错误与计算排序缺属性/未知 source/不安全模板。新回归覆盖原生 NULL/empty/边界、SQL 重载参数 cast、排序当前绑定/预算与 list/window cache、RLS 两租户/多 pool/stream/CSV/取消清理，以及 Snapshot 控制失败后连接池复用。原有 PostgreSQL 集成测试与 P0 合同测试均保留。

core 与 sqlx property 每项默认 256 样本；五个 fuzz target 独立于发布依赖，定时/手动 CI 每目标 120 秒，本地每项约 15 秒通过。native_values 发现的 JSON 大整数 RPC 精度问题已修复并保留 regression seed。自动 hash corpus 不提交 Git，CI 保存 artifact；有限 fuzz 不代表穷尽验证。

P1/P2 增量同环境新旧 Criterion 对比，20 条件完整管线从 28.138 µs 到 28.997 µs，缓存命中管线从 23.120 µs 到 24.092 µs；主要 CPU 场景增量约 3%–9%，短测含调度噪声。不能用更早不同运行时刻的绝对数据判断本次回归，也不能宣称零成本。postgres_workload 验证策略/count/深分页/流及实际 SQL 的 EXPLAIN；新增 cursor_non_null 后，本机 40,000 深分页整页中位数从未声明的 4.127 ms 降为 1.724 ms，NULL 语义默认保留。场景、配置与限制见 [09-TRANSPORT-AND-PERFORMANCE.md](./09-TRANSPORT-AND-PERFORMANCE.md)；部署负载/峰值内存仍需目标硬件验收。

## 源码结构与共享算法

core、derive、sqlx 已按职责目录重组，公开导入路径保持兼容。raw/policy、属性解析/代码生成、执行配置/分页/聚合/IO、Web 框架 adapter 与 PostgreSQL 能力目录分别归拢；目录和边界见 [05-IMPLEMENTATION.md](./05-IMPLEMENTATION.md)。algorithm 新增有界计数、公共 ASCII 名称语法、LRU 删除/空槽复用和调用方时间戳驱动的 TTL 缓存，已接入实际预算校验与 Label 缓存。

算法回归覆盖 30,000 步 LRU 参考模型对照、内部存储复用、TTL 边界/替换/清理、计数溢出和零预算。内置参数 filter 保留原声明模块，固定指纹回归避免目录重组改变配置身份；Cursor framing 保持不变。真实 PostgreSQL 全量回归、Clippy、独立 feature、依赖隔离及文档检查通过；五个 fuzz 目标构建通过，五个库和三个示例的发布包均成功解包编译，临时测试库已清理。重组性能抽查未发现明显退化，数据及限制见 09。

## 发布前剩余外部门禁

- 远端 CI 的 PostgreSQL 17 service 全量流水线待执行并作为发布门禁。
- 远端 CI 的 MySQL 8.4、MariaDB 11.4 service 回归待执行并作为发布门禁。
- 真实发布仓库/安全联系方式、crates.io 名称与所有权、版本发布及注册表安装（本地 CHANGELOG/打包已完成）。
- 目标部署硬件上的 M2 cache/parallel/snapshot/cursor/stream/window benchmark 报告，以及峰值内存和完整故障矩阵。

这些是发布与环境门禁，不是当前 M2 PostgreSQL 功能缺口。
