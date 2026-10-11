# 安装

[SiftQuery 0.1.0](https://crates.io/crates/sift-query) 已发布到 crates.io。需要 **Rust 1.99.0 或更新版本**，SQLx 集成使用 **0.9**。

## 安装 SQLite 配置

用 `cargo new sift-demo` 创建二进制项目，在 `Cargo.toml` 中添加：

```toml
[dependencies]
sift-query = { version = "0.1.0", default-features = false, features = ["sqlite", "serde"] }
sqlx = { version = "0.9", default-features = false, features = ["sqlite", "runtime-tokio", "macros"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

[快速开始](quick-start.md) 使用这套配置，可直接运行，无需外部数据库服务。

## 选择 PostgreSQL 或 MySQL

PostgreSQL：把两处 `sqlite` feature 都换成 `postgres`，执行器使用 `PgSiftQuery`。

MySQL/MariaDB：把两处 `sqlite` 都换成 `mysql`，执行器使用 `MySqlSiftQuery`。MariaDB 共用 SQLx MySQL 驱动。

默认 feature 会启用 PostgreSQL，以及 `chrono`、`decimal`、`json`、`serde`、`uuid`。切换数据库时设置 `default-features = false`，再显式选择目标后端。Cargo feature 会合并，应用中的其他依赖仍可能主动启用额外驱动。

## 可选集成

| Feature | 用途 |
| --- | --- |
| `serde` | 序列化支持 |
| `chrono`、`json`、`uuid` | 可选值类型，具体支持取决于后端 |
| `decimal` | PostgreSQL 与 MySQL 的 Decimal 支持 |
| `axum`、`actix-web`、`poem` | PostgreSQL Web 集成 |
| `mysql-axum`、`sqlite-axum` | 对应后端的 Axum 集成 |
| `metadata`、`rsql` | 查询元数据与有预算的 RSQL 解析 |
| `postgres-search`、`postgres-types` | PostgreSQL 原生搜索和值类型 |
| `pg-trgm`、`ltree`、`postgis`、`rls` | PostgreSQL 可选扩展与 RLS 上下文 |

其他框架可使用 `mysql-actix-web`、`sqlite-actix-web`、`mysql-poem`、`sqlite-poem`。选择原生功能前先阅读[后端指南](backend-selection.md)。

## API 文档

生成的 Rust API 文档见 [docs.rs](https://docs.rs/sift-query)，公开合同和执行入口见 [Rust API](../../04-RUST-API.md)。许可证为 Apache-2.0。
