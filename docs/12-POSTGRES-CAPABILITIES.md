# PostgreSQL 原生查询能力

这些能力是默认关闭的增量功能。公共 core 不依赖 sqlx；PG 后端只引入 sqlx-postgres。普通 query 不执行能力探测，不要求事务。安装、发布和隔离合同见 [11-RELEASE-AND-BACKENDS.md](./11-RELEASE-AND-BACKENDS.md)。

## Feature 与初始化

| Feature | 能力 | 额外依赖/要求 |
|---|---|---|
| `postgres-search` | `postgres` 模块、能力目录、全文查询 | PG 执行后端 |
| `json` + `postgres-search` | JSONB contains/contained-by/key | serde_json 精确数字 |
| `postgres-types` | bigint[]/text[]、int8range、inet/cidr | ipnet、JSON；PG 原生 bind/decode |
| `pg-trgm` | 相似过滤与受控距离排序 | 数据库 pg_trgm |
| `ltree` | ancestor/descendant、路径 Codec | 数据库 ltree |
| `postgis` | intersects/contains/within、EWKT Codec | wkt 解析器、数据库 PostGIS |

`PgCapabilities` 的 `with_operator` / `with_full_text` 声明服务端能力，`apply(policy)` 注册到 QueryPolicy；字段仍必须通过 `custom_operators` 显式允许名称。`require(Extension::...)` 声明数据库要求，应用启动时显式调用 `check(pool).await`，最多等待 5 秒。库不自动安装 extension；检查同时验证扩展 schema 在 search_path 中。生产池的角色、search_path 和数据库配置应与检查时一致。

`CustomOperator::typed` 明确字段类型与输入类型，例如 JSONB 字段的 `json_key` 接收 Text，range 的 `range_element` 接收 I64。HTTP、builder、FilterExpr、ServerScope 和 RPC 统一验证类型、arity、阶段和预算。程序化异类型输入使用 `custom_values`；SQL 给绑定值加明确 cast，防止 PostgreSQL 重载运算符的 prepared statement 参数类型混用。能力、输入类型和计算排序配置进入 fingerprint/cache；缓存仍重新绑定当前值。

完整可执行声明和 HTTP/builder/RPC/缓存等价测试见 [native_postgres.rs](../crates/sift-query-sqlx/tests/native_postgres.rs)。facade 包内 README 提供经过 doctest 的全文接入例子。

## 全文与 JSONB

全文字段使用 `FilterOnly<String>` 指向服务端声明的 tsvector 列或表达式，配合 `#[sqlx(skip)]`、`selectable=false`。此类型用于查询输入，不承诺 tsvector 原生结果解码。`TextConfig` 首版为 Simple、English、German、French；配置由服务端固定。

| HTTP 运算符 | PG 函数/语义 |
|---|---|
| `fts_plain` | plainto_tsquery |
| `fts_phrase` | phraseto_tsquery |
| `fts_web` | websearch_to_tsquery |
| `fts_query` | to_tsquery；非法语法由 PG 返回查询错误 |
| `json_contains` / `json_contained_by` | JSONB `@>` / `<@`，结构化 JSON 输入 |
| `json_key` | `?`，一个文本 key |
| `json_any_keys` / `json_all_keys` | `?|` / `?&`，text[] 输入，需 postgres-types |

全文文本始终作为参数绑定。空全文查询不匹配；首版只提供过滤，rank/headline 留待独立设计。JSONB 使用单个 JSON 字符串作为 HTTP 值，启用 arbitrary_precision 防止大整数 RPC 往返丢失精度。JSON null 是 JSON 值；SQL NULL 使用 `nl/nn`；key 不存在与 key 对应 JSON null 的区别由 `json_key` 判断。

命名 JSON 标量字段可声明静态表达式，例如测试中的 city 映射 `i.payload ->> 'city'`。请求只使用公开名 city，不能提交任意 path、cast 或函数。

## 原生值编码

| 类型 | HTTP 单值示例 | 结果/合同 |
|---|---|---|
| bigint[] | `[1,2,null]`、`[]` | Vec<i64> / Vec<Option<i64>>；RowValue::ArrayI64 |
| text[] | `["rust",null]` | Vec<String> / Vec<Option<String>>；RowValue::ArrayText |
| int8range | `[1,5)`、`(1,5]`、`empty`、`(,)` | PgInt8Range；离散边界规范化为 `[lower,upper)` |
| inet/cidr | `10.0.0.1/24`、`::1/128` | ipnet::IpNet；IPv4/IPv6 与网络掩码 |

SQL NULL 列与空数组、含 NULL 元素的数组不同；HTTP 列表本身是一个绑定参数，不增加 SQL 占位符数量。元素数量、JSON 节点数量仍消耗逻辑预算。标量列 `InList` 的 ANY 绑定保持原合同，与数据库 array 运算符分开。

数组提供 contains/contained-by/overlap；bigint[] 名称为 `array_contains` 等，text[] 为 `array_text_contains` 等，两组可共存同一 registry。range 提供 contains/contains-element/contained-by/overlap/left/right/adjacent；network 提供 strict/inclusive contains、contained-by 和 overlap。

`PgInt8Range` 保留 empty 与无界 `(,)` 的区别，不直接使用会丢失该区别的 sqlx PgRange 结果。开闭边界通过 Int8Range::new / FromStr 规范化，逆区间、溢出和手工非规范 Bounds 被拒绝。首版不覆盖其他 array 元素类型、其他 range/multirange 类型。JSON、array、range、network 不支持 cursor 排序，避免给不可比较结构化值签发 token。

## 可选扩展与排序

pg_trgm 的 `trgm_similar` 使用 `%`；阈值来自数据库配置，库不会在每次查询中 SET 会话阈值。相似排序通过一个静态 FilterOnly 浮点字段声明 `sort_from`、`sort_operator`、`sort_template="({field} <-> {0})"`，请求用 `orderBy=distance:asc`。排序引用唯一一条对应过滤条件的绑定值，并追加实体稳定键；缺失/重复输入、grouped、distinct 和 cursor 组合被拒绝。计算排序不开放客户端 SQL。

ltree 字段绑定 `codec="ltree"`，policy 注册 `LtreeCodec::new("path")`。PostGIS 字段绑定 `codec="ewkt"`，policy 注册 `EwktCodec::new("geom", 4326)`；验证 SRID、WKT 结构和有限坐标。field 名称限定 Codec 适用范围，配置进入 fingerprint。两者目前使用 FilterOnly<String>，提供查询能力，不承诺原生 ltree/geometry 结果解码。SRID/有效几何和数据库约束仍需与应用模型一致。

## 索引与性能

全文 tsvector 优先评估 GIN，表达式索引的配置必须与查询相同。JSONB 的 jsonb_ops 支持 containment 与 key existence；jsonb_path_ops 适合 containment，但不支持 `?/?|/?&`，应根据实际条件选择。命名标量提取字段可使用对应表达式 B-tree。

array contains/overlap 可用 GIN；range 用 GiST/SP-GiST；网络关系按工作负载评估 inet_ops 的 GiST/SP-GiST；相等查询可用 B-tree。pg_trgm 过滤可用 GIN/GiST，`<->` KNN 排序评估 GiST。ltree 和 geometry 通常使用各自 GiST operator class。

库不创建索引，也不保证优化器总选索引。真实测试验证参数化查询语义和可索引形状；上线前用目标数据分布执行 EXPLAIN (ANALYZE, BUFFERS)，评估选择率、分页和 count 成本。启用 feature 本身不会额外访问数据库，但新谓词和排序的数据库成本取决于索引及输入。
