# SiftQuery 文档索引

> Rust 生态中参数驱动的只读查询层。实体声明查询边界，参数描述查询意图，框架生成安全、确定且可观测的 SQL。

当前发布：0.1.0，8 个库 crate 已发布到 crates.io，API 为 pre-1.0。设计规范版本 v0.4 与 crate 版本分开管理。

主数据库合同：PostgreSQL；MySQL/MariaDB 与 SQLite 通过独立后端 crate 提供核心查询路径

数据库访问：sqlx 0.9

异步运行时：tokio

参考实现：Bean Searcher 4.8.12（Java）

## 从使用开始

- [安装](guides/zh/installation.md)：选择数据库和 feature。
- [快速开始](guides/zh/quick-start.md)：完整可运行的 SQLite 示例。
- [查询示例](guides/zh/query-cookbook.md)：筛选、排序、投影与分页。
- [选择数据库后端](guides/zh/backend-selection.md)：共有能力和 PostgreSQL 专属能力。
- [English overview](guides/en/overview.md)：英文入门与精简参考指南。

站点 UI 沿用 app-struct，提供中英文切换、全文搜索、主题、移动导航和页内目录。本地运行与部署说明见 [site/README.md](../site/README.md)。

## 1. 文档结构

以下文件共同组成 v0.4 规格，按顺序阅读：

1. [01-PRODUCT.md](./01-PRODUCT.md)：产品边界、原则、里程碑和已经拍板的决策。
2. [02-PROTOCOL.md](./02-PROTOCOL.md)：URL 查询参数协议、语法、冲突规则、查询预算和参数错误。
3. [03-QUERY-SEMANTICS.md](./03-QUERY-SEMANTICS.md)：条件、WHERE/HAVING、投影、DISTINCT、分页、聚合和 SQL 语义。
4. [04-RUST-API.md](./04-RUST-API.md)：crate 分层、实体声明、核心类型、公开 API 和 PostgreSQL 类型映射。
5. [05-IMPLEMENTATION.md](./05-IMPLEMENTATION.md)：实施顺序、测试矩阵、兼容边界、性能基准和验收标准。
6. [06-STATUS.md](./06-STATUS.md)：当前 Rust 1.99 实现进度、已验证能力和下一阶段门禁。
7. [07-BEAN-SEARCHER-CONFORMANCE.md](./07-BEAN-SEARCHER-CONFORMANCE.md)：Java 参考实现与 M1/M2 用例的对应关系及有意差异。
8. [08-P0-EXTENSIONS.md](./08-P0-EXTENSIONS.md)：参数扩展、GroupScope、scan/CSV、Web adapter、增强与观测、外部事务的实现合同。
9. [09-TRANSPORT-AND-PERFORMANCE.md](./09-TRANSPORT-AND-PERFORMANCE.md)：GET/POST 同协议、前端自动切换、渲染前模板缓存及性能验证。
10. [10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md](./10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md)：完整 RPC、HTTP 运算符、映射复用、多 pool 路由、时间/列表 Codec 的 Java 对应与 Rust 合同。
11. [11-RELEASE-AND-BACKENDS.md](./11-RELEASE-AND-BACKENDS.md)：发布门禁、支持矩阵与独立数据库后端边界。
12. [12-POSTGRES-CAPABILITIES.md](./12-POSTGRES-CAPABILITIES.md)：可选全文、JSONB、原生值、扩展、受控排序和索引指南。
13. [13-ECOSYSTEM.md](./13-ECOSYSTEM.md)：安全元数据/OpenAPI、RSQL 子集与事务内 RLS 上下文。
14. [14-PERFORMANCE-BASELINE.md](./14-PERFORMANCE-BASELINE.md)：本地与目标环境性能基线的采集方式和边界。
15. [15-PRODUCT-ENHANCEMENTS.md](./15-PRODUCT-ENHANCEMENTS.md)：能力矩阵、CSV、Label、可信路由、Codec 与受控 SQL 扩展。

发生冲突时，优先级为：

1. `02-PROTOCOL.md` 与 `03-QUERY-SEMANTICS.md` 中标记为 MUST 的规范性条款。
2. `04-RUST-API.md` 中的公开类型与 API 契约。
3. `01-PRODUCT.md` 中的产品原则。
4. `05-IMPLEMENTATION.md` 中的路线和非规范性建议。
5. `06-STATUS.md` 只描述实现状态，不修改规范语义。
6. `07-BEAN-SEARCHER-CONFORMANCE.md` 只定义兼容测试范围，不覆盖 v0.4 的严格语义。

早期 v0.3 设计不作为当前实现依据；当前发布事实以 `11-RELEASE-AND-BACKENDS.md` 与 `CHANGELOG.md` 为准。

## 2. 规范用语

- **MUST / 必须**：实现和兼容测试不可违反。
- **MUST NOT / 禁止**：实现不得提供该行为。
- **SHOULD / 应该**：除非有记录充分的理由，否则应遵守。
- **MAY / 可以**：可选能力，不构成兼容性承诺。

## 3. 当前实现范围

当前版本完整承诺 PostgreSQL 端到端能力，并为 MySQL/MariaDB、SQLite 提供独立的核心查询后端：

- 独立的 `sift-query-algorithm` 通用算法 crate；不依赖查询领域或数据库类型。
- derive 生成静态实体和字段元信息。
- URL query string 到 `UserQuery<E>` 的严格解析。
- 服务端 `ServerScope<E>` 与用户条件的隔离。
- 21 个内置运算符及服务端受控自定义运算符。
- 逻辑分组及 WHERE/HAVING 阶段校验。
- 稳定排序、分页和动态 RowMap 投影。
- `search`、`search_map`、`search_as`、`search_list`、`search_first`、`search_stream`、`count` 和 `sum`。
- PostgreSQL 原生编号占位符和类型化数组绑定。
- 列表、总数、`SUM/AVG/MIN/MAX` 及受控自定义服务端聚合。
- 查询预算、超时、tracing 和结构化错误。
- HMAC-SHA256 签名 keyset cursor。
- 顺序、受限并行和同快照执行策略。
- 显式窗口 count、有界 SQL shape cache。
- 分区解析器、SQL 拦截器和 RowMap 结果过滤器。
- axum、actix-web 和 poem 示例。
- P0 四组业务扩展：参数过滤/Codec/GroupScope、安全 scan/CSV/Web adapter、Label/PageEnricher/观测，以及可选外部事务入口；详见 `08-P0-EXTENSIONS.md`。
- 完整 QueryBuilder RPC、HTTP 自定义运算符白名单、编译期映射复用、命名/可信上下文多 pool 路由、时间/时区/列表 Codec；详见 `10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md`。
- 可选 PostgreSQL 原生搜索、array/range/network、pg_trgm/ltree/PostGIS、metadata/OpenAPI、RSQL 和 RLS；详见 12/13。

PostgreSQL 专属扩展和完整 P0 生产能力不会自动进入其他后端；MySQL/SQLite 已对齐核心分页、count、summaries、外部连接入口、Codec、安全 scan、CSV 流式导出、Web 参数 adapter 和基础 PageEnricher/Label/QueryObserver，MySQL/MariaDB 真实服务回归与 scan/export 后连接复用已通过发布 CI；完整取消、输出失败故障矩阵与目标环境性能验收仍待补充。所有后端都不接受来自 HTTP 的任意 SQL、表名或列名。Query Shape Cache 只缓存 SQL 模板，不缓存查询结果。

## 4. v0.4 解决的关键问题

相比 v0.3，v0.4 明确修正以下实现阻断项：

1. `SUM` 输出类型不再错误地等同于字段输入类型。
2. 查询绑定值与结果值分离，数组绑定携带明确的元素类型。
3. `QueryBuilder<E>` 生成的 `UserQuery<E>` 拥有正式执行入口。
4. 跨 WHERE/HAVING 阶段的 OR 表达式在 M1 被明确拒绝。
5. `DISTINCT + RowMap 投影 + 排序` 不再通过隐藏列静默改变去重语义。
6. 参数协议使用明确语法、运算符元数、重复参数和冲突规则。
7. OFFSET 分页必须具有唯一、稳定的最终排序，并检查计算后的 offset。
8. `group_by` 从 `GroupKey` 字段生成，避免字符串配置与字段标记漂移。
9. core 错误与 sqlx 后端错误分层。
10. Java 兼容改为显式兼容矩阵，不再笼统要求所有用例通过。

## 5. 设计底线

- HTTP 输入不能提供表名、列名、SQL 片段或 `ServerScope`。
- 参数错误发生时不得开始任何数据库操作。
- 所有标识符都必须来自编译期元信息；所有运行时值都必须绑定。
- 查询语义优先于 SQL 长度、缓存命中率和微小的分配优化。
- M1 不为了兼容 Java 的宽松行为而降低默认安全性和可诊断性。
- 文档中的公开代码示例必须进入 compile test 或 doctest。
