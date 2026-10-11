# 前 6 项产品增强

## 数据库能力矩阵

`sift-query-sqlx-common::backend_capabilities` 返回服务端声明的能力集合。PostgreSQL 额外提供 JSON、数组、全文和空间能力；MySQL/MariaDB 与 SQLite 提供同一套便携查询、游标、流式扫描、CSV、Label、Scope 和事务合同。能力矩阵只用于启动检查和元数据，不会在请求路径探测或自动开启数据库扩展。

MySQL 与 MariaDB 共用 SQLx MySQL 驱动。发布门禁仍需要分别在 MySQL 和 MariaDB 服务上执行真实集成测试；SQLite 使用内存或文件数据库执行同一组便携回归。

## 生产级 CSV 导出

三种 SQLx 后端的 `CsvExporter` 都使用有界 scan、批量写出、总超时、输出 flush、字段白名单和 CSV 注入防护。`with_concurrency` 为导出任务设置独立的有界并发闸门；`validate_export_policy` 拒绝零预算和过大的批次，`CsvExportDescriptor` 生成经过校验的 `.csv` 下载文件名和响应头。输出失败时返回原始 I/O 错误；调用方应把已经写出的内容视为 partial output，不要重试同一个响应流。

## 声明式 Label

在 `SearchEntity` 字段上声明：

```rust
#[search_field(label = "order_status")]
status: i32,
```

然后使用 `LabelEnricher::from_entity::<Order>()`，运行时仍由服务端注册 `DictionaryLabelLoader` 或远程 `LabelLoader`。HTTP 不提供字典名；ID 数量、超时、并发、缓存和缺失策略继续由 `LabelEnricher` 控制。

## 通用上下文 PoolRouter

`ContextPoolRouter<C>` 只解析受信任的上下文到已登记的池名，拒绝未知名称、重复名称和非法标识符。PostgreSQL、MySQL/MariaDB、SQLite 分别提供具体的 `*PoolRouter`，执行入口仍保持后端具体类型，避免把不同 SQL 方言和事务类型擦成一个动态执行器。

## Codec / FieldConvertor

`FieldConvertor` 是 `Codec` 的兼容名称；所有转换仍必须声明字段和值类型。新增 `StringCodec`、`ScalarTextCodec` 和可选 `JsonValueCodec`。Codec 指纹进入策略指纹，输入绑定值与结果/CSV 展示值保持分离，游标继续使用数据库值。

## 受控服务端 SQL

`CustomOperator`、`ServerSqlExtension` 和 `ServerSqlRegistry` 只接受启动时编译进服务端的模板。模板有长度、占位符、注释、分号、NUL 和字段白名单限制；请求只提交运算符名称与值，值始终走绑定参数。未知扩展、字段、类型、基数或注册额度会在执行前拒绝。`SqlInterceptor` 仍不接收 bind 值，也不能替代 ServerScope。

应用可在启动时把多个扩展放入 `ServerSqlRegistry`，再通过 `QueryPolicy::with_server_sql_registry::<Entity>()` 按实体校验字段白名单并注册；注册表不会接收请求里的 SQL 文本。
