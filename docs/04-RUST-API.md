# SiftQuery M2 Rust API

本文件定义 v0.4 的公开 Rust 类型和 API 合同。代码块反映当前实现；省略的 trait bound 或私有字段不改变对应行为合同。

## 1. Cargo 与 feature

最小 PostgreSQL 使用方式：

```toml
[dependencies]
sift-query = { version = "0.1", features = ["postgres", "chrono"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "chrono"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
```

当前 feature：

| feature | 作用 |
|---|---|
| `postgres` | 启用 `PgSiftQuery`、PostgreSQL renderer、binder 和 decoder |
| `chrono` | 日期和时间类型 |
| `decimal` | PostgreSQL numeric 与 Decimal 类型 |
| `uuid` | UUID 类型 |
| `json` | JSON/JSONB 结果、结构化精确输入；查询通过明确白名单开放 |
| `serde` | `RowValue` 的明确 JSON 序列化适配 |
| `postgres-search` / `postgres-types` | 受控 PG 运算符/全文与 bigint[]、text[]、int8range、inet/cidr |
| `pg-trgm` / `ltree` / `postgis` | 默认关闭的扩展谓词、输入 Codec 和受控相似排序 |
| `metadata` / `rsql` / `rls` | 公共描述/OpenAPI、有界 RSQL、可信 PG local 请求上下文 |

facade 必须统一依赖 feature，避免用户直接依赖的 sqlx feature 与内部实现不一致。

排序字段可使用 `cursor_non_null` 显式声明查询表达式由数据库保证非空；对应 SearchEntity::cursor_key_non_null，默认 false。它是服务端模型合同，不能由 HTTP 开启，具体语义见 03 的 cursor 章节。

## 2. 实体声明

### 2.1 普通实体

```rust
use chrono::NaiveDateTime;
use serde::Serialize;
use sift_query::{FilterOnly, SearchEntity};
use sqlx::FromRow;

#[derive(Debug, Serialize, FromRow, SearchEntity)]
#[search_entity(
    from = "app_user u left join role r on u.role_id = r.id",
    base_where = "u.deleted = false",
    default_order = "created_at:desc",
    stable_order = "id:asc",
    max_page_size = 100,
    max_offset = 20_000,
    timeout_ms = 2_000
)]
pub struct User {
    #[search_field(column = "u.id", sortable, operators = [Eq, InList])]
    pub id: i64,

    #[search_field(column = "u.username", sortable, operators = [Eq, StartWith])]
    pub username: String,

    #[search_field(column = "u.status", operators = [Eq, InList])]
    pub status: i32,

    #[search_field(column = "u.age", operators = [Eq, Ge, Le, Between])]
    pub age: i32,

    #[search_field(column = "u.created_at", sortable, operators = [Ge, Le, Between])]
    pub created_at: NaiveDateTime,

    #[search_field(column = "r.name", filterable = false)]
    pub role_name: Option<String>,

    #[search_field(column = "u.tenant_id", kind = FilterOnly, filterable = false)]
    #[sqlx(skip)]
    #[serde(skip)]
    pub tenant_id: FilterOnly<i64>,
}
```

`default_order` 与 `stable_order` 不是任意 SQL，而是由宏解析的字段排序列表：

```ebnf
order-list = field-name, ":", ("asc" | "desc"),
             { ",", field-name, ":", ("asc" | "desc") } ;
```

derive 必须验证字段存在、可排序且没有重复。`stable_order` 字段元组在最终结果集上的唯一性由应用和数据库 schema 保证。

实体级属性：

| 属性 | 默认值 | 当前含义 |
|---|---|---|
| `from` | 结构体名转 snake_case 后作为引用标识符 | 表、视图或受信任 FROM 片段 |
| `joins` | 无 | 服务端声明的 JOIN 片段列表，按声明顺序追加到 `from` |
| `table_prefix` | 空 | 自动表名的前缀；显式 from 优先 |
| `strip_suffix` | 空 | 自动表名前去掉匹配的结构体名后缀 |
| `column_prefix` | 空 | 默认列表达式前缀 |
| `column_case` | `snake` | 默认列名策略：snake / upper_snake / verbatim |
| `base_where` | 无 | 不含运行时绑定的固定 WHERE 条件 |
| `fixed_having` | 无 | 不含运行时绑定的固定 HAVING 条件 |
| `distinct` | false | 是否对最终用户投影去重 |
| `default_order` | 无 | 无客户端排序时使用的字段排序列表 |
| `stable_order` | 无 | 构成唯一全序的服务端字段排序列表 |
| `max_page_size` | 实例值 | 实体单页上限，只能收紧实例策略 |
| `max_offset` | 实例值 | 实体 offset 上限，只能收紧实例策略 |
| `timeout_ms` | 实例值 | 实体执行超时，只能收紧实例策略 |

`from`、`base_where` 和 `fixed_having` 是受信任应用配置，但仍禁止 SiftQuery 占位符、多语句和语句终止符。`group_by` 属性在 v0.4 M1 中被移除；使用它必须产生明确的宏迁移错误。

`joins` 用于声明只读 Read Model 的固定关联，不能由 HTTP 或 RPC 输入生成。每个片段会经过与 `from` 相同的 SQL 片段检查，并进入实体 schema fingerprint：

```rust
#[derive(Debug, SearchEntity)]
#[search_entity(
    from = "app_user u",
    joins = ["JOIN organization o ON o.id = u.org_id"],
    stable_order = "id:asc"
)]
struct UserReadModel {
    #[search_field(column = "u.id", sortable)]
    id: i64,
    #[search_field(column = "o.name")]
    organization_name: String,
}
```

这是服务端固定的读模型组合，字段、排序、Scope 和分页仍沿用同一套安全合同；它不提供客户端关系展开、自动 JOIN 推导或跨 pool JOIN。

### 2.2 分组实体

M1 不再接受实体级自由字符串 `group_by`。GROUP BY 从 `GroupKey` 字段生成：

```rust
use chrono::NaiveDate;
use sift_query::{FilterOnly, SearchEntity};
use sqlx::FromRow;

#[derive(Debug, FromRow, SearchEntity)]
#[search_entity(
    from = "student_score s",
    stable_order = "course_id:asc"
)]
pub struct CourseScore {
    #[search_field(
        column = "s.course_id",
        kind = GroupKey,
        sortable,
        operators = [Eq, InList]
    )]
    pub course_id: i64,

    #[search_field(
        column = "sum(s.score)",
        kind = Aggregate,
        sortable,
        operators = [Eq, Ge, Le, Between]
    )]
    pub total_score: i64,

    #[search_field(
        name = "createdDate",
        column = "s.created_at::date",
        kind = FilterOnly,
        operators = [Eq, Ge, Le, Between]
    )]
    #[sqlx(skip)]
    pub created_date_filter: FilterOnly<NaiveDate>,
}
```

`FilterOnly<T>` 是零大小、可默认构造的标记类型。生成的字段常量携带查询值类型 `T`，而不是 `FilterOnly<T>`：

```rust
CourseScoreFields::CREATED_DATE_FILTER // FieldRef<CourseScore, NaiveDate, FilterOnlyKind>
```

### 2.3 字段属性

| 属性 | 默认值 | 含义 |
|---|---|---|
| `name` | Rust 字段名 | 外部参数名和 RowMap 键 |
| `column` | Rust 字段名转 snake_case | 受信任列或表达式 |
| `kind` | `Scalar` | `Scalar`、`GroupKey`、`Aggregate`、`FilterOnly` |
| `filterable` | true | 是否允许用户条件 |
| `selectable` | true；FilterOnly 为 false | 是否允许 RowMap 返回 |
| `sortable` | false | 是否允许客户端排序 |
| `operators` | `[Eq]` | 用户运算符白名单 |
| `custom_operators` | 空 | 已注册 HTTP 自定义运算符的名称白名单 |
| `codec` | 无 | 命名输入/动态结果 Codec 绑定 |
| `result_alias` | Rust 字段名 | 完整类型的 PostgreSQL 结果别名 |

约束：

- `FilterOnly` 必须使用 `FilterOnly<T>` 字段类型并同时标记 `#[sqlx(skip)]`。
- `FilterOnly` 强制 `selectable=false`。
- `Aggregate` 的 Rust 类型必须匹配聚合表达式的数据库输出类型。
- 分组实体中的普通结果字段在编译期报错。
- `search_ignore` 字段完全不进入 SiftQuery 元信息；在完整查询中必须能由 `FromRow` 跳过或默认构造。

### 2.4 FromRow 约束

M1 支持普通具名结构体字段、`Option<T>`、明确的 `#[sqlx(rename = "...")]`、`#[sqlx(skip)]` 和 `#[sqlx(default)]`。

M1 不承诺自动兼容：

- `#[sqlx(flatten)]`。
- 类型级 `rename_all`。
- 需要自定义运行时反射的字段映射。

结果别名默认是去除 raw identifier 前缀后的 Rust 字段名，并按 PostgreSQL 规则引用。若存在 `#[sqlx(rename)]`，`SearchEntity` derive 必须读取并使用同一别名。

derive 必须拒绝：

- 超过 PostgreSQL 63 字节标识符上限的结果别名。
- 完全相同或 PostgreSQL 截断后冲突的别名。
- `search_ignore` 或 `FilterOnly` 无法由 `FromRow` 跳过/默认构造的明显组合。

## 3. 静态元信息

```rust
pub trait SearchEntity: Sized + Send + Sync + 'static {
    const META: &'static EntityMeta;
}

pub struct EntityMeta {
    pub entity_name: &'static str,
    pub from: &'static str,
    pub base_where: Option<&'static str>,
    pub fixed_having: Option<&'static str>,
    pub distinct: bool,
    pub fields: &'static [FieldMeta],
    pub default_order: &'static [OrderMeta],
    pub stable_order: &'static [OrderMeta],
    pub limits: EntityLimits,
    pub schema_fingerprint: u64,
}

pub struct FieldMeta {
    pub id: FieldId,
    pub public_name: &'static str,
    pub column: &'static str,
    pub result_alias: Option<&'static str>,
    pub value_type: ValueType,
    pub kind: FieldKind,
    pub filterable: bool,
    pub selectable: bool,
    pub sortable: bool,
    pub operators: OperatorSet,
}

#[repr(transparent)]
pub struct FieldId(pub(crate) u16);
```

`EntityMeta` 不依赖 sqlx。字段解析成功后，AST 保存 `FieldId`，但 `UserQuery<E>` 仍通过实体类型和 schema fingerprint 防止跨实体误用。

实体的 `max_page_size`、`max_offset` 和 `timeout_ms` 与实例级策略同时存在时取更严格值。实体声明不能通过普通属性放宽实例的硬上限。

## 4. 类型化字段引用

derive 生成带实体、值类型和阶段种类的字段常量：

```rust
pub struct FieldRef<E, T, K> {
    id: FieldId,
    _marker: std::marker::PhantomData<fn() -> (E, T, K)>,
}

pub enum ScalarKind {}
pub enum GroupKeyKind {}
pub enum AggregateKind {}
pub enum FilterOnlyKind {}

pub struct FilterOnly<T>(std::marker::PhantomData<fn() -> T>);

impl<T> Default for FilterOnly<T> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

pub trait RowPhaseKind {}
impl RowPhaseKind for ScalarKind {}
impl RowPhaseKind for GroupKeyKind {}
impl RowPhaseKind for FilterOnlyKind {}
```

库类型 `FilterOnly<T>` 必须无条件实现 `Copy`、`Clone`、`Debug` 和 `Default`，这些实现不能要求 T 实现对应 trait。

`ServerScope<E>` 的构建方法只接受 `K: RowPhaseKind`，从类型系统上阻止把聚合条件用作租户或数据权限条件。

字段值类型在编译期检查；运算符是否位于字段白名单仍由构建器在 `build()` 时验证。

## 5. 核心查询类型

```rust
pub struct UserQuery<E> {
    predicate: PredicateArena,
    projection: Projection,
    order: Vec<OrderBy>,
    paging: Paging,
    schema_fingerprint: u64,
    _entity: std::marker::PhantomData<fn() -> E>,
}

pub struct ServerScope<E> {
    predicate: PredicateArena,
    _entity: std::marker::PhantomData<fn() -> E>,
}

pub struct SearchOptions<E> {
    summaries: Vec<AggregateRef<E>>,
    _entity: std::marker::PhantomData<fn() -> E>,
}

pub struct QueryPlan<E> {
    meta: &'static EntityMeta,
    row_predicate: PredicateArena,
    group_predicate: PredicateArena,
    projection: Projection,
    order: Vec<OrderBy>,
    paging: Paging,
    summaries: Vec<AggregateRef<E>>,
}
```

字段保持私有，通过只读访问器和构建器操作。调用方不能在验证后直接修改 AST、FieldId 或 schema fingerprint。

`UserQuery<E>` 的公开 Clone 实现必须共享不可变内部存储或采用等价的低成本方案；执行 `&UserQuery<E>` 时不能深拷贝整棵 AST。分页状态必须保留 page、size、offset 是否由调用方显式提供，以便 `search_first` 执行操作级校验。

## 6. 查询值、绑定值与结果值

v0.4 不再用同一个 `SqlValue` 同时承担查询输入、PostgreSQL 数组绑定和行结果。

### 6.1 Core 标量值

```rust
pub enum ScalarValue {
    Bool(bool),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    #[cfg(feature = "decimal")]
    Decimal(rust_decimal::Decimal),
    Text(String),
    Bytes(Vec<u8>),
    #[cfg(feature = "chrono")]
    Date(chrono::NaiveDate),
    #[cfg(feature = "chrono")]
    Time(chrono::NaiveTime),
    #[cfg(feature = "chrono")]
    DateTime(chrono::NaiveDateTime),
    #[cfg(feature = "chrono")]
    Timestamp(chrono::DateTime<chrono::Utc>),
    #[cfg(feature = "uuid")]
    Uuid(uuid::Uuid),
    #[cfg(feature = "json")]
    Json(Box<serde_json::Value>),
    #[cfg(feature = "postgres-types")]
    ArrayI64(Vec<Option<i64>>),
    #[cfg(feature = "postgres-types")]
    ArrayText(Vec<Option<String>>),
    #[cfg(feature = "postgres-types")]
    Int8Range(Box<Int8Range>),
    #[cfg(feature = "postgres-types")]
    Network(Box<ipnet::IpNet>),
}

pub struct TypedList {
    pub element_type: ValueType,
    pub values: Vec<ScalarValue>,
}
```

查询输入没有 SQL `Null`。SQL NULL 条件由独立 AST 运算符表达；JSON null 属于结构化 JSON，原生数组中的 NULL 元素属于有明确元素类型的数组值。ScalarValue 是 non_exhaustive。

### 6.2 PostgreSQL 绑定值

```rust
pub enum PgBindValue {
    Scalar(ScalarValue),
    Array(TypedList),
}

pub struct RenderedPgQuery {
    pub sql: std::sync::Arc<str>,
    pub binds: Vec<PgBindValue>,
    pub shape_id: QueryShapeId,
}
```

`Array(TypedList)` 是标量列 InList/NotIn 绑定，必须验证元素同质性且列表非空。原生数据库数组属于 ScalarValue::ArrayI64/ArrayText，有明确元素类型，可包含空数组或 NULL 元素，两者语义分开。

### 6.3 RowMap 结果值

```rust
pub type RowMap = indexmap::IndexMap<String, RowValue>;

pub enum RowValue {
    Null,
    List(Vec<RowValue>),
    Bool(bool),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    #[cfg(feature = "decimal")]
    Decimal(rust_decimal::Decimal),
    Text(String),
    Bytes(Vec<u8>),
    #[cfg(feature = "chrono")]
    Date(chrono::NaiveDate),
    #[cfg(feature = "chrono")]
    Time(chrono::NaiveTime),
    #[cfg(feature = "chrono")]
    DateTime(chrono::NaiveDateTime),
    #[cfg(feature = "chrono")]
    Timestamp(chrono::DateTime<chrono::Utc>),
    #[cfg(feature = "uuid")]
    Uuid(uuid::Uuid),
    #[cfg(feature = "json")]
    Json(serde_json::Value),
    #[cfg(feature = "postgres-types")]
    ArrayI64(Vec<Option<i64>>),
    #[cfg(feature = "postgres-types")]
    ArrayText(Vec<Option<String>>),
    #[cfg(feature = "postgres-types")]
    Int8Range(Int8Range),
    #[cfg(feature = "postgres-types")]
    Network(ipnet::IpNet),
}
```

RowMap 解码依据最终投影中的 FieldMeta，不根据数据库运行时类型名称猜测 Rust 类型。

`RowValue::List` 由受控结果 Codec 或展示增强生成，不作为 SQL 数组绑定或 cursor 值。serde 输出 JSON 数组，CSV 默认用紧凑 JSON 文本；各项保留原有标量表示规则。

## 7. HTTP 值转换

M1 使用以下规范格式：

| ValueType | 输入格式 |
|---|---|
| Bool | `true` 或 `false` |
| I32 / I64 | 十进制整数，不允许分隔符 |
| F32 / F64 | Rust 标准十进制形式，但拒绝 NaN 和正负 Infinity |
| Decimal | rust_decimal 可无损解析的十进制文本 |
| Text | URL 解码后的 UTF-8 字符串 |
| Bytes | 无填充 base64url；只在字段显式启用时允许 |
| Date | `YYYY-MM-DD` |
| Time | `HH:MM:SS[.fraction]` |
| DateTime | `YYYY-MM-DDTHH:MM:SS[.fraction]`，无时区 |
| Timestamp | RFC 3339，转换并保存为 UTC |
| Uuid | 标准连字符 UUID |

`Option<T>` 的 HTTP 查询值类型仍是 T；SQL NULL 使用 `nl/nn`。JSONB/原生数组/range/network 的专用编码和白名单运算符见 [12-POSTGRES-CAPABILITIES.md](./12-POSTGRES-CAPABILITIES.md)。JSON 与所有新增结构化类型不支持 cursor；tsvector/ltree/geometry 首版使用 FilterOnly 查询字段。

以上是未配置 Codec 时的默认值格式。命名 Codec 可提供业务表示，新增时间/时区/列表 Codec 及其 RPC 无损表示见 [10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md](./10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md)。

## 8. QueryPolicy

`QueryPolicy` 是构造后不可变的实例级协议与预算配置：

```rust
pub struct QueryPolicy {
    names: ParamNames,
    limits: QueryLimits,
    hard_limits: QueryHardLimits,
}
```

要求：

- 默认控制参数名与参数协议一致。
- 自定义控制参数名必须满足名称语法且彼此不能重复；它与目标实体字段名的冲突在 `ParamParser<E>` 创建或首次解析 E 时检查。
- 普通 builder 只能收紧默认限制。
- 放宽 hard limit 必须使用明确命名的高权限构造函数。
- `PgSiftQuery` 构造后不能原地修改 policy；变更配置应创建新实例，避免同一实例产生不稳定语义。
- 会改变 SQL 的配置必须进入 policy fingerprint。

## 9. RawParams 与解析

```rust
impl RawParams {
    pub fn parse(query: &str) -> Result<Self, ParamError>;
}

pub struct ParamParser<'p> {
    policy: &'p QueryPolicy,
}

impl<'p> ParamParser<'p> {
    pub fn parse<E: SearchEntity>(
        &self,
        raw: &RawParams,
    ) -> Result<UserQuery<E>, ParamError>;
}
```

`PgSiftQuery::parse<E>` 是使用当前实例 policy 的便捷入口。

解析完成的 `UserQuery<E>` 仍会在执行前检查 schema fingerprint 和当前 policy 的硬预算，防止在不同配置实例之间复用由更宽松策略构建的查询。

## 10. 程序化查询

```rust
let query = search
    .query_builder::<User>()
    .eq(UserFields::STATUS, 1)
    .between(UserFields::AGE, 20, 30)
    .order_by(UserFields::CREATED_AT, SortOrder::Desc)
    .page(1, 15)
    .build()?;

let page = search
    .search::<User>(&query, &scope)
    .await?;
```

`QueryBuilder<E>` 输出 `UserQuery<E>`，并使用创建它的 `PgSiftQuery` 所持有的 `QueryPolicy`。程序化查询必须遵守与 HTTP 查询相同的：

- 运算符白名单。
- 字段能力。
- 分页上限。
- 条件和值数量预算。
- WHERE/HAVING 阶段规则。
- 稳定排序规则。

区别只是程序化 API 的字段和值类型由 Rust 编译器检查，不需要字符串字段解析和字符串值转换。

嵌套布尔条件使用类型化 `FilterExpr<E>`，不接受 SQL 字符串：

```rust
let name_or_status = FilterExpr::any([
    FilterExpr::starts_with(UserFields::USERNAME, "A")?,
    FilterExpr::eq(UserFields::STATUS, 1)?,
])?;

let query = search
    .query_builder::<User>()
    .filter(name_or_status)
    .build()?;
```

`FilterExpr::all/any` 拒绝空集合，并沿用 HTTP gexpr 的阶段规则：包含 Row 与 Group 条件的 OR 在 build 阶段返回 `MixedPredicatePhase`。

## 11. QueryInput

为避免为 RawParams 与 UserQuery 复制整套执行方法，M1 使用 sealed 输入 trait：

```rust
pub trait QueryInput<E: SearchEntity>: private::Sealed {
    fn into_user_query(
        self,
        policy: &QueryPolicy,
    ) -> Result<UserQuery<E>, ParamError>;
}
```

只为以下类型实现：

- `&RawParams`：按当前 policy 解析。
- `&UserQuery<E>`：校验实体 schema fingerprint 和当前 policy 后复制不可变查询计划。

该 trait 保持 sealed，防止第三方绕过验证直接构造“已验证”查询。实现可以在不改变公开调用方式的前提下为已验证查询增加内部借用快路径。

## 12. PgSiftQuery

```rust
pub struct PgSiftQuery {
    pool: sqlx::PgPool,
    policy: QueryPolicy,
    execution: PgExecutionPolicy,
}

impl PgSiftQuery {
    pub fn new(pool: sqlx::PgPool) -> Self;

    pub fn with_policy(
        pool: sqlx::PgPool,
        policy: QueryPolicy,
        execution: PgExecutionPolicy,
    ) -> Result<Self, ConfigError>;

    pub fn parse<E: SearchEntity>(
        &self,
        raw: &RawParams,
    ) -> Result<UserQuery<E>, ParamError>;

    pub fn query_builder<E: SearchEntity>(&self) -> QueryBuilder<'_, E>;

    pub async fn search<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>;

    pub async fn search_with<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: &SearchOptions<E>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>;

    pub async fn search_map<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<RowMap>, PgSearchError>;

    pub async fn search_map_with<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: &SearchOptions<E>,
    ) -> Result<Page<RowMap>, PgSearchError>;

    pub async fn search_list<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Vec<E>, PgSearchError>
    where
        E: SearchEntity + for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>;

    pub async fn search_first<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Option<E>, PgSearchError>
    where
        E: SearchEntity + for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>;

    pub async fn count<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<i64, PgSearchError>;

    pub async fn sum<E, T, K>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        field: FieldRef<E, T, K>,
    ) -> Result<Option<<T as PgSumType>::Output>, PgSearchError>
    where
        E: SearchEntity,
        T: PgSumType,
        K: SummableKind;
}
```

`impl QueryInput<E>` 允许原始参数和程序化查询共享同一套 API：

```rust
search.search::<User>(&raw, &scope).await?;
search.search::<User>(&query, &scope).await?;
```

## 13. ServerScope

```rust
let scope = ServerScope::<User>::new()
    .eq(UserFields::TENANT_ID, tenant_id)
    .build()?;
```

`ServerScope` 支持 Row 阶段的 AND、OR 和括号组合，但不接受字符串字段名和 SQL 片段。它拥有独立于 HTTP 的服务端复杂度上限。

`filterable=false` 只禁止用户查询和 `QueryBuilder<E>` 使用该字段，不禁止服务端把 Row-capable 字段用于 ServerScope。

明确不需要数据隔离的调用必须写为：

```rust
let scope = ServerScope::<User>::unrestricted();
```

命名使用 `unrestricted` 而不是 `allow_all`，以强调这是一次安全边界选择。

## 14. Page 与聚合

```rust
pub struct Page<T> {
    pub data: Vec<T>,
    pub total_count: i64,
    pub summaries: Vec<SummaryValue>,
    pub next_cursor: Option<String>,
}

pub struct SummaryValue {
    pub field: &'static str,
    pub function: AggregateFn,
    pub value_type: ValueType,
    pub value: Option<RowValue>,
}

pub enum AggregateFn {
    Sum,
    Avg,
    Min,
    Max,
    Custom(CustomAggregate),
}
```

M2 的 `SearchOptions` 公开 `sum/avg/min/max/custom`，最多包含 8 项。`summaries` 顺序与请求顺序一致。空输入的聚合值返回 `None`；框架不自动转换为零。`CustomAggregate` 与 `CustomOperator` 一样，只能由服务端静态配置创建。

启用 `serde` feature 时，`Page<T>` 在 `T: Serialize` 时实现 `Serialize`，`SummaryValue` 和 `RowValue` 使用查询语义文档定义的稳定 JSON 映射，因此 axum 等 Web 框架可以直接返回 `Json<Page<T>>`。

```rust
let options = SearchOptions::<User>::builder()
    .sum(UserFields::AGE)
    .build()?;

let page = search
    .search_with::<User>(&query, &scope, &options)
    .await?;
```

### 14.1 PostgreSQL SUM 类型

```rust
pub trait PgSumType {
    type Output: PgSummaryValue;
}

pub trait PgSummaryValue {}

pub trait SummableKind {}
impl SummableKind for ScalarKind {}
impl SummableKind for GroupKeyKind {}
impl SummableKind for AggregateKind {}

impl PgSumType for i32 {
    type Output = i64;
}

#[cfg(feature = "decimal")]
impl PgSumType for i64 {
    type Output = rust_decimal::Decimal;
}

#[cfg(feature = "decimal")]
impl PgSumType for rust_decimal::Decimal {
    type Output = rust_decimal::Decimal;
}

impl PgSumType for f32 {
    type Output = f32;
}

impl PgSumType for f64 {
    type Output = f64;
}
```

最终实现以 PostgreSQL 与 sqlx 0.9 集成测试为准。未实现 `PgSumType` 的字段不会暴露类型化 sum。

`i64` 和 Decimal 的实现只在 `decimal` feature 开启时提供。未开启对应 feature 时，涉及该输出类型的聚合在编译期不可用。

## 15. M2 执行与扩展 API

静态投影由 `#[derive(SearchProjection)]` 声明，通过 `search_as::<E, P>` 执行。游标由 `QueryPolicy::with_cursor_codec(CursorCodec)` 显式启用；`cursor=` 请求首屏，`Page.next_cursor` 返回下一页不透明 token。

```rust
#[derive(sqlx::FromRow, sift_query::SearchProjection)]
#[search_projection(entity = User)]
struct UserSummary {
    id: i64,
    #[search_field(source = "username")]
    name: String,
}

let page = search.search_as::<User, UserSummary>(&raw, &scope).await?;
```

```rust
pub enum ExecutionStrategy {
    SequentialBestEffort,
    ParallelBestEffort,
    Snapshot(SnapshotIsolation),
}

pub enum SnapshotIsolation {
    RepeatableRead,
    Serializable,
}

pub enum CountStrategy {
    Separate,
    WindowWhenPossible,
}

pub struct PgExecutionPolicy {
    pub timeout: Duration,
    pub strategy: ExecutionStrategy,
    pub count_strategy: CountStrategy,
    pub max_parallel_queries: usize,
    pub shape_cache_capacity: usize,
}
```

`shape_cache_capacity=0` 关闭缓存；允许范围为 `1..=16384`。缓存保留基础 SQL 模板及绑定位置/转换指令，不保留绑定值或查询结果。命中发生在基础 SQL 渲染前，每次从当前 QueryPlan 重建 binds；分区解析和 SQL 拦截器继续逐次执行。可通过 `shape_cache_stats` 读取累计统计并通过 `clear_shape_cache` 清空条目。GET/POST adapter、前端 helper 和完整缓存合同见 [09-TRANSPORT-AND-PERFORMANCE.md](./09-TRANSPORT-AND-PERFORMANCE.md)。

```rust
pub type SearchStream<T> = Pin<Box<dyn Stream<Item = Result<T, PgSearchError>> + Send>>;

pub fn search_stream<E>(
    &self,
    query: impl QueryInput<E>,
    scope: &ServerScope<E>,
) -> Result<SearchStream<E>, PgSearchError>;
```

`search_stream` 只执行 list SQL，不返回 total_count。流被提前 drop 时，持有活动语句的连接会关闭而不是直接归还池。

扩展点通过 consuming builder 注册：

```rust
let search = PgSiftQuery::new(pool)
    .with_partition_resolver(partition_resolver)
    .with_sql_interceptor(sql_interceptor)
    .with_result_filter(result_filter);
```

- `PartitionResolver` 返回结构化 `PartitionName`；schema、table、alias 都必须通过标识符校验并由库引用。
- `SqlInterceptor` 只接收占位符 SQL，不接收绑定值。
- `ResultFilter` 只处理 `search_map` 的 RowMap，可修改或丢弃行；权限隔离仍必须使用 `ServerScope`。

## 16. 错误分层

core crate：

```rust
pub enum ParamError { /* 参数协议错误 */ }
pub enum PlanError { /* 查询计划错误 */ }
pub enum ConfigError { /* 静态配置错误 */ }
```

PostgreSQL/sqlx crate：

```rust
pub enum PgSearchError {
    Param(ParamError),
    Plan(PlanError),
    Render(RenderError),
    Bind(String),
    ParallelGateClosed,
    Sql(sqlx::Error),
    Decode(DecodeError),
    Extension(ExtensionError),
    Timeout(TimeoutStage),
}

pub enum TimeoutStage {
    PoolAcquire,
    ListQuery,
    ClusterQuery,
}
```

facade 在 `postgres` feature 下可以：

```rust
pub use sift_query_sqlx::PgSearchError as SearchError;
```

core 不能依赖或暴露 `sqlx::Error`。

## 17. 快速开始

```rust
use serde::Serialize;
use sift_query::{PgSiftQuery, RawParams, SearchEntity, ServerScope};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Serialize, FromRow, SearchEntity)]
#[search_entity(
    from = "app_user",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
struct User {
    #[search_field(sortable, operators = [Eq, InList])]
    id: i64,

    #[search_field(operators = [Eq, StartWith])]
    username: String,

    #[search_field(operators = [Eq, InList])]
    status: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPool::connect("postgres://localhost/app").await?;
    let search = PgSiftQuery::new(pool);
    let raw = RawParams::parse(
        "status=1&sort=id&order=desc&page=1&size=15"
    )?;
    let scope = ServerScope::<User>::unrestricted();

    let page = search.search::<User>(&raw, &scope).await?;
    println!("total = {}, rows = {}", page.total_count, page.data.len());
    Ok(())
}
```

该示例必须进入 compile test；测试数据库连接部分可使用 `no_run` doctest 或独立示例 crate 编译。

## 18. P0 产品补齐入口

`UserQuery::to_rpc` / `to_rpc_with_names` 和 `QueryBuilder::build_for_rpc` / `build_for_rpc_with_names` 编码完整用户查询；`QueryPolicy::with_custom_operator` 注册 HTTP 运算符；`search_entities!` 组合字段片段和命名映射预设；`PgPoolRouter` 命名注册及 `PgPoolSelector<C>` 提供可信上下文路由。各入口的预算、错误、Java 对应和实例见 [10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md](./10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md)。

## 19. 查询增强 API

字段权限通过服务端 Scope 附加：

```rust
let access = FieldAccessPolicy::<User>::builder()
    .allow(UserFields::ID)
    .allow(UserFields::STATUS)
    .build()?;
let scope = ServerScope::<User>::unrestricted().with_field_access(access)?;
let page = search.search_map::<User>(&raw, &scope).await?;
```

Map 的动态全投影会自动保留允许字段；用户显式投影、过滤、排序、聚合和 facet 使用未授权字段会在获取连接前返回错误。服务端 Scope 自身的权限条件仍照常生效。

无总数 cursor 分页使用 `CursorPage<T>`：

```rust
let page = search.search_map_cursor::<User>(&raw, &scope).await?;
if page.has_more {
    // 将 page.next_cursor 作为下一次 RawParams 的 cursor 参数。
}
```

该入口只执行一个有界 list 查询，内部读取 `size + 1` 行，不执行 count/cluster。`CustomOperator::exists`、`CustomOperator::not_exists` 用于服务端声明的关联过滤；客户端不能传入 SQL 模板、表名或列名。

PostgreSQL 还提供以下运维和分析入口：

- `validate_schema::<E>()`：用 `EXPLAIN`、`sqlx::describe` 校验结果列的数量、alias、数据库类型和 `ValueType`。
- `diagnose::<E>()`：对授权后的 list/cluster SQL 返回 JSON 执行计划和保守索引建议；只读、参数化、不会自动改索引。
- `buckets::<E>()`：执行服务端声明的 `BucketRequest`，支持时间单位和固定数值边界。
- `facets::<E>()` / `search_map_with_facets::<E>()`：执行受限 Facet，返回 top-N 桶与 `other_count`；`FacetRequest::excluding_self_filter()` 可排除当前 Facet 字段的用户条件，同时保留 ServerScope。

这些分析 API 不接受客户端 SQL。Facet convenience API 会先执行列表再执行 Facet；需要列表与 Facet 共享快照时，在调用方开启外部只读事务并使用 `search_map_with_facets_on`。
