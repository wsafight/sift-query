# Bean Searcher 产品能力补齐

更新时间：2026-10-10。参考源码版本为 Bean Searcher 4.8.12。

本轮完成此前产品排序中的 1、2、6、7、8，对应路线图 P0.11、P0.4、P0.12、P0.7、P0.3。这里定义 Rust 入口及其与 Java 的对应关系；默认严格协议继续生效。

| 产品功能 | Java 参考 | SiftQuery 入口 |
|---|---|---|
| QueryBuilder RPC | [`MapBuilder.buildForRpc` / `RpcNames`](../reference/bean-searcher-dev/bean-searcher-doc/docs/zh/guide/usage/rpc.md) | `build_for_rpc` / `UserQuery::to_rpc` |
| HTTP 自定义运算符 | [`FieldOp` / `FieldOpPool`](../reference/bean-searcher-dev/bean-searcher-doc/docs/zh/guide/advance/fieldop.md)、`DbField.onlyOn` | `QueryPolicy::with_custom_operator` + 字段命名白名单 |
| 映射复用与命名 | [继承映射](../reference/bean-searcher-dev/bean-searcher-doc/docs/zh/guide/bean/inherit.md)、[ORM 注解与命名](../reference/bean-searcher-dev/bean-searcher-doc/docs/zh/guide/bean/aignore.md) | `search_entities!` 字段片段 / 命名预设、derive 命名选项、显式 sqlx alias |
| 命名与动态数据源 | [`NamedDataSource` / Spring 路由数据源](../reference/bean-searcher-dev/bean-searcher-doc/docs/zh/guide/advance/datasource.md) | `PgPoolRouter` + `PgPoolSelector<C>` |
| 时间与列表转换 | [日期时间、时区、`ListFieldConvertor`](../reference/bean-searcher-dev/bean-searcher-doc/docs/zh/guide/advance/convertor.md) | 时间 Codec / `TimestampCodec` / `ListResultCodec` |

## 1. 完整查询的 RPC 编码

```rust
let params = QueryBuilder::<User>::new(&policy)
    .in_list(UserFields::ID, [1_i64, 2])
    .page(2, 20)
    .build_for_rpc()?;
let encoded = params.to_query_string();
// 接收端仍完整解析，不跳过字段、运算符或复杂度校验。
let decoded = ParamParser::new(&policy)?.parse::<User>(&params)?;
```

已构建的查询使用 `query.to_rpc(&policy)`；远端控制参数名不同，使用 `to_rpc_with_names(&policy, &names)` 或 builder 的 `build_for_rpc_with_names(&names)`。接收端用同样的 `ParamNames` 创建 policy。`QueryPolicy::with_param_names` 会验证名称、重建控制名检查缓存并更新 fingerprint。

编码保留嵌套 AND/OR、重复条件值、零参数运算符、ignore-case、动态投影、最终稳定排序和 page/offset/cursor。条件使用独立 `qN` 分组及 `gexpr`；输出顺序稳定，控制项按 projection/order/size/paging 排列。排序只发送能重建最终顺序的最短显式前缀，追加的稳定键不占显式排序预算。签名 cursor 使用 policy 的 cursor codec 重新编码；两个服务须使用兼容的 schema、排序和密钥。

编码前验证查询的 schema/policy fingerprint，编码后再用目标参数名的完整 parser 验证语义往返。AST 的相同 AND/OR 可以结合，节点编号不作为远程合同。Bean Searcher 兼容策略的 forced-empty 查询仍返回空结果；默认严格策略不允许越界分页。

Codec 的 `format_input` 定义规范 RPC 文本，编码器检查 `parse(format_input(value))` 等于原内部值。默认使用结果表示；有展示精度损失的 Codec 必须提供独立的无损输入表示。内置时间 Codec 已处理此问题。枚举/业务布尔值使用业务名称，列表结果 Codec 保留原数据库文本。

RPC 仍受 RawParams 和 parser 的全部预算限制。程序化查询若超过传输预算、使用未允许的 HTTP 运算符，或参数 filter 改写规范值，编码会返回错误。发送方与接收方须约定 Codec 与运算符配置；RPC 参数中不携带 SQL 模板。此接口只接受 `UserQuery`，服务端 Scope、权限上下文和数据源没有序列化入口。

## 2. HTTP 自定义运算符

```rust
#[derive(SearchEntity)]
#[search_entity(from = "app_user u", stable_order = "id:asc")]
struct User {
    #[search_field(column = "u.id", sortable)]
    id: i64,
    #[search_field(column = "u.status", custom_operators = ["statusAbove"])]
    status: i32,
}
let above = CustomOperator::new("statusAbove", "{field} > {0}", 1)?;
let policy = QueryPolicy::default().with_custom_operator(above, ValueType::I32)?;
// status=1&status-op=statusAbove
```

注册名称必须是最多 64 字节的合法 ASCII 名称，不能覆盖任一内置名称或重复注册；最多 64 项。安全模板和 arity 延续 `CustomOperator` 校验，所有值仍通过 PostgreSQL bind 绑定。

`custom_operators = ["name"]` 经 derive 生成 `SearchEntity::custom_operator_names`，参与 schema fingerprint。注册名称、模板、arity、类型参与 policy fingerprint。字段声明的每个名称须注册、类型匹配且字段可过滤；配置错误在获取连接前失败。HTTP 还逐条件检查白名单、参数数量、Codec 返回类型和 ignore-case。

程序化受信任 `CustomOperator` 继续可直接使用。只有同时注册且被字段允许的运算符才能输出为 RPC。Java 对部分不允许的运算符会回退；Rust 保留明确报错，避免请求意图被静默改变。

## 3. 编译期映射复用

```rust
search_entities! {
    fields Identity {
        #[search_field(sortable)]
        id: i64,
    }
    fields Common {
        include Identity;
        status: i32,
    }
    mapping Users {
        #[search_entity(from = "app_user u", column_prefix = "u.", stable_order = "id:asc")]
    }
    #[derive(Debug, SearchEntity, sqlx::FromRow)]
    #[search_mapping(Users)]
    pub struct User { include Common; name: String }
    #[derive(Debug, SearchEntity, sqlx::FromRow)]
    #[search_mapping(Users)]
    pub struct UserDto {
        include Identity;
        #[search_field(column = "upper(u.name)")]
        display: String
    }
}
```

字段片段和预设在同一个宏调用内声明；支持前向引用与片段嵌套，最多 16 层，循环和未知引用在编译期报错。宏接受非泛型、具名字段 struct。片段展开后仍走普通 derive 的字段名/alias/排序/SQL 校验及 fingerprint 计算，不建立运行时元信息缓存。

命名预设只含 `search_entity` 属性。实体的显式属性覆盖预设；字段的显式 `column` 覆盖列名默认值。可配置：

| 属性 | 行为 |
|---|---|
| `table_prefix = "app_"` | 自动表名前缀 |
| `strip_suffix = "Dto"` | 自动表名先去掉匹配的类型名后缀，再转 snake case |
| `column_prefix = "u."` | 默认列表达式前缀 |
| `column_case = "snake"` | 默认 snake case |
| `column_case = "upper_snake"` | 默认 UPPER_SNAKE_CASE |
| `column_case = "verbatim"` | 保留 Rust 字段名 |

显式 `from` 优先于自动表名策略。`search_field(name, result_alias)` 明确区分 HTTP 名称和解码 alias；`sqlx(rename)` 与显式 alias 必须一致。`search_ignore` / `sqlx(skip)`、`FilterOnly` 与静态 `SearchProjection` 保留原合同。共享字段若发生重复，明确报错，不隐式覆盖片段内容。

这些能力对应 Java 的映射继承、命名策略和注解协作。Rust 使用编译期组合及显式 sqlx 属性，不反射读取其他 ORM 的私有注解；生命周期继续通过 context hook、PageEnricher 和显式 trait 接入。

## 4. 命名连接池与可信上下文路由

```rust
let router = PgPoolRouter::default()
    .register_named("primary", primary_searcher)?
    .register_named("archive", archive_searcher)?
    .with_default_named("primary")?
    .route_entity::<History>("archive")?;
let page = router.search_map::<History>(&params, &scope).await?;
```

pool 名称是非空、最多 64 字节的 ASCII 字母/数字/下划线/连字符配置标签；重复名称或不存在的静态路由报错。按实体路由以 `TypeId` 为键，两个同名 Rust 类型不会互相覆盖。原有 `new`、`with_searcher`、`register_entity`、`register_pool` 继续可用。

```rust
impl PgPoolSelector<AuthContext> for RegionSelector {
    fn select_pool(&self, _: &'static EntityMeta, context: &AuthContext)
        -> Result<String, PoolRouteError>
    {
        match context.region {
            Region::East => Ok("primary".into()),
            Region::West => Ok("archive".into()),
        }
    }
}
let page = router.search_with_context::<User, _>(&params, &scope, &auth_context, &selector).await?;
let search = router.searcher_with_context::<User, _>(&auth_context, &selector)?;
// 所选 searcher 也提供 count、summaries、scan、export 等现有入口。
```

selector 是应用拥有的同步非阻塞代码，只接收实体与受信任的类型化上下文。应用必须从认证/授权步骤构造上下文；库不会把 URL 参数提供给 selector。动态选择未知 pool 时拒绝请求，不回退默认 pool。普通实体路由没有专用配置时才使用默认 pool。

每个命名 pool 对应独立 `Arc<PgSiftQuery>`，保留其 policy、timeout、并发预算、SQL 模板缓存和扩展配置。路由产生 debug 事件，字段只有 entity/backend/pool_route；pool 标签应是低敏感度的稳定配置名，不使用租户或用户 ID。请求上下文不进入日志或公开结果。

仅支持 PostgreSQL；不提供跨 pool JOIN、事务或全局快照。普通 query 不要求 transaction。

## 5. 可配置时间与列表 Codec

时间 Codec 需要 `chrono` feature：

| Codec | 输入与内部值 | 动态结果 |
|---|---|---|
| `DateCodec` | 配置日期格式 → NaiveDate | 配置日期文本，无时区 |
| `TimeCodec` | 配置格式或精确 ISO 时间 → NaiveTime | 配置时间文本 |
| `DateTimeCodec` | 配置格式或精确 ISO 本地时间 → NaiveDateTime | 配置本地时间文本，无时区 |
| `TimestampCodec::new(name, field, format, timezone)` | 配置本地时间 / RFC 3339 / epoch 毫秒 → UTC DateTime | 在配置 IANA 时区中格式化 |

`TimestampCodec` 在构造时检查格式和 IANA 时区。配置本地时间遇到夏令时重复或不存在时明确报错；带 offset 的 RFC 3339 或配置格式是明确瞬时值，可以消除歧义。解析先尝试 RFC 3339，再尝试配置格式，最后接受 epoch 毫秒；因此紧凑数字日期不会被误判为 epoch。空字符串和超出范围的 epoch 毫秒报错。数据库 NULL 保留 NULL。

Time/DateTime/Timestamp 的 RPC 表示使用精确内部文本，保留小数秒；展示格式即使只到分钟也不会导致 RPC 查询值损失。builder、bind、cursor 使用内部值，RowMap / CSV 使用展示结果；类型化 `FromRow` 保持原数据库类型。

```rust
let policy = QueryPolicy::default()
    .with_codec(TimestampCodec::new("created", "createdAt", "%Y/%m/%d %H:%M:%S", "Asia/Shanghai")?)
    .with_codec(ListResultCodec::new("tags", "tags", "|", ValueType::I64, 128)?);
```

`ListResultCodec` 将数据库 Text 字段按字面分隔符转成 `RowValue::List`；不改变输入查询的 Text bind，不增加 PostgreSQL array 字段运算符。分隔符长度 1..32 字节，项目预算 1..16384；仅接受标量项目类型，不接受 Json 对象项目。NULL 保留 NULL，空白文本输出空列表，非空文本中的空项目按项目类型处理：Text 允许，数值等类型报错。不会隐式 trim 各项。

列表在 serde 中输出 JSON 数组，`i64` 项仍输出精确字符串，CSV 的默认文本格式为紧凑 JSON。列表不能作为 cursor 值。配置包含分隔符、项目类型、项目预算并参与 policy fingerprint。它对应 Java 的字符串转简单 List；复杂业务对象列表可通过显式 PageEnricher 实现。

## 6. 验证

新增 core `rpc` / `business_codecs`、facade `product_alignment`、sqlx `pool_router` / `product_alignment` 和 derive compile-fail 回归。真实 PostgreSQL 17.11 用例在独立临时测试库创建两套相同表结构和两个独立 pool，验证并发路由、数据隔离、配置和模板缓存隔离、路由日志、typed/RowMap、count/scan/CSV、自定义运算符绑定及 RPC 经 GET/POST 的结果等价。

这些功能的完成不代表 GA 发布门禁通过；发布依赖图、远端 CI、fuzz 和正式部署性能基线的剩余状态见 [06-STATUS.md](./06-STATUS.md) 与 [14-PERFORMANCE-BASELINE.md](./14-PERFORMANCE-BASELINE.md)。
