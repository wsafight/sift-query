# Bean Searcher 兼容性说明

参考基线：Bean Searcher 4.8.12。SiftQuery 的兼容目标是查询语义与结果，不是 Java API、SQL 别名或占位符文本逐字符一致。

## M1 已映射用例

| Java 测试来源 | M1 对应能力 | Rust 验证位置 | 状态 |
|---|---|---|---|
| `ExprParserTestCase` | AND 优先级、括号、幂等与局部吸收 | `sift-query-core/tests/protocol.rs` | 兼容子集 |
| `OrderByTestCase` | 单字段、多字段和默认排序 | `sift-query-sqlx/tests/bean_searcher_compat.rs` | 语义兼容 |
| `PageResolverTestCase` | 默认分页、page/size 与 offset | `sift-query-core/tests/protocol.rs` | 严格增强 |
| `GroupByTestCase.test_where` | GroupKey 条件进入 WHERE | `sift-query-sqlx/tests/bean_searcher_compat.rs` | 语义兼容 |
| `GroupByTestCase.test_having` | Aggregate 条件进入 HAVING | `sift-query-sqlx/tests/bean_searcher_compat.rs` | 语义兼容 |
| `GroupByTestCase.test_count` | 分组 count 使用子查询 | `sift-query-sqlx/tests/renderer.rs` | 语义兼容 |
| `GroupByTestCase.test_sum` | 对分组结果再次 SUM | `sift-query-sqlx/tests/postgres.rs` | 语义兼容且类型增强 |

## 有意差异

- JDBC `?` 改为 PostgreSQL `$n`。
- Java `c_N` 别名改为稳定、引用后的 Rust 字段别名。
- 未知字段和非法分页不再忽略或返回空集，而是返回 `ParamError`。
- 任意 SQL 参数和 MapBuilder SQL 片段不进入 M1。
- M1 的跨 WHERE/HAVING OR 被拒绝，不生成语义错误的 SQL。
- OFFSET 分页始终追加应用声明的稳定键。
- Java 的部分 Bean 映射拆为完整类型查询和 RowMap 查询。

## M2 已补齐的 Java 对应能力

| Java 能力 | Rust M2 对应 | 差异 |
|---|---|---|
| 剩余 FieldOp | 21 个公开内置 `Operator` | PostgreSQL 使用原生数组与编号占位符 |
| 自定义 FieldOp | `CustomOperator` | 只能由服务端创建的受控模板，不接受 HTTP SQL |
| Select/字段映射 | `SearchProjection<E>` + `search_as` | 编译期静态映射，不反射构造部分 Bean |
| 分页扩展 | HMAC 签名 cursor | 新增安全能力，不追求 Java token 格式兼容 |
| SQL 拦截器 | `SqlInterceptor` | 不暴露绑定值，修改后重算 shape id |
| 结果过滤器 | RowMap `ResultFilter` | 过滤后不改写数据库 total_count |
| 表名解析 | `PartitionResolver` + `PartitionName` | 只允许校验后的 schema/table/alias |
| 聚合 | `SUM/AVG/MIN/MAX/CustomAggregate` | PostgreSQL 输出类型显式化 |

## P0 已补齐的产品能力

完整 RPC 对应 `MapBuilder.buildForRpc/RpcNames`；HTTP 运算符对应 `FieldOpPool/DbField.onlyOn`；映射复用对应继承与命名策略；命名/动态 pool 对应 `NamedDataSource/AbstractRoutingDataSource`；时间/时区与简单列表 Codec 对应 Java 转换器。具体源码/文档链接、Rust API 和验证位置见 [10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md](./10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md)。

Rust 使用编译期组合、显式 sqlx alias 与受信任上下文，保留字段白名单和严格错误，不复制 Java 反射、运算符静默回退或客户端动态 SQL。Label/CSV 与其他首批 P0 扩展见 [08-P0-EXTENSIONS.md](./08-P0-EXTENSIONS.md)。

## 当前不兼容或不在范围

- MySQL/MariaDB 和 SQLite 不纳入本 Bean Searcher PostgreSQL 兼容矩阵；它们由 P0.16 独立后端能力矩阵验证。
- Java 动态方言和异构多数据源。
- 任意 SQL 参数、未校验的 MapBuilder SQL 片段。
- 无界 `searchAll`、反射转换复杂对象列表和运行时修改 schema；用有界 scan、显式增强 trait 和编译期元信息替代。

M1 兼容矩阵仍用于回归；M2 新能力使用 Rust API 和 PostgreSQL 结果语义验收，不承诺 Java API 或 SQL 文本兼容。
