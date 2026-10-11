# SiftQuery

SiftQuery 是 Rust/sqlx 的类型化只读查询库。应用声明允许查询的字段、运算符和服务端 Scope，库负责参数解析、计划、参数化 SQL、分页和查询预算。

PostgreSQL 后端还提供三项面向生产的能力：`validate_schema::<E>()` 用 `EXPLAIN` 做启动/CI schema 合同校验；`ScopeProvider` 与 `search_authorized` 等入口把认证上下文转换为服务端权限范围；`FacetRequest`、`facets` 和 `search_map_with_facets` 提供受限、复用同一过滤与 Scope 的聚合桶。Facet 只接受实体声明的字段，不接受客户端 SQL。

当前版本还提供一组面向业务查询的增强：字段级访问策略、无 COUNT 的 keyset cursor 分页、服务端声明的 `EXISTS/NOT EXISTS`、Facet 与 DISTINCT 语义、metadata 驱动的 TypeScript `QueryBuilder`、PostgreSQL schema 结果类型校验、`EXPLAIN (FORMAT JSON)` 查询诊断与保守索引建议，以及时间/数值分桶。诊断只返回参数化 SQL、执行计划和建议，不自动创建或修改索引；所有能力继续复用实体元数据、`ServerScope` 和既有查询预算。

当前 PostgreSQL 执行后端包含组合筛选、聚合、签名 cursor、scan/CSV、Codec、Web adapter、Label、观测及完整 RPC；MySQL/MariaDB 与 SQLite 先提供独立的核心查询执行路径。普通 query 不要求事务；SQL 模板缓存不缓存结果。

可选能力已包含全文/JSONB、原生 array/range/network、pg_trgm/ltree/PostGIS、安全 metadata/OpenAPI、RSQL 子集及 RLS 请求上下文。各能力独立 feature，默认关闭；接入与索引合同见 [PostgreSQL 能力](docs/12-POSTGRES-CAPABILITIES.md) 和 [生态接入](docs/13-ECOSYSTEM.md)。

数据库后端按独立 crate/feature 隔离：公共 core 不依赖 sqlx；选择 PostgreSQL、MySQL/MariaDB 或 SQLite 时只引入对应驱动，不作为 PostgreSQL 执行路径的运行时分支。

安装：

```toml
[dependencies]
sift-query = { version = "0.1.0", default-features = false, features = ["postgres", "serde"] }
sqlx = { version = "0.9", default-features = false, features = ["postgres", "runtime-tokio", "macros"] }
```

0.1.0 已发布到 [crates.io](https://crates.io/crates/sift-query)，源码见 [GitHub](https://github.com/wsafight/sift-query)。最低 Rust 版本为 1.99.0；当前 API 为 pre-1.0。

从[安装](docs/guides/zh/installation.md)、[快速开始](docs/guides/zh/quick-start.md)和[数据库选择](docs/guides/zh/backend-selection.md)开始。项目已添加基于 app-struct UI 的中英文文档站点，本地运行及 GitHub Pages 部署见 [site/README.md](site/README.md)。

参见 [文档索引](docs/README.md)、[实现状态](docs/06-STATUS.md)、[变更记录](CHANGELOG.md) 和 [发布指南](docs/11-RELEASE-AND-BACKENDS.md)。Apache-2.0 许可证见 [LICENSE](LICENSE)。
