# HTTP 传输与查询模板缓存

更新时间：2026-10-10。

## GET / POST 的同一参数协议

`QueryParams` 保留仅读取 URL query 的原有入口。新增 `SearchParams`：GET 读取 query；POST 读取 UTF-8 `application/x-www-form-urlencoded` body。同一路由必须同时注册 GET 和 POST，三个 workspace 示例已采用这一方式。参数顺序、重复键、filter、Codec、entity 校验和权限 scope 的语义一致。

POST 携带非空 query 时返回 `400 conflicting_query_body`，不合并来源；其他 media type 或非 UTF-8 charset 返回 `415 unsupported_query_media_type`。读取 body 按实际累计字节检查，不信任 Content-Length；默认上限与 `QueryLimits.max_raw_query_bytes` 相同，为 64 KiB，默认读取 deadline 为 5 秒。超限返回 `413 query_body_too_large`，超时返回 `408 query_body_timeout`。

`SearchParamConfig { limits, body_timeout }` 可以安装为 axum `Extension`、actix 原始 app data、poem endpoint data。应用应复制其实际 query policy 的 limits，避免 extractor 与 parser 使用不同预算。query parser 的 `QueryTooLong` 映射为 `413 query_too_long`；POST 不提高查询预算，超限时必须减少参数。

```rust,ignore
// axum：SearchParams 是消费 body 的 extractor，应放在 handler 参数最后。
async fn users(State(search): State<PgSiftQuery>, params: SearchParams)
    -> Result<Json<Page<User>>, WebError>
{
    Ok(Json(search.search::<User>(params.raw(), &ServerScope::unrestricted()).await?))
}
let app = Router::new().route("/users", get(users).post(users));
```

## 前端编码和自动选择 method

`clients/sift-query.ts` 是没有运行时依赖的 TS helper。输入是有序参数对，重复键用多个条目表示。`encodeQuery` 与 Rust 的 `RawParams::to_query_string` 使用相同编码：UTF-8、空格 `%20`、字面加号 `%2B`，保留 `-_.~`。

```ts
import { search } from "./sift-query.ts";
const response = await search("/users", [
  ["id", "1"], ["id", "2"], ["id-op", "il"],
], { credentials: "include", signal });
```

helper 在发送前计算完整编码 URL 的 UTF-8 字节数，默认不超过 2000 字节使用 GET，否则发送 POST form。`getUrlBytes` 可按部署调整，2000 是保守阈值，不是浏览器或代理的通用硬上限。地址较长时，即使参数少也会走 POST。不会因 414 或其他错误重试；调用方的 headers、credentials 和 AbortSignal 保留。

`maxQueryBytes` 和 `maxRawItems` 可以复制服务端的有效预算，默认分别为 64 KiB 和 128，客户端超限不发送请求。字段、运算符、解码后单值和复杂度预算仍由服务端完整校验。helper 不要求完整 QueryBuilder RPC 编码器。

流式 CSV 下载可用 `downloadForm(endpoint, entries)`，通过隐藏 POST form 打开下载，避免 JS 将整个文件缓存在 Blob 中；只支持浏览器表单可提供的认证方式。需要自定义下载 headers 或异步导出时，由应用接入下载流或导出任务接口。

nginx 的请求行必须放进单个 header buffer；典型配置为 4 KiB/8 KiB，其他代理和部署可能更小。调大 header buffer 不能代替 POST 支持。服务端 64 KiB 查询预算也不是 HTTP 链路接受同等长度 URL 的保证。

## 渲染前模板缓存

启用 `PgExecutionPolicy.shape_cache_capacity` 后，`PgSiftQuery` 在基础 SQL 渲染前查询模板缓存。命中后复用 `Arc<str>` 和原有 QueryShapeId，仅执行绑定规则，从当前 QueryPlan 读取当前请求的值。覆盖 list、count/summaries、SUM、window count，以及使用这些路径的静态投影、外部连接和 scan。

键使用完整结构相等比较：Rust 实体类型与静态元信息身份、schema/policy seed、执行目标、语句类型、投影及别名、排序、分页模式、DISTINCT、四个 WHERE/HAVING predicate 的树结构、operator/ignore-case/占位符数量、summary 配置和 cursor NULL 形态。普通值、page/offset 数值和 scope 值不进入键；`InList`/`NotIn` 数组长度不改变 SQL 模板，`OrLike` 的值数量进入键。自定义 operator/aggregate 的完整配置参与比较，哈希碰撞不能产生错误命中。

缓存项只包含 SQL、Shape ID 和绑定位置/转换指令，不保存过滤值、cursor 值或查询结果。LIKE 转义、类型化数组、LIMIT/OFFSET 和重复 cursor 等值绑定均从当前请求重建。查询解析、预算、scope 合并和 QueryPlan 校验继续执行。

分区解析和 SQL 拦截器每次执行，发生在基础模板复用之后；其输出不进入此缓存。`shape_cache_hit` 表示基础模板命中，最终 `query_shape_id` 仍描述扩展后的 SQL。因此动态分区、拦截器拒绝和动态输出不会因命中而被跳过。

实例及其 clone 共享有界 LRU；查询渲染和重建 binds 均在锁外执行。LRU 使用安全索引链表，查找、提升和淘汰均为平均 O(1)，不在淘汰时扫描所有条目。已处于最近使用位置的模板命中使用并发读锁，其他命中用写锁提升，保持准确淘汰顺序。默认容量仍为 0，上限仍为 16384；重复查询形状较多时可从 256 开始评估，低命中率工作负载保留关闭选项。

`QueryShapeCache::render_list/render_cluster/render_sum/render_window_list` 返回 `(RenderedPgQuery, hit)`，可直接用于 renderer 基准；原有 `intern` 继续提供渲染后 SQL 分配复用，并与模板条目共用容量和统计。

## 减少重复工作与验证

无 RawParamFilter 时 parser 借用原始 RawParams；编码器直接写入同一 String，字节计数不再为每个参数创建编码字符串。字段名校验按 policy 实例及实体类型缓存，最多 256 项，clone 共享；不同自定义 ParamNames 的 policy 独立校验，手写实体依然校验。Codec 的运行时校验继续保留。

无分区和 SQL 拦截器时不再复制完整 SQL；SQL/投影 buffer 按计划预留空间，GROUP BY 直接遍历静态字段。动态投影和 QueryPlan 仍按当前请求构建。

```bash
node --test clients/sift-query.test.ts
cargo test -p sift-query-sqlx --all-features --test web_transport --test template_cache
cargo bench -p sift-query-sqlx --bench core_pipeline -- --sample-size 20 --warm-up-time 1 --measurement-time 2
```

基准分别覆盖 parse/plan/render、单条件与 20 条件直接渲染、256 容量的稳定命中与冷启动、256 种形状污染 16 容量缓存、4 线程共享缓存，以及本轮新增的实体静态校验冷热路径、Codec 绑定冷热路径、MySQL/SQLite portable renderer 命中与高基数形状、CSV 逐行写出和 64 KiB 分块写出。并发基准包含线程创建和 join，不把该数据描述为数据库 QPS 或生产吞吐保证。正式性能结论应使用目标部署硬件和真实形状分布。

2026-10-11 的短采样（aarch64-apple-darwin，release，10 samples，warmup 0.2 秒，measurement 0.5 秒）记录了本轮新增路径：

| 场景 | 点估计 |
|---|---:|
| 静态校验 warm | 1.19 µs |
| 静态校验 cold | 2.15 µs |
| Codec 绑定 warm | 15.4 ns |
| Codec 绑定 cold | 1.49 µs |
| MySQL portable template 命中 | 458 ns |
| SQLite portable template 命中 | 440 ns |
| MySQL portable 高基数形状 | 685 ns |
| SQLite portable 高基数形状 | 707 ns |
| CSV 逐行写出（512 行） | 52.7 µs |
| CSV 分块写出（512 行） | 52.8 µs |

CSV 基准使用计数 writer 验证了逐行路径产生 512 次写调用，分块路径低于逐行次数；当前短样本中 CPU 格式化成本占主导，因此耗时接近。portable 高基数形状包含 512 个结构，故意超过默认 256 项缓存容量，用于观察低命中成本。

## 本机基准结果

2026-10-10，在 `aarch64-apple-darwin`、Rust 1.99.0、release 构建下，使用上面的 Criterion 命令测得以下点估计。直接渲染和缓存结果来自同一轮运行；稳定命中与冷启动使用容量 256。

| 场景 | 直接渲染 | 模板缓存 | 条件 |
|---|---:|---:|---|
| 20 条件 parse + plan + render | 21.40 µs | 17.24 µs | 稳定命中，仍解析和校验当前请求 |
| 单条件纯渲染 | 0.71 µs | 0.28 µs | 稳定命中 |
| 20 条件纯渲染 | 4.48 µs | 0.90 µs | 稳定命中 |
| 单条件纯渲染 | 0.71 µs | 1.31 µs | 每次冷启动 |
| 20 条件纯渲染 | 4.48 µs | 5.54 µs | 每次冷启动 |
| 256 种形状循环 | 3.48 µs | 5.82 µs | 容量 16，持续淘汰 |
| 4 线程合计 400 次纯渲染 | 620.53 µs | 181.93 µs | 同一形状稳定命中，含线程创建与 join |

这些是本机 CPU 路径的 smoke 基准，没有数据库或网络耗时。缓存命中能减少渲染工作，但冷启动和低命中率会增加成本，因此默认容量保持 0；应结合真实形状分布与 `ShapeCacheStats` 决定是否开启。

## P1/P2 增量性能复核

上表保留早期测量，不作为跨运行时刻的回归比较。2026-10-10，在 Apple M3 Pro（12 核、36 GB）、同一工具链/release/all-features 配置下，顺序运行本轮变更前的打包源码与当前实现，Criterion 各 warmup 0.5 秒、measurement 1 秒、20 samples；没有同时运行编译或 fuzz。

| CPU 场景 | 本轮前 | 本轮后 | 点估计变化 |
|---|---:|---:|---:|
| 20 条件 parse/plan/render | 28.138 µs | 28.997 µs | +3.1% |
| 20 条件 parse/plan/cache hit | 23.120 µs | 24.092 µs | +4.2% |
| 单条件直接渲染 | 0.929 µs | 0.957 µs | +3.1% |
| 单条件 cache hit | 0.395 µs | 0.384 µs | -2.7% |
| 20 条件直接渲染 | 5.783 µs | 6.102 µs | +5.5% |
| 20 条件 cache hit | 1.208 µs | 1.318 µs | +9.1% |
| 高基数容量 16 | 7.526 µs | 8.079 µs | +7.3% |

JSON/range/network 输入用间接存储保持常用 ScalarValue 布局，减少普通谓词携带结构化值的内存成本。仍存在新增类型/校验带来的小幅 CPU 成本，不能宣称零损耗；短基准含调度噪声，并发场景波动更大。功能按 feature 隔离，普通查询没有额外 SQL/事务/能力探测；RLS 的事务、扩展谓词的索引需求和计算排序的成本按需承担。

可复现数据库工作负载：在专用可丢弃 DATABASE_URL 下运行 `cargo bench -p sift-query-sqlx --bench postgres_workload`。它创建专用 sift_benchmark schema，50,000 行均匀十组数据、PK(id)/B-tree(status,id)，单客户端、pool=4、warmup=3、samples=20，比较 sequential/parallel/两档 snapshot/window、offset/keyset 深分页及 10,000 行流，并对实际渲染 SQL 与绑定值执行 EXPLAIN (ANALYZE, BUFFERS)。基准显式提升自己的 offset 预算至 40,000；普通查询默认预算不变。成功/查询失败均清理本次 schema；schema 已存在时拒绝覆盖。异常进程终止后的 schema 由调用方清理。数据库版本、数据量、索引、并发与分布随输出记录，硬件需调用方附记。

同一 M3 Pro / PostgreSQL 17.11 本地运行的中位数与 P95（均含客户端管线，分页含完整 count）：

| 场景 | 中位数 | P95 |
|---|---:|---:|
| Sequential / Separate count，status=1，50 行 | 0.944 ms | 1.066 ms |
| Parallel，同一条件 | 0.701 ms | 0.803 ms |
| Repeatable Read Snapshot | 1.042 ms | 1.119 ms |
| Serializable Snapshot | 1.100 ms | 1.400 ms |
| Window count | 1.446 ms | 1.505 ms |
| OFFSET 40,000，50 行 | 3.611 ms | 3.763 ms |
| Cursor，未声明键非空 | 4.127 ms | 4.322 ms |
| Cursor，显式 cursor_non_null | 1.724 ms | 1.738 ms |
| 流式消费 10,000 行 | 5.385 ms | 5.442 ms |

未声明键非空时，`id > $1 OR id IS NULL` 保留 NULL 排序语义，这组数据的计划过滤前 40,000 行、命中 328 个 shared buffers。新 `cursor_non_null` 声明只用于由数据库保证非空的排序表达式；范围条件成为 Index Cond，list 计划命中 3 个 shared buffers，执行约 0.013 ms。库拒绝该声明下的 NULL token，不根据非 Option Rust 类型自动推断。修改声明进入 schema fingerprint，旧 token 失效；未声明的 nullable 行为继续保留。完整分页仍承担全量 count，不能把 list 的 0.013 ms 宣称为整页延迟。

此负载中 window count 更慢，parallel 更快；二者均不能泛化到其他数据分布/连接池/部署。默认仍为 Sequential + Separate，扩展能力与缓存按实际负载选择。

本地数据库 smoke 与 CPU smoke 都不替代目标硬件上的负载、峰值内存、取消成本和 SLO 门禁。

## 源码重组与共享算法抽查

同日、同一机器，使用重组前保留的 all-features release 基准二进制与重组后的同配置代码顺序对比；warmup 1 秒、measurement 2 秒、40 samples。测量期间没有并行编译或 fuzz。场景为默认 policy、无参数 filter 的 20 条件查询；覆盖通用预算计数和 SQL 模板缓存，不覆盖 Label TTL 路径。

| CPU 场景 | 重组前 | 重组后 |
|---|---:|---:|
| 20 条件 parse/plan/render | 32.990 µs | 29.086 µs |
| 20 条件 parse/plan/cache hit | 24.775 µs | 23.778 µs |
| 20 条件纯 cache hit | 1.3007 µs | 1.3161 µs |
| 高基数容量 16 | 7.4353 µs | 7.3175 µs |

首项前测区间较宽（31.535–34.512 µs），不能据此宣称确定的加速收益。此次抽查未发现明显退化，后两项 Criterion 判为 noise threshold 内变化；它不是全部负载的性能保证。目录拆分本身不增加执行步骤；计数器无分配，TTL 命中保留单次哈希查找，LRU 删除以空槽复用保持存储有界。TTL 惰性清理与空槽索引仍有状态及操作成本，不承诺零损耗。
