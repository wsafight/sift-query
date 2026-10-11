# 发布与数据库隔离

公共 `sift-query-core` 管理参数、类型、查询模型、Scope 和预算，不依赖 sqlx、连接池或 Web 框架。`sift-query-derive` 只生成静态映射。数据库执行后端按 crate 隔离：`sift-query-sqlx` 提供 PostgreSQL，`sift-query-mysql` 提供 MySQL/MariaDB，`sift-query-sqlite` 提供 SQLite；`sift-query-sqlx-common` 只包含无驱动的共享合同、扫描预算、CSV/观测和 Web 参数适配器，不启用任何数据库驱动。

```toml
sift-query = { version = "0.1.0", default-features = false, features = ["postgres", "serde"] }
sqlx = { version = "0.9", default-features = false, features = ["postgres", "runtime-tokio"] }
```

这样只编译 PG 驱动。MySQL/MariaDB 和 SQLite 使用独立配置：

```toml
sift-query = { version = "0.1.0", default-features = false, features = ["sqlite", "chrono"] }
# 或：features = ["mysql", "chrono", "json"]
```

后端都提供严格参数解析、白名单字段、参数化 SQL、分页、count、动态 map、typed page、summaries、caller-owned connection/transaction、有限 scan 和 CSV 流式导出入口。MySQL 与 MariaDB 共用 SQLx MySQL 驱动；SQLite 使用 SQLx SQLite 驱动。PG 专属的 JSONB、array/range/network、全文搜索、扩展检测和 RLS 不会泄漏到其他后端。

portable 后端的 HTTP 参数 adapter 位于 `sift-query-sqlx-common`，不包含数据库驱动；`mysql-axum` / `mysql-actix-web` / `mysql-poem` 和对应的 `sqlite-*` feature 只打开目标后端及框架 extractor。PostgreSQL 的 `axum` / `actix-web` / `poem` feature 保持原有入口。

后端 feature 不能同时关闭应用其他依赖主动启用的驱动；本库自身的依赖图由 `scripts/check-backend-isolation.sh` 分别验证。

`scripts/check-backend-isolation.sh` 检查正常依赖图；测试依赖与工作区其他示例不代表用户安装依赖。PG 专项能力使用单独 feature，不能导致普通查询自动探测扩展、开启事务或额外访问数据库。缓存仅缓存 SQL 模板，重新绑定当前值。

当前验证矩阵：Rust 1.99.0、sqlx 0.9、PostgreSQL 17、CI 中的 MySQL 8.4 与 MariaDB 11.4，以及 SQLite 文件/内存数据库；三者均有真实查询回归，SQLite 另有事务入口和取消后连接复用回归。MariaDB 与 MySQL 共用 SQLx 驱动，CI 已配置独立服务回归；本地环境未运行 MySQL/MariaDB 服务，取消、输出失败、连接复用等完整故障矩阵仍需发布环境执行。CI 分别验证每个后端 feature，不把 workspace `all-features` 当作单后端安装结果。

代码中的 `backend_capabilities(BackendKind)` 是部署前能力矩阵；它不会替代 MySQL、MariaDB 和 PostgreSQL 服务的真实集成测试。

发布顺序：algorithm → core → derive → sqlx-common → sqlx/mysql/sqlite（任意顺序）→ facade。版本统一管理。0.x 次版本可以包含明确记录的 API 变化，修订版本只修复兼容性问题；协议和 cursor framing 的变更必须说明迁移及旧 token 行为。提升 MSRV 在次版本中记录。

本地发布门禁：格式、Clippy、测试、依赖隔离、文档和 `bash scripts/check-packages.sh`。脚本运行 `cargo package --workspace --all-features --allow-dirty --target-dir <独立临时目录>`，包含实际解包编译。未发布草稿反复使用同一 0.1.0 版本时，Cargo 会缓存临时 registry 的旧源码；独立 target 路径避免取到上次草稿，且不删除用户的全局缓存。生成目录随命令输出保留供检查。包内 README、LICENSE 必须可独立读取，不能依赖工作区外路径。fuzz 独立工作区不进入生产依赖。

当前为未发布 0.1.0；本地打包已验证。公共仓库地址、维护者安全联系方式、crates.io 名称/所有权、真实远端 CI 与注册表安装还需在正式发布环境配置并验证，不能把本地成功记为已发布。发布不由本地检查脚本自动执行。
