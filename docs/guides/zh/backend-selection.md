# 选择数据库后端

SiftQuery 支持 PostgreSQL、MySQL/MariaDB 和 SQLite。公共查询模型不依赖 SQLx，执行后端由独立 crate 提供。

## 选择驱动

| 数据库 | Facade feature | 执行器 | 后端 crate |
| --- | --- | --- | --- |
| PostgreSQL | `postgres` | `PgSiftQuery` | `sift-query-sqlx` |
| MySQL / MariaDB | `mysql` | `MySqlSiftQuery` | `sift-query-mysql` |
| SQLite | `sqlite` | `SqliteSiftQuery` | `sift-query-sqlite` |

关闭默认 feature，再启用所需后端。MariaDB 共用 SQLx MySQL 驱动。SQLx 其他版本或驱动支持的数据库，不代表 SiftQuery 已经提供相应后端。

## 能力对比

| 能力 | PostgreSQL | MySQL / MariaDB | SQLite |
| --- | --- | --- | --- |
| 严格参数、字段/运算符白名单、Scope | 支持 | 支持 | 支持 |
| Typed / Map 查询、count、summaries | 支持 | 支持 | 支持 |
| 签名 keyset cursor、scan、CSV | 支持 | 支持 | 支持 |
| 调用方连接 / 事务入口 | 支持 | 支持 | 支持 |
| Codec、Label、观测、Web 参数适配 | 支持 | 支持 | 支持 |
| `ScopeProvider` 授权入口 | 支持 | 支持 | 支持 |
| Schema 校验与 Facet | PG API | — | — |
| 原生 JSONB、数组/范围/网络、全文 | 可选 | — | — |
| pg_trgm / ltree / PostGIS / RLS 上下文 | 可选 | — | — |

能力矩阵描述库合同，不会安装扩展，也不会在普通请求中探测数据库。SQL 方言、聚合输出类型与原生值支持仍需按后端确认。

## 验证边界

发布验证覆盖 PostgreSQL 17、MySQL 8.4、MariaDB 11.4 和 SQLite。MySQL、MariaDB 有独立真实服务回归和 scan/export 后连接复用检查；取消及输出失败的完整故障矩阵仍待补充，部署负载与内存预算仍应在目标环境验收。

详情见[发布与后端隔离](../../11-RELEASE-AND-BACKENDS.md)、[PostgreSQL 能力](../../12-POSTGRES-CAPABILITIES.md)。本地可从 [SQLite 示例](quick-start.md)开始。
