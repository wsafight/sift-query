# P0 业务扩展与执行入口

更新时间：2026-10-11。本文记录已交付的四组能力及其使用合同；不表示 GA 发布门禁已经完成。

## 1. 参数过滤、Codec 与 GroupScope

参数处理顺序为：原始预算检查 → 注册顺序执行 RawParamFilter → 规范化预算检查 → 严格 parser → QueryPlan → renderer。过滤器不接收 SQL、连接或权限 scope，业务默认值不替代权限条件。

| 接口 | 合同 |
|---|---|
| `IndexedArrayParamFilter` | 把重复 `field[]` 或显式 `field[0]` 转为标准索引；重复索引、混合冲突和非连续索引由严格 parser 拒绝 |
| `JsonArrayParamFilter` | `json` feature 下展开非空标量数组；拒绝 malformed JSON、null、对象和嵌套数组 |
| `SuffixOperatorParamFilter` | `name-ct=x`、`name-ct-ic=x` 转为标准参数；同一后缀保留重复值、冲突后缀报错，零参数运算符使用 `id-nl=`；不吞掉不认识的后缀 |
| `DefaultParamFilter` | 仅补入调用方未提供的同名参数；仍保留原有重复值与顺序 |
| `QueryLimits` | 同时限制原始字节、解码字节、过滤步骤、输出数量和字节、`max_filter_micros` |

过滤器是受信任同步扩展；时间预算在每一步返回后检查，无法抢占陷入死循环的任意 Rust 代码。内置步骤的工作量由输入/输出预算限制。自定义步骤必须有界、无数据库调用，并自行把配置写入 `fingerprint()`。过滤链顺序和默认参数配置参与 policy fingerprint。

`Codec` 统一定义输入转换和动态结果转换。注册顺序不能解决同一字段的歧义：重复名称、缺失显式绑定和不兼容绑定均在执行前报错。`#[search_field(codec = "status")]` 通过 `SearchEntity::codec_name` 绑定名称，绑定进入 schema fingerprint；codec 的配置 fingerprint 进入 policy fingerprint。

内置 `EnumCodec` 将稳定枚举名称与 `i32` 数据库代码双向映射，未知名称/代码和空字符串报错；`BooleanCodec` 提供严格业务布尔名称；`DateCodec` 在 `chrono` feature 下提供日期格式，没有隐式时区。未配置 Codec 的 timestamp 保持 RFC 3339/UTC 合同。新增时间/时区/列表 Codec、`format_input` 与 RPC 表示见 [10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md](./10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md)。数组规范化与单值 codec 可组合使用。

程序化 builder 和 cursor 继续使用字段的内部数据库值；codec 只转换外部输入和 RowMap 展示值，不改变 sqlx 类型化 `FromRow`。动态 cursor 在结果 codec、ResultFilter 和 enrichment 之前生成，避免展示转换改变游标键。未注册 codec 的标准 serde 表示保持原有合同；CSV 默认格式使用 `RowValue::to_external_text`。

`GroupScope<E>` 只接受类型匹配的 Aggregate 字段，拥有独立 `ScopeLimits`，可以 AND/OR 组合服务端条件。客户端条件与它始终在外围通过 AND 合并：

```text
WHERE  = base_where AND ServerScope AND user_row_predicate
HAVING = fixed_having AND GroupScope AND user_group_predicate
```

`QueryPlan::new_with_scopes`、`search_with_group_scope`、`search_map_with_group_scope` 及带 options 的 `search_typed_with_scopes` / `search_map_with_scopes` 提供入口。普通非分组实体不能把标量字段伪装成 Aggregate。分组 count、summaries 和 DISTINCT 共享这套 HAVING 语义。

## 2. scan、CSV 与 Web

`scan_stream` / `scan_map_stream` 使用单条 PostgreSQL 查询的 statement snapshot，强制稳定排序，拒绝显式 page、size、offset、cursor。分组实体也使用单查询流；首版没有 cursor 分批模式。

`ScanPolicy` 是服务端专用策略：`max_rows`、`batch_size`、`timeout` 均须有限且有效。批大小最大 16384，控制协作式交付；扫描 deadline 独立于普通分页超时，包含等待并发名额和读取。默认每个 searcher 有 4 个活动 scan 名额、16 个等待名额，克隆实例共享预算，可通过 `with_scan_concurrency` 设置。

扫描 SQL 最多读取 `max_rows + 1` 行作为超限检测，流最多交付 `max_rows` 行；额外一行触发 `ScanLimitExceeded`。达到准确行数且没有额外记录时成功。提前 drop、客户端断开、输出错误和超时会关闭仍在读取的连接；正常消费恢复 statement timeout 后归还连接。

`scan` / `scan_map` 是显式收集为 Vec 的便利入口，内存随设定行数增长。需要有界流式内存时使用 stream 入口。流和 CSV 的内存随当前行及设定批大小增长，而不是随全部结果行数增长。

`CsvExporter` 直接消费 scan 流。`CsvColumn` 声明可导出字段、标题、顺序及服务端 formatter；空列配置使用实体的 selectable 投影。`CsvColumn::label` 显式声明来源字段的增强标签列。CSV 输出 UTF-8，处理引号、分隔符、换行，并为公式前缀及前导控制字符加保护。

导出顺序固定为：RowMap decode → result codec → 每批 ResultFilter → PageEnricher/Label → formatter → CSV escaping → AsyncWrite。批大小和整次导出 deadline 由 `CsvExportPolicy` 控制；输出 writer 本身的背压也在 deadline 内。任何查询、增强或写出失败都传播为错误；已经写出的数据可能不完整，应用不得在该错误后把导出标记为成功。tracing 记录失败类别和已写行数。

| Feature | 可复用入口 |
|---|---|
| `axum` | `QueryParams: FromRequestParts`，`WebError: IntoResponse` |
| `actix-web` | `QueryParams: FromRequest`，`WebError: ResponseError` |
| `poem` | `QueryParams: FromRequest`，`WebError: IntoResponse`，可转换为 poem error |

三个 adapter 都从原始 query 解析，不经过单值 Map，保留顺序和重复参数。默认 extractor 使用原始参数的库预算；自定义预算可以通过 `QueryParams::parse_with_limits` 接入应用自己的 extractor。

新增 `SearchParams` 支持同一路由 GET query / POST form body，三个示例已更新。它使用有限 body 大小及读取 deadline，POST 不接受同时提供 query，不提高 core 预算；配置和前端自动切换见 [09-TRANSPORT-AND-PERFORMANCE.md](./09-TRANSPORT-AND-PERFORMANCE.md)。`QueryParams` 保留原有只读 query 的入口。

`public_error` 提供可覆盖的默认映射：查询字节超限 413、其他参数错误 400、查询/增强超时 504、scan 行预算超限 413、其他错误 500。公开 JSON 仅包含稳定错误码，默认不包含 SQL、表达式、绑定值或 cursor。三个 workspace 示例已经实际使用正式 adapter。

## 3. PageEnricher、Label 与观测

`PageEnricher` 按注册顺序异步接收整页 RowMap。库检查返回行数、total_count、summaries 和 next_cursor 没有被改写；每个增强步骤有 5 秒保护。`ResultFilter` 保持同步逐行展示过滤职责。

`TypedPageEnricher<T>` 是显式 opt-in 的类型化增强 trait；`search_enriched` 使用该入口，保留分页元数据，并要求行数不变。`LabelTarget` 显式声明 label ID 的读取和增强字段的写入，`LabelEnricher` 同时实现 RowMap 和类型化路径。实现者必须保持数据库字段、排序键和行顺序不变。

Label 按字典 key 汇总所有绑定字段的 ID，同页同 key 只调用一次批量 loader。默认总 ID 预算 512、每批 loader timeout 2 秒、共享活动加载并发 4，可配置。提供 `DictionaryLabelLoader` 以及供远程服务实现的 `LabelLoader`。

缺失行为可选 Omit、Null、Error；loader 失败可选 Error 或 KeepRows。已有 label 目标字段、重复 source 绑定、模糊 loader、重复/未请求的返回 ID 会明确报错。错误 batch 不进入缓存。缓存默认关闭；启用后为实例及其克隆共享的有界 LRU/TTL 正结果缓存，按实体、字典 key、ID 隔离。不同租户/语言的字典应使用不同 enricher 实例；依赖请求上下文的远程 loader 默认关闭缓存。

公开查询、外部执行和流操作均产生 `sift_query` span；组合操作可包含子操作 span。流的 span 保持到消费/取消结束。默认字段只有低敏感度元数据，`QueryObserver` 提供独立于 Prometheus/OpenTelemetry SDK 的完成回调，包含耗时、返回行数、结果类别和 slow 标记。observer panic 不影响查询结果；回调应非阻塞。

实例阈值使用 `with_slow_query_threshold`；实体阈值使用 `with_entity_slow_query_threshold`，优先于实例值。默认不输出 SQL 全文、绑定值或用户参数。

| 字段/事件 | 实际测量含义 |
|---|---|
| `entity/backend/operation` | 实体、postgres、公开操作名 |
| `execution_strategy/count_strategy` | 实例执行与 count 策略 |
| `query_shape_id/shape_cache_hit` | SQL 形状及模板 cache 命中；不作为默认指标标签 |
| `parse_duration_us/plan_duration_us/render_duration_us` | 参数处理、计划构建和渲染事件 |
| `pool_wait_duration_us` | 成功显式获取连接的等待耗时；外部连接入口无 pool wait |
| `list_duration_us/cluster_duration_us` | SQL 执行、读取及连接会话清理；类型化查询含 sqlx 内部 decode |
| `decode_duration_us` | 仅在 RowMap/summary 的独立转换阶段可以可靠测量时记录 |
| `duration_us/rows_returned/outcome` | 全操作耗时、行数，以及 success/cancelled/parameter/timeout/database/decode/extension/budget/output/configuration |
| `rows_written` | CSV 成功写出完整批次后的累计行数 |

不能可靠拆分的网络/读取/sqlx decode 耗时不伪造为独立指标。流的全操作耗时包括消费方背压。参数错误的 span 会结束，但不会产生连接获取或 SQL 执行事件。

## 4. 外部 connection / transaction

普通 `search` 继续由 `PgPool` 管理连接，不要求事务。高级入口 `search_on` / `search_map_on` 接受借用的 `PgConnection`；传入 sqlx Transaction 的可变解引用即可参与已有事务。`search_on_with` / `search_map_on_with` 同时支持 GroupScope 和 summaries。

库不 begin、commit、rollback、归还外部连接，也不设置或重置外部连接的 statement timeout、isolation 或其他 session 配置。外部模式保留客户端等待 deadline，只支持 SequentialBestEffort，Parallel 和 Snapshot 在 SQL 执行前拒绝；count 使用顺序独立语句，忽略实例的窗口优化配置。

调用方负责设置服务器端 statement timeout。数据库错误可能使事务进入 aborted 状态，调用方必须回滚；客户端 deadline/未来取消后，不承诺借用连接可立即复用，调用方应关闭或丢弃连接，或按 sqlx/数据库合同完成恢复。库不会为恢复连接而擅自结束调用方事务。

## 5. Schema 合同、权限策略与 Facet

PostgreSQL 后端提供 `PgSiftQuery::validate_schema::<E>()`，通过 `EXPLAIN (FORMAT JSON)` 让数据库只计划、不执行实体的完整动态投影。它会检查 `from`、固定条件、字段表达式、别名、排序、分页和分组语法，适合在服务启动或 CI 中显式调用；它不读取 `information_schema`，也不自动扫描业务表。

`ScopeProvider<C, E>` 从认证后的可信上下文构造 `ServerScope<E>`。`search_authorized`、`search_map_authorized` 和 `count_authorized` 会先生成 Scope，再进入普通计划校验和 SQL 执行路径；查询参数不会传入 provider。PostgreSQL、MySQL/MariaDB 和 SQLite 后端都提供这些授权入口。

`FacetRequest::new(Field, limit)` 声明一个服务端 facet。`PgSiftQuery::facets` 和 `search_map_with_facets` 只接受可选择、可排序的实体字段，最多 8 个 facet、每个最多 1000 个桶，返回每个值的计数和未返回桶的 `other_count`。Facet 重用相同的用户查询和 `ServerScope`，不接受列名、表达式或 SQL 输入。

## 6. 验证与剩余边界

协议、业务扩展和观测测试位于 core 的 `tests/extensions.rs`、sqlx 的 `tests/products.rs`、`tests/observability.rs`；真实数据库用例位于 `tests/p0_postgres.rs`，覆盖 GroupScope/统计/DISTINCT、Codec、两种 count 策略的 enrichment、scan 超限/取消/并发、CSV 与 Label、外部写入可见性、事务所有权和 statement timeout。

上述四组能力已实现，后续又补齐 HTTP 自定义运算符、完整 QueryBuilder RPC、映射复用、多 pool 路由和完整时间/简单列表 Codec，合同见 [10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md](./10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md)。发布依赖环、元数据、远端 CI、fuzz 和正式性能基线的状态见 [06-STATUS.md](./06-STATUS.md) 与 [14-PERFORMANCE-BASELINE.md](./14-PERFORMANCE-BASELINE.md)。
