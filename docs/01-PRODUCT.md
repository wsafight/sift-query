# SiftQuery 产品规格

版本：v0.4

状态：M2 PostgreSQL 实现规格；MySQL/MariaDB、SQLite 通过独立 P0.16 后端扩展

## 1. 产品定位

SiftQuery 面向使用 axum、actix-web 或 poem 构建后台管理系统、运营平台和数据看板的 Rust 团队。

它负责把受约束的查询参数转换为类型化查询计划，再渲染为参数化 SQL。它与 sqlx 的分工是：

- sqlx 负责连接池、数据库协议、事务、SQL 执行和基础行解码。
- SiftQuery 负责允许查询哪些字段、如何组合条件、如何排序分页、如何生成稳定 SQL，以及如何限制查询成本。
- 写入、迁移、实体关系、领域事务和业务授权继续由应用或 ORM 负责。

SiftQuery 不是完整 ORM，也不是允许客户端提交任意 SQL 的查询网关。

## 2. 目标场景

- 内部管理后台的多条件列表。
- 运营平台的组合筛选与排序。
- 数据看板的列表、总数和受控聚合。
- 同一个只读查询实体被多个 HTTP 接口或后台任务复用。
- 希望保留 sqlx 显式执行模型，但减少重复参数解析和 SQL 条件拼装的服务。

不建议直接使用：

- 未经应用级认证、授权和限流保护的公网通用查询 API。
- 需要写事务、实体状态追踪、关系级联或懒加载的业务。
- 大规模 OLAP、任意维度分析和用户自定义 SQL。
- 不允许事务、但要求列表与总数严格来自同一数据库快照的接口。

## 3. 产品原则

### 3.1 Rust 原生优先

Bean Searcher 是行为参考，不是逐行翻译模板。公开 API 必须符合 Rust 的所有权、静态类型、显式错误和异步执行模型。

### 3.2 语义兼容优先

兼容目标是查询结果语义和错误分类，而不是 SQL 文本逐字符相同：

- 相同字段、运算符和合法逻辑分组产生相同的谓词语义。
- 绑定值顺序稳定且可测试。
- 在相同数据库快照下，列表、总数和聚合结果符合兼容矩阵。
- PostgreSQL 使用 `$1`、`$2`，不模拟 JDBC `?` 占位符。
- 内部列别名不复制 Java 的 `c_0`、`c_1`。

兼容范围以 [05-IMPLEMENTATION.md](./05-IMPLEMENTATION.md) 的矩阵为准。严格错误策略、有意改变项和未进入 M1 的运算符不计入 M1 兼容通过率。

### 3.3 不可信输入与服务端策略分离

HTTP 输入只能构造 `UserQuery<E>`。租户、数据权限、软删除等强制条件只能由服务端代码构造 `ServerScope<E>`。

最终行级谓词始终满足：

```text
base_where AND server_scope AND user_row_predicate
```

最终分组谓词始终满足：

```text
fixed_having AND user_group_predicate
```

客户端不能删除、覆盖或改写 `base_where`、`ServerScope` 和 `fixed_having`。

### 3.4 类型化结果与动态投影分离

- `search<E>` 返回完整的 `E`，不接受动态字段裁剪。
- `search_map<E>` 返回 `RowMap`，支持 `onlySelect` 或 `selectExclude`。
- `search_as<E, P>` 使用编译期静态投影类型，不依赖反射或未初始化字段。

M1 禁止通过反射或未初始化字段构造部分实体。

### 3.5 默认严格失败

未知字段、未知运算符、错误元数、冲突参数、越界分页和非法表达式必须返回参数错误，并且不得获取数据库连接或执行 SQL。

Bean Searcher 的宽松行为只能通过 M2 的命名兼容策略显式启用，不能用单个布尔开关混合不同语义。

### 3.6 正确性优先于优化

投影裁剪、统计 SQL 精简、数组绑定和模板缓存不得改变 NULL、DISTINCT、GROUP BY、HAVING、排序和聚合语义。

无法证明等价的优化必须关闭或返回明确错误。

## 4. 查询生命周期

```text
raw URL query string
        |
        v
RawParams                         有序、多值、未信任
        |
        v
ParamParser<E> + QueryPolicy      解析、字段校验、预算检查
        |
        v
UserQuery<E>                      已绑定实体的用户查询 AST
        |
        +---- ServerScope<E>      服务端行级强制条件
        +---- SearchOptions<E>    服务端聚合请求
        |
        v
QueryPlan<E>                      投影、两阶段谓词、排序、分页、聚合
        |
        v
PostgresRenderer                  PostgreSQL SQL + 有序类型化绑定值
        |
        v
PgSiftQuery                       sqlx 执行、超时、解码、tracing
        |
        +---- Page<E>
        +---- Page<RowMap>
```

`UserQuery<E>`、`ServerScope<E>` 和 `QueryPlan<E>` 都绑定实体类型，禁止把一个实体解析出的字段编号用于另一个实体。

## 5. crate 边界

### 5.1 `sift-query-algorithm`

完全不依赖查询领域类型、sqlx 或 Web 框架，包含：

- 通用有界 LRU Map。
- 用于可复现指纹的稳定 64 位哈希累加器。

该 crate 禁止出现 Entity、QueryPlan、SQL、数据库方言等领域概念；只有形成第二个真实调用点的通用算法才进入这里。

### 5.2 `sift-query-core`

不依赖 sqlx，包含：

- 实体和字段的后端无关元信息。
- `RawParams`、`UserQuery<E>`、`ServerScope<E>` 和 `QueryPlan<E>`。
- 参数解析、预算检查和核心错误。
- 条件 AST、排序、分页、投影和聚合描述。
- 后端无关的标量值、类型化列表和绑定槽位描述。

### 5.3 `sift-query-derive`

包含 `SearchEntity` derive 和相关属性，负责：

- 生成静态实体元信息和类型化字段常量。
- 验证字段外部名称、保留名称、重复名称和结果别名。
- 验证默认排序字段和稳定分页键。
- 为分组实体生成 GROUP BY 元信息。
- 尽可能在编译期拒绝明显不合法的组合。

### 5.4 `sift-query-sqlx`

当前版本只提供 PostgreSQL 后端：

- PostgreSQL 渲染器。
- 标量和数组绑定器。
- 完整实体与 RowMap 解码。
- 列表、统计、超时、取消和 tracing。
- `PgSearchError`，其中可以包含 `sqlx::Error`。

### 5.5 `sift-query`

Facade crate，重导出常用 core、derive 和 PostgreSQL 类型。`postgres` feature 打开 sqlx 集成。

algorithm 不得依赖任何领域 crate。core 不得直接公开或依赖 `sqlx::Error`。启用 PostgreSQL feature 时，facade 可以把 `PgSearchError` 重导出为面向用户的 `SearchError`。

## 6. 当前数据库与执行模型

当前 M1 + M2 实现只支持 PostgreSQL：

- 使用 sqlx 0.9 和 tokio。
- 使用 PostgreSQL 原生编号占位符。
- `PgSiftQuery` 固定组合 `PgPool`、PostgreSQL renderer、binder 和 decoder。
- 不提供可在运行时与 MySQL、SQLite renderer 混用的通用执行器。

`search` 默认执行列表 SQL 和统计 SQL。默认使用 `SequentialBestEffort`：两条语句不承诺同一快照，并发写入时允许短暂不一致。

M2 提供 `ParallelBestEffort`、`Snapshot(RepeatableRead)` 和 `Snapshot(Serializable)`。Snapshot 在同一连接、只读显式事务中依次执行 list 与 cluster；任何失败都会关闭该连接，成功提交后才归还连接池。

## 7. M1 与后续里程碑

### 7.1 M0：可行性门禁

M0 必须在公开 API 冻结前验证：

- sqlx 动态标量绑定和 PostgreSQL 类型化数组绑定。
- 空数组、NULL 和 `ANY` 的真实数据库语义。
- `SUM` 输入到输出类型映射。
- `FromRow` 别名、可空类型和被忽略字段的约束。
- GROUP BY、HAVING、DISTINCT 和动态投影的结果语义。
- 查询超时后的连接安全和服务器端取消行为。
- 核心解析与渲染性能基线。
- 最终 MSRV。

任一门禁失败时，应修改规格而不是在实现中添加未记录的例外。

### 7.2 M1：PostgreSQL 可用核心

- 本规格定义的 12 个运算符。
- 严格参数协议和查询预算。
- 类型化 `UserQuery<E>` 与 `ServerScope<E>`。
- 完整实体和动态 RowMap 查询。
- 确定性 SQL、数组绑定和稳定分页。
- 列表、总数、基础聚合和两阶段谓词。
- PostgreSQL 集成测试、compile test 和 axum 示例。

### 7.3 M2：生产增强

- [x] 剩余 Bean Searcher 运算符及受控自定义运算符。
- [x] `search_as`、HMAC 签名游标分页和取消安全的流式读取。
- [x] `ParallelBestEffort` 与只读事务 `Snapshot`。
- [x] 有界 Query Shape Cache 和显式窗口计数策略。
- [x] 命名的 `BeanSearcher4812` 兼容策略。
- [x] SQL 拦截器、RowMap 结果过滤器和安全分区解析器。
- [x] `SUM/AVG/MIN/MAX` 及受控自定义服务端聚合。
- [x] axum、actix-web 和 poem 的 PostgreSQL 示例。

本次 M2 PostgreSQL 交付不要求 MySQL/MariaDB 和 SQLite 达到同等能力；它们由 P0.16 独立后端路线推进，不作为 PostgreSQL 完成条件。

### 7.4 M3：生态评估

- 同构及异构多数据源注册表。
- SQL Server、Oracle、达梦等 renderer 和驱动可用性评估。
- 导出和标签扩展 crate。

## 8. 非目标

- 写入、迁移和实体状态追踪。
- ORM 关系图与自动 JOIN 推导。
- 自动读取数据库 schema。
- 通过 HTTP 接受任意表名、列名或 SQL 片段。
- 完整版的 MySQL/MariaDB、SQLite 能力和异构数据源；P0.16 先提供前两者的核心查询路径。
- 类型化结果的运行时字段裁剪；动态裁剪使用 RowMap，静态裁剪使用 `search_as`。
- GraphQL、OData 或 JSON:API 协议实现。
- 核心库内置查询结果缓存。
- CSV、Excel 导出和枚举标签翻译。

## 9. 已拍板决策

1. 产品名为 SiftQuery，facade crate 为 `sift-query`。
2. M1 的完整端到端合同以 PostgreSQL 为基线；其他后端使用独立能力矩阵。
3. Java 兼容目标由显式矩阵定义，不追求 SQL 字符串一致。
4. 完整类型结果与动态 RowMap 投影分离。
5. HTTP `UserQuery<E>` 与服务端 `ServerScope<E>` 分离。
6. 默认严格返回参数错误。
7. HTTP 输入不能生成 SQL 标识符和 SQL 片段。
8. derive 生成静态元信息，不保留运行时反射式 MetaResolver。
9. 数据库执行器固定 pool、renderer、binder 和 decoder。
10. M1 的程序化查询与 HTTP 查询最终生成同一种 `UserQuery<E>`。
11. M1 拒绝无法保持语义的跨 WHERE/HAVING OR。
12. M1 分页必须具备唯一稳定的最终排序。
13. 查询结果缓存、全局并行 count 和窗口计数不作为默认优化。
14. 所有公开示例必须参加编译或集成测试。
15. M2 仍以 PostgreSQL 为完整实现；MySQL/MariaDB 和 SQLite 按 P0.16 独立交付，不能隐式引入其他驱动。
16. Shape Cache、窗口计数、并行执行和扩展钩子必须显式配置，默认行为保持可预测。
17. `search_stream` 不执行 count；提前丢弃流时连接不得带着活动语句返回连接池。
