# SiftQuery M2 查询语义

本文件定义 `UserQuery<E>` 到 PostgreSQL 查询结果之间的规范语义。任何 SQL 重写和性能优化都必须保持本文件定义的结果。

## 1. 实体字段种类

M1 将查询字段分为四类：

| 字段种类 | 进入类型化结果 | 可进入 RowMap | 条件阶段 | 进入 GROUP BY |
|---|---:|---:|---|---:|
| `Scalar` | 是 | 由 `selectable` 决定 | Row / WHERE | 否 |
| `GroupKey` | 是 | 由 `selectable` 决定 | Row / WHERE | 是 |
| `Aggregate` | 是 | 由 `selectable` 决定 | Group / HAVING | 否 |
| `FilterOnly<T>` | 否 | 否 | Row / WHERE | 否 |

说明：

- 非分组实体通常只包含 `Scalar` 和可选的 `FilterOnly<T>`。
- 分组实体的所有结果字段必须是 `GroupKey` 或 `Aggregate`。
- 分组实体可以使用 `FilterOnly<T>` 在聚合前过滤原始行。
- 分组实体禁止包含可返回的 `Scalar` 字段，因为 PostgreSQL 无法在不聚合、不分组的情况下确定其值。
- `GroupKey` 条件规范化为 WHERE，以便在分组前下推。
- `Aggregate` 条件进入 HAVING。

M1 的 GROUP BY 表达式必须从所有 `GroupKey.column` 生成，不再同时维护自由字符串形式的 `group_by`。

## 2. 条件 AST 与阶段

每个条件节点在解析后拥有确定阶段：

```rust
pub enum PredicatePhase {
    Row,
    Group,
}
```

- `Scalar`、`GroupKey`、`FilterOnly<T>` 产生 Row 条件。
- `Aggregate` 产生 Group 条件。
- `ServerScope<E>` 只能使用 Row-capable 字段和 Row 运算符。
- `base_where` 是固定 Row 条件。
- `fixed_having` 是固定 Group 条件。

### 2.1 AND 的阶段拆分

AND 可以安全拆分到两个阶段：

```text
row_A AND group_B
```

渲染为：

```sql
WHERE row_A
HAVING group_B
```

### 2.2 OR 的阶段限制

OR 节点的所有后代必须属于同一阶段：

- `row_A OR row_B` 合法，整体进入 WHERE。
- `group_A OR group_B` 合法，整体进入 HAVING。
- `row_A OR group_B` 非法，返回 `MixedPredicatePhase`。
- `(row_A AND row_B) OR group_C` 非法。

M1 禁止通过把跨阶段 OR 改写成 WHERE 与 HAVING 的 AND 来改变语义，也不为此自动构造复杂派生表。

### 2.3 最终谓词

规范化后：

```text
WHERE  = base_where AND server_scope AND user_row_predicate
HAVING = fixed_having AND user_group_predicate
```

不存在某阶段条件时不生成对应子句。ServerScope 始终在用户组表达式外层，用户条件不能削弱它。

## 3. 条件顺序与规范化

SQL 与绑定值顺序按以下规则确定：

1. `base_where` 不含运行时绑定值。
2. `ServerScope` 按服务端构建顺序。
3. 未分组用户条件按实体字段声明顺序。
4. 命名组按照 gexpr AST 第一次出现顺序。
5. 同一组内按实体字段声明顺序。
6. 重复键值按 RawParams 出现顺序；编号值按编号顺序。
7. 客户端排序按请求顺序，服务端稳定键随后追加。

规范化 MAY 执行以下有界变换：

- `A AND A -> A`。
- `A OR A -> A`。
- 常量折叠。
- 不需要指数展开的局部吸收律。

M1 MUST NOT 使用无界分配律展开，也不承诺生成逻辑最简表达式。

## 4. 运算符语义

### 4.1 比较运算符

`eq`、`ne`、`gt`、`ge`、`lt` 和 `le` 各绑定一个已转换的非 NULL 值。

HTTP 空字符串是普通字符串。非字符串字段收到空字符串时，如果转换失败则返回 `InvalidValue`。

SQL NULL 只能通过 `nl` 和 `nn` 查询。M1 不把空字符串、字符串 `null` 或缺失字段隐式转换为 NULL。

### 4.2 Between

`bt` 使用两个同类型值，语义是包含边界：

```sql
column BETWEEN $1 AND $2
```

M1 不自动交换上下界。下界大于上界仍生成合法 SQL，通常得到空结果。应用若需要拒绝该输入，应使用自定义参数过滤器；M1 不对所有可排序类型定义跨类型大小比较。

### 4.3 InList

PostgreSQL 后端使用一个类型化数组绑定：

```sql
column = ANY($1)
```

规范要求：

- 数组至少包含一个元素。
- 数组元素必须为同一字段查询类型。
- HTTP 输入不允许数组中的 NULL。
- 不同数组长度生成相同 SQL 文本。
- 绑定模型必须携带元素类型，即使未来支持空数组也不能依赖第一个元素推断类型。

`NotIn` 使用与 PostgreSQL `NOT IN` 三值逻辑等价的 `column <> ALL($n)`。HTTP 数组不能含 NULL；列值为 NULL 时谓词结果仍是 UNKNOWN，不会被错误地当成 true。

### 4.4 字符串匹配

`sw` 和 `ct` 是字面匹配，不允许用户输入 `%`、`_` 或 `\` 获得额外通配能力。

渲染器必须先转义这三个字符，再添加框架自己的通配符，并显式生成 ESCAPE 子句：

```sql
column LIKE $1 ESCAPE '\'
column ILIKE $1 ESCAPE '\'
```

- `sw(value)` 绑定 `escaped(value) + "%"`。
- `ct(value)` 绑定 `"%" + escaped(value) + "%"`。
- `ew(value)` 绑定 `"%" + escaped(value)`。
- `ic=true` 时使用 ILIKE。

`ol` 和 `nk` 接受 LIKE pattern，并保留其中的 `%` 与 `_`；它们用于需要显式 pattern 语义的受控接口。`ol` 将多个 pattern 用 OR 连接。`ey/ny` 分别定义为 `NULL 或空字符串` 与 `非 NULL 且非空字符串`。

### 4.5 常量与自定义运算符

`at/af` 分别渲染为常量真和常量假。受控 `CustomOperator` 只能由服务端代码创建，模板必须包含 `{field}`，按声明 arity 使用 `{0}` 等值槽位，并拒绝 `$`、`;`、行注释和块注释标记。HTTP 输入不能创建或选择自定义模板。

`eq/ne + ic=true` 使用 PostgreSQL 明确的大小写归一化表达式；M1 基线为：

```sql
lower(column) = lower($1)
lower(column) <> lower($1)
```

其大小写行为受数据库字符集和 collation 影响。SiftQuery 不声称等价于 Unicode 全量 case folding，也不承诺普通 B-tree 索引自动生效。生产使用方可按查询形态选择 citext、函数索引、`text_pattern_ops` 或 pg_trgm。

## 5. UserQuery 与 ServerScope 合成

`UserQuery<E>` 只包含：

- 用户条件 AST。
- 动态投影请求。
- 客户端排序。
- 分页。

它不得包含：

- 表名、列名或 SQL 片段。
- `base_where` 或 `fixed_having`。
- 租户和权限条件。
- 服务端聚合请求。

`ServerScope<E>` 只能由服务端 API 构建。默认没有从字符串或 RawParams 反序列化 ServerScope 的 API。

`ServerScope::unrestricted()` 是显式能力，用于确认该调用允许访问实体全部基础数据。示例代码可以使用它，但安全敏感应用 SHOULD 通过封装后的仓储函数提供 scope，避免业务入口随意调用。

## 6. 投影语义

### 6.1 完整类型

`search<E>`、`search_with<E>`、`search_list<E>` 和 `search_first<E>` 查询构造 `E` 所需的全部结果字段。

- `FilterOnly<T>` 和 `search_ignore` 字段不进入 SELECT。
- 收到 `onlySelect` 或 `selectExclude` 时返回 `ProjectionNotAllowed`。
- SELECT 中每个表达式使用 derive 确认过的结果别名。
- 完整类型解码必须保留 `sqlx::FromRow` 的 NULL 和类型错误行为。

### 6.2 RowMap

`search_map<E>` 和 `search_map_with<E>` 根据最终投影只选择必要字段。

- 没有投影参数时选择所有 `selectable=true` 的结果字段。
- `onlySelect` 明确包含字段。
- `selectExclude` 从所有可选择字段中排除字段。
- RowMap 键使用字段公开名称，并按实体声明顺序排列。
- 用于过滤的 `FilterOnly<T>`、被排除字段和只服务于统计 SQL 的表达式不得进入最终 RowMap。

渲染器可以在非 DISTINCT 查询的内部 SELECT 中增加数据库需要的隐藏表达式，但 decoder 必须根据投影表排除隐藏列。

### 6.3 RowMap 值

RowMap 值由后端的 `RowValue` 表示。M1 必须明确支持的 PostgreSQL 类型集合；不在集合中的类型返回 `UnsupportedColumnType` 或 `DecodeError`，禁止回退成调试字符串。

启用 `serde` feature 时，RowValue 使用稳定 JSON 映射：NULL 为 `null`，Bool/I32 为 JSON 原生值，I64 和 Decimal 为十进制字符串，Bytes 为无填充 base64url，日期时间与 UUID 为规范字符串，JSON/JSONB 保持原生 JSON 结构。有限 F32/F64 为 JSON number，NaN 和正负 Infinity 返回序列化错误。I64 使用字符串是为了避免 JavaScript IEEE-754 精度丢失。

启用 postgres-types 时，原生 bigint[] 为字符串或 null 的 JSON 数组，text[] 为字符串或 null 的数组；int8range 和 inet/cidr 为规范字符串。JSON number 保留 JSON 自身语义，客户端应选择能保留其数值精度的解码器。typed DTO 的 serde 映射由其自身实现决定，RowMap 的稳定映射不会改写 DTO 的 Serialize。

### 6.4 静态投影

`search_as<E, P>` 要求 `P: SearchProjection<E> + FromRow<PgRow>`。投影源字段与结果别名由 derive 在编译期固化，HTTP 动态投影参数仍被拒绝。游标模式下，最终排序字段必须全部包含在 P 中，以便安全生成下一页 token。

## 7. DISTINCT

`distinct = true` 的语义是对用户最终可见投影去重：

- 完整类型查询：对构造 E 的全部结果字段去重。
- RowMap 查询：对 onlySelect/selectExclude 计算后的最终字段集合去重。

### 7.1 DISTINCT 与排序

PostgreSQL 的 SELECT DISTINCT 要求排序表达式能够由投影确定。为避免隐藏排序列改变去重键，M1 使用以下规则：

- DISTINCT RowMap 查询的每个客户端排序字段必须存在于最终投影。
- 实体稳定排序字段同样必须存在于最终投影。
- 任一字段缺失时返回 `DistinctOrderNotProjected`。
- 禁止通过把隐藏排序字段加入 DISTINCT SELECT 后再从 RowMap 删除的方式绕过检查。

完整类型查询选择全部结果字段，因此只要排序字段本身是结果字段即可。`FilterOnly<T>` 不能作为 DISTINCT 查询的排序字段。

## 8. GROUP BY 与聚合实体

### 8.1 GROUP BY 生成

M1 从 `GroupKey` 字段的 column 表达式按实体声明顺序生成 GROUP BY。实体级自由字符串 `group_by` 不进入 M1。

至少存在一个 `GroupKey` 或 `Aggregate` 字段时，该实体被视为聚合实体。derive 必须检查：

- 所有结果字段都是 `GroupKey` 或 `Aggregate`。
- 至少存在一个结果字段。
- `GroupKey` 和 `Aggregate` 字段的结果别名互不冲突。
- `FilterOnly<T>` 只用于 WHERE。

### 8.2 Aggregate 字段

`Aggregate` 字段的 column 是受信任的聚合表达式，例如 `sum(score)`。字段 Rust 类型描述该表达式的数据库输出类型，而不是其输入列类型。

对 Aggregate 字段的筛选进入 HAVING；对 GroupKey 和 FilterOnly 字段的筛选进入 WHERE。

### 8.3 固定 HAVING

`fixed_having` 是实体作者提供的受信任 SQL 表达式，不接受 HTTP 值，也不能包含 SiftQuery 管理的绑定占位符。它始终与用户 Group 条件做 AND。

## 9. 排序与稳定分页

### 9.1 稳定排序合同

每个允许分页返回数据的实体必须声明 `stable_order`。该字段元组必须由应用保证在最终结果集上唯一。

示例：

```text
default_order = created_at:desc
stable_order  = id:asc
```

生成顺序：

1. 有客户端排序时先保留客户端顺序。
2. 没有客户端排序时使用实体 `default_order`。
3. 把尚未出现的 `stable_order` 字段按声明方向追加到末尾。

最终 ORDER BY 必须构成全序。若实体没有稳定键，或 DISTINCT/投影使稳定键不可用，则分页查询返回 `UnstablePagination` 或更具体的投影错误。

derive 只能验证字段存在、可排序和没有重复；唯一性是应用与数据库 schema 的契约，集成测试 SHOULD 使用唯一约束验证。

### 9.2 NULL 顺序

每个排序项内部规范化为：

```text
field : direction : nulls
```

M1 默认遵守 PostgreSQL 原生规则：ASC 为 `NULLS LAST`，DESC 为 `NULLS FIRST`。renderer SHOULD 显式输出 NULLS 子句，使 SQL 语义不依赖隐含规则，并为未来游标分页复用。

### 9.3 offset

page 模式的 offset 使用 checked arithmetic，并在计算后检查 `max_offset`。LIMIT 和 OFFSET 使用绑定值，不直接拼接用户字符串。

`size=0` 不执行 list SQL。

### 9.4 cursor

cursor 使用最终稳定排序执行词典序 keyset 比较，并显式处理 `ASC NULLS LAST`、`DESC NULLS FIRST`。第一页使用 `cursor=`，后续 token 由服务端 HMAC-SHA256 签名。list SQL 使用 keyset 条件且不生成 OFFSET；count 与 summaries 始终统计忽略 cursor 位置后的完整过滤结果。分组实体在当前 M2 返回参数错误。

服务端可对实际 SQL 表达式保证非空的排序字段声明 `#[search_field(sortable, cursor_non_null)]`。此声明使比较使用直接范围与等值条件，避免 NULL 分支阻碍索引 seek；NULL token 在编解码/计划阶段被拒绝。默认不声明，Rust 的非 Option 类型不足以证明外连接表达式非空。属性参与 schema fingerprint，修改后旧 cursor 失效。只有数据库非空约束在该查询表达式中仍成立时才能声明，库不运行 schema 探测。

## 10. API 操作语义

| API | 条件 | 排序 | 分页 | 投影 | list SQL | cluster SQL |
|---|---:|---:|---:|---:|---:|---:|
| `search` | 是 | 是 | 是 | 完整实体 | 是 | count |
| `search_with` | 是 | 是 | 是 | 完整实体 | 是 | count + summaries |
| `search_map` | 是 | 是 | 是 | 动态 | 是 | count |
| `search_map_with` | 是 | 是 | 是 | 动态 | 是 | count + summaries |
| `search_as` | 是 | 是 | 是 | 编译期静态投影 | 是 | count |
| `search_list` | 是 | 是 | 是 | 完整实体 | 是 | 否 |
| `search_stream` | 是 | 是 | 是 | 完整实体 | 流式 | 否 |
| `search_first` | 是 | 是 | 强制 limit 1、offset 0 | 完整实体 | 是 | 否 |
| `count` | 是 | 校验但不渲染 | 校验但不渲染 | 校验但不渲染 | 否 | count |
| `sum` | 是 | 校验但不渲染 | 校验但不渲染 | 校验但不渲染 | 否 | sum |

补充规则：

- `search_first` 收到显式 page、size 或 offset 时返回 `PagingNotAllowed`，禁止静默忽略。
- `count` 和 `sum` 可以接收与列表相同的 RawParams，所有参数仍需完成安全校验，但排序、分页和投影不进入 SQL。
- `size=0` 的 `search/search_map` 只执行 cluster SQL。
- `size=0` 的 `search_list` 返回空列表且不执行 SQL。
- `search_with/search_map_with` 的 count 和所有 summaries 尽可能合并为一条 cluster SQL。
- `search_stream` 逐行产生 `Result<E, PgSearchError>`，不执行 count；完整消费后恢复会话配置，提前丢弃或超时则关闭连接，不把活动语句返回池。

## 11. 列表与统计 SQL

### 11.1 普通查询

- list SQL 只选择最终列表投影和数据库语义必需的内部表达式。
- count SQL 不携带 ORDER BY、LIMIT 和 OFFSET。
- count 返回 PostgreSQL `count(*)` 的 i64。

### 11.2 DISTINCT

带 DISTINCT 时，count 统计最终投影去重后的行数：

```sql
SELECT count(*)
FROM (
    SELECT DISTINCT <final projection>
    FROM ...
    WHERE ...
) sq_count
```

### 11.3 GROUP BY

带 GROUP BY 时，total_count 是最终分组结果的行数，而不是原始输入行数。

只查询分组总数时，内层 SELECT 可以使用常量：

```sql
SELECT count(*)
FROM (
    SELECT 1
    FROM student_score
    WHERE ...
    GROUP BY course_id
    HAVING ...
) sq_count
```

请求分组聚合的外层汇总时，只把外层聚合依赖的表达式加入内层 SELECT。所有精简必须由 PostgreSQL 结果测试证明等价。

## 12. 聚合结果

### 12.1 total_count

`Page.total_count` 的含义是忽略分页后、应用最终投影与分组语义后的逻辑结果行数。

### 12.2 SUM

SUM 的输出类型由 PostgreSQL 聚合类型规则决定，不能假设等于字段类型：

| 字段表达式类型 | SUM 输出 Rust 类型 |
|---|---|
| `i32` / PostgreSQL integer | `i64` |
| `i64` / PostgreSQL bigint | Decimal 对应类型 |
| Decimal / PostgreSQL numeric | 相同 Decimal 类型 |
| `f32` / PostgreSQL real | `f32` |
| `f64` / PostgreSQL double precision | `f64` |

具体支持集合由 `PgSumType::Output` 表达，并由 PostgreSQL 集成测试锁定。没有映射的字段不能调用类型化 sum。

空输入的 SUM 返回 `None`，不隐式转换成零。

### 12.3 AVG、MIN 与 MAX

- PostgreSQL `avg(integer/bigint/numeric)` 返回 Decimal，`avg(real/double precision)` 返回 `f64`。
- `min/max` 保持输入字段类型；当前支持整数、浮点、Decimal、文本、日期时间和 UUID，不对 bool、bytes、JSON 开放。
- 不支持的字段/聚合组合在获取数据库连接前返回渲染错误。

### 12.4 多聚合

`SearchOptions<E>` 由服务端构建。多聚合结果按请求顺序返回，每项包含：

- 公开字段名。
- 聚合函数。
- 明确的结果类型。
- 可空结果值。

M2 的 `SearchOptions` 支持 SUM、AVG、MIN、MAX 和受控 `CustomAggregate`。自定义聚合只能由服务端静态模板创建，必须使用 `{field}`，并显式声明输出 `ValueType`；HTTP 参数不能请求聚合模板。

普通实体的 SUM 对 WHERE 过滤后的原始结果行聚合。分组实体的 SUM 对最终分组子查询的对应字段再次聚合，因此必须再次应用 PostgreSQL 类型提升规则。

每次查询最多请求 8 个 summaries，同一字段的同一聚合函数不能重复。HTTP 参数不能直接增加 summaries。

## 13. SQL 占位符与绑定

PostgreSQL renderer 直接生成 `$1`、`$2`，禁止先生成问号 SQL 再做字符串替换。

`RenderedPgQuery` 同时保存 SQL 和有序绑定值。标量与数组绑定是不同变体；数组必须携带元素类型。

所有运行时输入，包括 LIMIT 和 OFFSET，均通过 sqlx bind 传递。表、列、排序表达式和聚合表达式只能来自已验证的静态元信息。

受信任 SQL 属性仍属于应用配置，不是安全沙箱。M1 必须拒绝其中的 SiftQuery 占位符、语句终止符和多语句形式，数据库语法错误在启动验证或首次执行时映射为配置错误。

## 14. SQL 形状

相同实体 schema、最终投影、谓词结构、运算符、排序、分页模式、聚合请求和渲染策略必须生成相同 SQL 文本。

绑定值本身不进入 SQL 形状。InList 数组长度不改变 SQL 形状。

形状指纹至少包含：

- 后端和实体 schema 指纹。
- 最终投影 FieldId 列表。
- Row 与 Group AST 结构。
- 运算符和 ignore-case 模式。
- 最终排序及 NULL 顺序。
- 分页模式。
- 聚合请求。
- 会改变 SQL 的 QueryPolicy 指纹。

M2 可显式启用有界 Query Shape Cache。缓存键包含最终 SQL shape id，缓存值只包含 SQL 模板的共享分配，不包含绑定值或查询结果；完整 SQL 比较用于检测哈希碰撞。容量为 0 时关闭，非零容量采用 LRU 淘汰并公开 hit/miss/eviction/collision 统计。

## 15. 一致性

M1 默认 `SequentialBestEffort`：

- list 与 cluster 从同一个不可变 QueryPlan 派生。
- 两条 SQL 的条件和绑定逻辑一致。
- 两条语句可以看到不同数据库快照。
- 并发写入时允许列表与 total_count 短暂不一致。

M2 提供三种执行策略：

- `SequentialBestEffort`：默认值，同一连接顺序执行，但两条语句不保证同一快照。
- `ParallelBestEffort`：list 与 cluster 并发执行，受独立 semaphore 限制；任一分支失败仍等待另一分支完成，避免隐式取消留下活动查询。
- `Snapshot(RepeatableRead | Serializable)`：同一连接内使用只读显式事务，成功提交后归还池；任何分支或提交失败均关闭连接。

`WindowWhenPossible` 是显式 count 策略，仅用于无 summaries、非 DISTINCT、非分组、非 cursor、非 Snapshot 且 size 大于 0 的分页。结果非空时读取 `count(*) OVER()`；offset 导致空页时再执行独立 count，保证 `total_count` 正确。

## 16. 扩展钩子

执行顺序固定为：QueryPlan 完成 → 分区解析 → SQL 渲染 → SQL 拦截 → Shape Cache → 数据库执行 → cursor 提取 → RowMap 结果过滤。

- `PartitionResolver` 只能返回经过 ASCII 标识符校验并逐段引用的 schema/table/alias，不能返回任意 FROM SQL。
- `SqlInterceptor` 是服务端可信扩展，只接收占位符 SQL，不接收绑定值；修改后重新计算 shape id。
- `ResultFilter` 只作用于动态 `RowMap`，可修改或丢弃行。丢弃不会改写数据库 `total_count`，因此适合脱敏/展示过滤，不应替代 `ServerScope` 权限条件。
- 扩展错误发生在相应阶段并作为 `PgSearchError::Extension` 返回；错误文本不得包含绑定值。

## 17. 示例

### 17.1 普通查询

请求：

```text
status=1&status=2&status-op=il&username=Al&username-op=sw&sort=created_at&order=desc&page=1&size=20
```

规范化谓词：

```text
status IN [1, 2] AND username STARTS_WITH "Al"
```

最终排序：

```text
created_at DESC, id ASC
```

示意 SQL：

```sql
SELECT u.id AS "id", u.username AS "username", u.status AS "status"
FROM app_user u
WHERE u.deleted = false
  AND u.status = ANY($1)
  AND u.username LIKE $2 ESCAPE '\'
ORDER BY u.created_at DESC NULLS FIRST, u.id ASC NULLS LAST
LIMIT $3 OFFSET $4
```

绑定值按顺序为：类型化 `i32[] [1,2]`、文本 `Al%`、`20`、`0`。

### 17.2 非法跨阶段 OR

```text
A.course_id=1&B.total_score=100&B.total_score-op=gt&gexpr=A%7CB
```

`course_id` 是 GroupKey，规范阶段为 Row；`total_score` 是 Aggregate，阶段为 Group。表达式 `A|B` 返回 `MixedPredicatePhase`，不得执行 SQL。
