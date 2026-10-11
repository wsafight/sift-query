# SiftQuery M1/M2 实施、测试与验收

本文件定义从 M0 原型到当前 PostgreSQL M2 的实施顺序和完成标准。实现不得通过跳过协议校验或缩小测试语义来满足性能目标。

## 1. 实施原则

1. 先锁定协议和查询语义，再冻结公开 API。
2. 先用真实 PostgreSQL 证明类型、NULL、聚合和 DISTINCT 语义，再做 SQL 精简。
3. core、derive、renderer、executor 分层测试，失败定位不能只依赖端到端测试。
4. 参数错误必须在获取连接前完成。
5. 性能优化必须由基准证明，并能在不改变语义的情况下关闭。
6. M2 能力不得以未完成、默认开启的形式进入 M1。

## 2. 建议工作区

```text
sift-query/
  Cargo.toml
  crates/
    sift-query-algorithm/
    sift-query-core/
    sift-query-derive/
    sift-query-sqlx/
    sift-query-sqlx-common/
    sift-query-mysql/
    sift-query-sqlite/
    sift-query/
  examples/
    axum-postgres/
    actix-postgres/
    poem-postgres/
```

每个 crate 的集成测试独立放在自己的 `tests/` 目录。文档目录可以与代码 workspace 同根，也可以发布到独立站点，但 compile test 必须直接引用公开 API。

`sift-query-algorithm` 只接收无领域依赖、至少已有两个实际调用点或明确基础设施用途的算法。当前包括 `BoundedLruMap<K, V>`、`TtlLruMap<K, V>`、`BoundedCounter`、`StableHasher64` 和公共 ASCII 名称语法；查询 AST、协议和 SQL 语义留在所属 crate。

### 2.1 源码职责目录

各 crate 按职责组织源码，`lib.rs` 维持公开导出，内部目录不作为新的公共 API。

| crate | 源码目录与职责 |
|---|---|
| core | `model/` 元信息和值；`query/` 构建/谓词/计划/Row 与 Group Scope；`protocol/` raw/URL/parser/RPC/RSQL；`policy/` 配置/预算/校验/指纹；`cursor/` 类型与签名编解码；`extensions/` 参数过滤；`codecs/` 转换接口/注册表/内置实现；`metadata/` 公共描述 |
| derive | `attrs/` 实体/字段/sqlx/排序属性解析；`mapping/` 片段与预设；`codegen/` 实体/投影/指纹生成；`types.rs` 类型识别；`naming.rs` 命名；`validation.rs` 声明校验 |
| sqlx-common | `backend.rs` 后端绑定/解码合同；`renderer/` MySQL/SQLite SQL；`page.rs` 通用 Page/CursorPage；`enrichment/` Label 与 PageEnricher；`csv.rs` CSV 安全写出；`observe.rs` 低敏观测；`router.rs` 可信上下文路由；`web.rs` 公共请求规则 |
| sqlx (PostgreSQL) | `execution/` 配置/查询/聚合/IO/策略/scan/外部事务；`renderer/` PostgreSQL SQL；`cache/` SQL 模板；`enrichment/` PostgreSQL 扩展生命周期与专属错误；`export/` 导出入口；`web/` PostgreSQL 错误映射与框架 adapter；`postgres/` bind/decode/原生类型/运算符/能力检查/RLS |
| mysql / sqlite | 各自的 `backend.rs` 绑定/解码、`execution/` 查询/调用方连接/scan、`export.rs` 和 `router.rs`；共享 SQL renderer、Page、Label、CSV、观测和 Web 基础合同，不把连接生命周期擦成动态执行器 |
| algorithm | 小型纯算法文件，维持零依赖；时间戳由调用方传入，锁与异步加载留在 sqlx |
| facade | 薄导出入口，按 feature 暴露 PostgreSQL 能力 |

目录调整保持公开导入路径、已有指纹字节序列、Cursor framing、默认事务策略和驱动隔离。每个 framework/PG 扩展继续由原 feature 控制。LRU 删除回收空槽，避免反复删除/插入增长内部存储；TTL 采用惰性清理，不引入后台任务。core 的预算规则仍负责单位与错误分类，通用计数器只负责溢出和上限。

## 3. M0 可行性门禁

M0 不追求完整功能，只回答会改变架构的问题。

### 3.1 Gate A：动态绑定

验证：

- 使用枚举保存拥有所有权的不同标量类型，并逐项绑定到 `sqlx::query`。
- `Vec<i32>`、`Vec<i64>`、`Vec<String>`、UUID 和日期数组绑定。
- `column = ANY($1)` 的空数组、单元素和多元素行为。
- 类型化空数组是否需要显式 SQL cast。
- 绑定失败时的错误类别和 SQL 是否已经发送。

通过条件：`PgBindValue` 可以在不使用字符串插值、不使用 unsafe 类型擦除的前提下执行全部 M1 类型。

### 3.2 Gate B：聚合输出类型

在真实 PostgreSQL 中验证：

- integer SUM 解码为 i64。
- bigint SUM 解码为 numeric 对应 Rust 类型。
- numeric、real 和 double precision 的 SUM。
- 空输入返回 NULL。
- 分组实体的外层 SUM 类型是否再次提升。

通过条件：`PgSumType::Output` 表与数据库实际返回类型完全一致。

### 3.3 Gate C：FromRow 与别名

验证：

- 普通字段、raw identifier、`Option<T>` 和 `#[sqlx(rename)]`。
- `FilterOnly<T> + #[sqlx(skip)]`。
- `search_ignore + #[sqlx(skip/default)]`。
- PostgreSQL 63 字节别名边界。
- NULL 和类型错误与直接 sqlx `FromRow` 一致。

通过条件：derive 能在编译期拒绝不支持的映射，文档示例全部编译。

### 3.4 Gate D：查询语义

使用种子数据验证：

- WHERE、HAVING 和合法的两阶段 AND 拆分。
- 跨阶段 OR 在执行前被拒绝。
- DISTINCT RowMap 的投影和排序限制。
- GROUP BY count 的最小投影 SQL。
- LIKE/ILIKE 的 `%`、`_` 和 `\` 字面转义。
- 稳定排序键追加后跨页不重不漏。

### 3.5 Gate E：超时与取消

使用 `pg_sleep` 和小连接池验证：

- pool acquire 超时可以单独识别。
- list 和 cluster 查询超时可以单独识别。
- timeout 返回后，连接只有在协议状态安全时才能回到池中；否则必须丢弃。
- 客户端收到 Timeout 后，数据库查询不会无限制占用资源。
- 下一条查询不会继承前一条语句的 `statement_timeout` 或其他会话设置。

如果仅丢弃 future 不能满足这些条件，PostgreSQL 后端必须采用可验证的 server-side timeout 或安全取消策略。

### 3.6 Gate F：MSRV 与依赖

锁定：

- sqlx 0.9 和全部默认 feature 的实际 MSRV（sqlx 0.9 要求 Rust 1.94）。
- proc-macro、serde、chrono、decimal、uuid 等直接依赖的 MSRV。
- 最小 feature 组合与全 feature 组合。

M0 全部通过后才能冻结 v0.4 API 并进入 M1。

## 4. M1 实施顺序

### Phase 1：core 协议与 AST

- `RawParams` 编码解析和预算前置检查。
- 参数名语法、控制参数、运算符元数和类型转换。
- `UserQuery<E>`、`ServerScope<E>` 和 sealed `QueryInput<E>`。
- 连续节点存储的条件 AST。
- Row/Group 阶段传播和跨阶段 OR 拒绝。
- 投影、稳定排序和分页规范化。
- core 错误模型。

完成条件：不依赖数据库即可覆盖全部参数协议和 AST 测试。

### Phase 2：derive

- `SearchEntity`、`search_entity`、`search_field` 和 `search_ignore`。
- FieldId、字段常量、元信息和 schema fingerprint。
- 默认排序与稳定排序验证。
- GroupKey 驱动的 GROUP BY 元信息。
- result alias 和 FromRow 支持范围检查。
- trybuild compile-pass / compile-fail 测试。

完成条件：非法实体配置尽可能在编译期失败，错误消息指向对应属性。

### Phase 3：PostgreSQL renderer 与 binder

- `$1..$n` 占位符直接生成。
- 标量和类型化数组绑定。
- WHERE/HAVING、LIKE 转义和 ignore-case。
- 完整投影、RowMap 投影、DISTINCT 和 GROUP BY。
- list、count、sum 和多 summaries SQL。
- SQL shape id。

完成条件：SQL golden test 与 PostgreSQL 结果测试同时通过。

### Phase 4：执行器与解码

- `PgSiftQuery` 的 QueryInput 入口。
- 完整 `FromRow` 和 RowMap 解码。
- SequentialBestEffort list + cluster。
- pool acquire、执行、解码超时和取消。
- tracing、低基数指标和错误映射。

完成条件：全部 PostgreSQL 集成测试、取消测试和连接池复用测试通过。

### Phase 5：facade、示例与发布

- feature 和重导出。
- axum 示例。
- doctest、compile test 和迁移说明。
- crates.io 名称确认与占位。
- MSRV CI、最小 feature CI 和全 feature CI。

## 5. 测试层次

### 5.1 协议单元测试

至少覆盖：

- 百分号编码、`+`、非法 UTF-8 和空值。
- 重复键、编号键、混用和编号缺口。
- 每个运算符的正确元数与错误元数。
- 重复 `field-op/field-ic`。
- `sort+order`、`orderBy` 和冲突组合。
- onlySelect/selectExclude、空投影和不可选择字段。
- gexpr 优先级、深度、节点数、未定义组和未使用组。
- 显式 offset 与 page 冲突。
- page 计算溢出和计算后超过 max_offset。
- 所有预算边界的 `limit-1`、`limit`、`limit+1`。

### 5.2 属性测试

SHOULD 使用 property-based test 验证：

- parser 对任意输入不 panic。
- 解析后再规范化是幂等的。
- SQL 占位符编号连续并与 binds 数量一致。
- 不同 InList 长度保持相同 SQL 形状。
- 任意非法输入都不会到达 executor spy。
- 任意最终分页排序都以 stable_order 结尾且字段不重复。

### 5.3 compile test

compile-pass：

- 快速开始完整示例。
- 普通实体、聚合实体、FilterOnly、sqlx rename 和 nullable 字段。
- RawParams 与 UserQuery 两种 QueryInput。
- i32、i64、Decimal、f32、f64 的类型化 sum。

compile-fail：

- 重复公开字段名。
- 字段名与控制参数冲突。
- 不存在或不可排序的 default/stable order 字段。
- 分组实体包含可返回 Scalar。
- Aggregate 字段用于 ServerScope。
- 不支持的 SUM 类型。
- 超长或截断冲突的结果别名。
- 未满足约束的 FilterOnly/search_ignore。

### 5.4 SQL golden test

验证：

- SQL 文本。
- 绑定值顺序和绑定值类型。
- shape id 的稳定性。
- 同一语义输入的规范化顺序。

Golden test 只验证 renderer，不替代数据库结果测试。

### 5.5 PostgreSQL 集成测试

至少覆盖：

- 12 个运算符和全部支持类型。
- NULL、空字符串、通配符和 Unicode。
- InList 1、10、100 个元素。
- 普通、DISTINCT、GROUP BY、HAVING。
- 完整实体、RowMap、onlySelect 和 selectExclude。
- search、search_with、search_map、search_map_with、search_list、search_first、count、sum。
- SearchOptions 最多 8 个 SUM，顺序与结果稳定对应。
- size=0 不执行 list SQL。
- count/sum 不执行 list SQL。
- search_list/search_first 不执行 cluster SQL。
- 并发写入下 BestEffort 允许的不一致边界。
- SQL 错误、解码错误和 timeout 后连接池仍可用。

## 6. Java 兼容矩阵

M1 不再使用“所有 Java 测试标准化后全部通过”的笼统指标。每个移植用例必须先标注以下状态：

| 能力 | M1 状态 | 验收方式 |
|---|---|---|
| 基础字段条件 | 兼容子集 | 结果集一致 |
| 12 个 M1 运算符 | 兼容子集 | 谓词与结果一致 |
| 剩余 Bean Searcher 运算符 | M2 | M1 不计入通过率 |
| 逻辑分组 | 条件兼容 | 仅同阶段 OR 和可安全拆分 AND |
| 排序与分页 | 语义增强 | 相同稳定排序下结果一致 |
| 列表与总数 | 兼容 | 相同快照输入下语义一致 |
| WHERE/HAVING 路由 | 条件兼容 | 遵守 v0.4 阶段规则 |
| DISTINCT/GROUP BY count | 兼容 | 数据库结果一致 |
| JDBC `?` 占位符 | 有意改变 | 归一化后比较 |
| Java `c_N` 别名 | 有意改变 | 不比较内部别名 |
| 反射构造部分 Bean | 有意改变 | 使用 RowMap 或未来 search_as |
| 未知字段忽略 | 有意改变 | M1 返回 ParamError |
| 非法分页返回空集 | 有意改变 | M1 返回 ParamError |
| ThreadLocal 动态方言 | 不支持 | 使用后端具体执行器 |
| 继承字段 | 不支持 | 使用 Rust 显式组合 |

M1 的 Java 兼容指标为：矩阵中标记“兼容”或“兼容子集”的适用用例全部通过。条件兼容用例必须先满足 v0.4 前置条件。

移植 Java 测试或实现片段时必须保留 Apache-2.0 许可证要求和必要归属信息。

## 7. 性能基准

### 7.1 基线

手写、固定且使用 prepared statement 的 sqlx 查询是性能基线。SiftQuery 的目标是限制动态能力带来的额外开销，不宣称数据库查询天然快于手写 SQL。

### 7.2 Core 基准

覆盖：

- 0、5、20、32 个条件。
- InList 1、10、100 个值。
- 无分组、同阶段分组和合法两阶段 AND。
- 完整投影、5 字段 RowMap 和宽 RowMap。
- 冷启动与稳定运行。

报告：

- p50、p95、p99。
- 每秒吞吐。
- 分配次数和分配字节。
- SQL 长度、逻辑值数量和绑定槽位数。

M1 验收目标：在 release 构建、记录 CPU/内存/OS/Rust 版本/allocator、完成预热的条件下，20 个条件的 parse + plan + render p95 低于 1 毫秒。基准必须报告样本数和输入数据，不允许只记录单次最好结果。

### 7.3 解码与端到端基准

比较：

- 直接 sqlx `FromRow`。
- SiftQuery 完整类型查询。
- SiftQuery RowMap 查询。

端到端报告至少拆分：

- 解析与计划。
- 渲染。
- pool wait。
- 数据库执行与读取。
- Row 解码。

若 sqlx API 无法精确分离网络读取和解码，指标名称必须标明是组合耗时或估算值，不能展示为虚假的精确拆分。

### 7.4 M2 基准

以下能力已经实现，发布前应在目标部署硬件补充独立基准：

- Query Shape Cache 冷启动、命中和污染。
- ParallelBestEffort。
- Snapshot。
- cursor pagination。
- search_stream。
- count window strategy。

本地基线可用 `bash scripts/collect-performance-baseline.sh` 采集。脚本记录工具链、主机信息和 Git revision，运行 core Criterion 基准；设置一次性 `DATABASE_URL` 时还运行可复现的 PostgreSQL 工作负载。输出格式、数据库清理合同和目标硬件验收边界见 [14-PERFORMANCE-BASELINE.md](./14-PERFORMANCE-BASELINE.md)。

`shape_cache_hit` 只能来自实际的有界缓存命中，不能由 SQL 形状相同推测；基准必须同时报告容量、冷启动、稳定命中和高基数污染场景。

## 8. 超时、取消与执行原子性

M1 的执行 deadline 覆盖：

1. 获取数据库连接。
2. list SQL。
3. cluster SQL。
4. 行读取和解码。

参数解析、QueryPlan 构建和 SQL 渲染在获取连接前完成，并分别受查询预算和进程级 CPU 防护约束。

规则：

- list 成功而 cluster 失败时，`search` 整体返回错误，不返回部分 Page。
- summaries 与 count 位于同一 cluster SQL；该 SQL 失败时不返回部分 summaries。
- timeout 后只有协议状态安全的连接才能返回池。
- 所有会话级配置必须恢复，禁止把 statement timeout 泄漏给下一个请求。
- timeout 错误记录阶段，但公开 HTTP 响应不包含 SQL。

M1 可以选择 client-side cancellation、PostgreSQL server-side timeout 或组合方案，但必须通过 M0 Gate E 的可观测行为测试。

## 9. 可观测性

每次查询生成一个 tracing span，至少包含：

| 字段 | 说明 |
|---|---|
| `entity` | 低基数实体名 |
| `backend` | `postgres` |
| `operation` | search/search_map/count/sum 等 |
| `parse_duration_us` | 参数解析与验证事件 |
| `plan_duration_us` | QueryPlan 构建事件 |
| `render_duration_us` | SQL 和绑定槽位生成事件 |
| `pool_wait_duration_us` | 显式获取连接成功时的等待事件 |
| `list_duration_us` | list 执行、读取及会话清理；类型化 sqlx decode 包含在内 |
| `cluster_duration_us` | cluster 执行、读取及会话清理事件 |
| `decode_duration_us` | RowMap/summary 独立转换可可靠测量时记录，不伪造网络读取与 sqlx decode 的拆分 |
| `rows_returned` | 返回列表行数 |
| `outcome` | success/cancelled 或低基数错误分类 |
| `query_shape_id` | 事件或受控 span 字段，不作为默认时序指标标签 |

P0 的生命周期、慢查询阈值、QueryObserver、CSV 写出及外部连接的实际行为见 `08-P0-EXTENSIONS.md`。

禁止作为默认指标标签或 info 日志字段：

- 原始参数值。
- SQL 全文。
- 绑定值。
- tenant/user ID。
- query_shape_id。

debug SQL 日志必须显式开启，只输出占位符 SQL；绑定值默认完全隐藏。若未来支持值日志，必须按字段元信息脱敏并设置长度上限。

## 10. M1 验收标准

M1 发布必须同时满足：

1. 一个常规列表接口的业务代码不超过 5 行。
2. 增加普通可筛选字段只修改实体声明，不修改接口处理代码。
3. 所有公开示例通过 doctest、compile test 或示例 crate 编译。
4. 参数协议的正向、负向和预算边界测试全部通过。
5. 非法参数在获取连接前失败；executor spy 和集成测试均证明没有 SQL。
6. PostgreSQL 集成测试覆盖列表、总数、排序、分页、分组和基础聚合。
7. 兼容矩阵中 M1 适用的 Java 用例全部通过。
8. 不同长度 InList 使用一个数组槽位并保持相同 SQL 形状。
9. 跨 WHERE/HAVING OR 在执行前返回 `MixedPredicatePhase`。
10. DISTINCT RowMap 不会因隐藏排序列改变去重结果。
11. 分页最终排序包含应用声明的唯一稳定键。
12. `search_list/search_first` 不执行 cluster SQL；`count/sum` 不执行 list SQL。
13. size=0 的 search 不执行 list SQL。
14. GROUP BY 的 cluster SQL 只选择 count、HAVING 和请求聚合真正依赖的表达式。
15. SUM 类型提升与 PostgreSQL 实际返回类型一致。
16. timeout 后连接池可继续安全使用，且不会泄漏会话配置。
17. 20 条件 parse + plan + render 基准满足记录完整环境后的 p95 目标。
18. 最小 feature、全 feature、MSRV 和当前稳定 Rust CI 全部通过。

任何一项未满足时，版本仍是 pre-release，不应宣称 M1 完成。

## 11. M1 发布前检查表

- [ ] crates.io 名称已确认或占位。
- [ ] MSRV 已写入 Cargo.toml、README 和 CI。
- [ ] v0.4 所有 MUST 条款有实现或测试映射。
- [ ] 所有公开示例已编译。
- [ ] PostgreSQL 版本矩阵已记录。
- [ ] 兼容矩阵有逐项测试编号。
- [ ] 错误消息不泄露 SQL 和绑定值。
- [ ] 文档中不存在 M2 API 被描述为 M1 已实现。
- [ ] CHANGELOG 说明相对 Bean Searcher 的有意差异。
- [ ] benchmark 报告包含硬件、工具链、样本和原始数据。

## 12. M2 PostgreSQL 验收矩阵

原 M2 验收矩阵以 PostgreSQL 为目标；MySQL/MariaDB 和 SQLite 通过独立 P0.16 后端推进，不改变 PostgreSQL 执行路径。其余 M2 项必须满足：

| 能力 | 验收要求 | 主要验证位置 |
|---|---|---|
| 21 个内置运算符 | arity、字段白名单、绑定顺序和 PostgreSQL SQL 语义 | `core/tests/protocol.rs`、`sqlx/tests/renderer.rs`、`sqlx/tests/postgres.rs` |
| 自定义运算符/聚合 | 仅服务端构造；模板拒绝占位符注入、多语句和注释 | `core/tests/predicate.rs`、`sqlx/tests/m2_features.rs` |
| 静态投影 | derive 映射、别名、cursor 排序字段完整性 | `sift-query/tests/derive_and_parse.rs`、PostgreSQL 测试 |
| 签名 cursor | HMAC 防篡改、schema/order 绑定、keyset SQL、总数语义 | `core/tests/cursor.rs`、renderer 与 PostgreSQL 测试 |
| stream | 不执行 cluster；完整消费与提前 drop 均保持池可用 | PostgreSQL 测试 |
| 并行/快照 | 并发门控；两分支完整收敛；只读事务同快照；失败连接隔离 | PostgreSQL 测试 |
| 窗口 count | 仅在安全 shape 使用；空页回退独立 count | renderer 与 PostgreSQL 测试 |
| Shape Cache | 容量有界、O(1) LRU、完整结构相等比较、模板命中重建当前 binds、不缓存值 | `sqlx/tests/m2_features.rs`、`sqlx/tests/template_cache.rs` |
| 兼容策略 | `Strict` 默认；`BeanSearcher4812` 显式命名 | `core/tests/protocol.rs` |
| 扩展钩子 | 固定顺序、分区标识符校验、不向拦截器暴露 binds | `sqlx/tests/m2_features.rs`、PostgreSQL 测试 |
| Web 示例 | axum、actix-web、poem 全部作为 workspace member 编译 | `examples/*-postgres` |

M2 仍执行 M1 的 format、Clippy、最小 feature、compile-fail、真实 PostgreSQL 和文档门禁。所有 production Rust 源文件保持在约 600 行以内；测试只能位于各 crate 的 `tests/`，production 源码不放 `#[cfg(test)]`。 源码门禁由 `bash scripts/check-source-lines.sh` 执行，限制为 600 行（包含 examples，排除文档与 reference）。
