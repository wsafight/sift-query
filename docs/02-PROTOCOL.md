# SiftQuery M2 参数协议

本文件定义从 URL query string 或 POST form 参数到 `UserQuery<E>` 的规范行为。M2 保留全部 M1 严格规则，并增加运算符和签名游标；除明确标记为 SHOULD 的条款外均为兼容合同。

## 1. 输入与编码

`RawParams::parse` 接受不含前导 `?` 的原始 query string，并按 `application/x-www-form-urlencoded` 规则解析：

- MUST 在拆分参数项后对 key 和 value 做百分号解码。
- `+` 解码为空格；字面加号必须写成 `%2B`。
- key 和 value 必须是合法 UTF-8。
- 不完整或非法的百分号编码返回 `InvalidEncoding`。
- 参数名区分大小写；字段名、组名和控制参数都不做大小写归一化。
- 必须保留参数项的输入顺序和重复值。
- 必须先检查原始 query string 总长度，再进行解码和分配。

HTTP 适配器 MUST 从框架提供的原始 query string，或限长读取的 `application/x-www-form-urlencoded` POST body 构造 `RawParams`，禁止先反序列化为 `HashMap<String, String>`。POST 与 GET 使用相同的查询预算和严格语义；统一入口的来源冲突、读取 deadline 及配置见 [09-TRANSPORT-AND-PERFORMANCE.md](./09-TRANSPORT-AND-PERFORMANCE.md)。

```rust
pub struct RawParams {
    entries: Vec<(String, String)>,
}
```

## 2. 名称与保留字

M1 的公开字段名和组名使用以下 ASCII 语法：

```ebnf
name        = letter, { letter | digit | "_" } ;
letter      = "A".."Z" | "a".."z" ;
digit       = "0".."9" ;
field-name  = name ;
group-name  = name ;
```

限制：

- 名称长度为 1 到 64 字节。
- 字段名与组名区分大小写。
- M1 名称中禁止 `-`、`.`、`,`、`:` 和空白字符。
- 字段外部名称不得等于当前 `QueryPolicy` 配置的控制参数名。
- derive MUST 在编译期检查默认控制参数的冲突；自定义 `QueryPolicy` 在构造时检查控制名自身，在 `ParamParser<E>` 创建或首次解析 E 时检查与实体字段名的冲突。

默认保留控制参数：

```text
page size offset cursor sort order orderBy gexpr onlySelect selectExclude
```

## 3. 参数键语法

```ebnf
condition-id = [ group-name, "." ], field-name ;
value-key    = condition-id, [ "-", index ] ;
operator-key = condition-id, "-op" ;
ignore-key   = condition-id, "-ic" ;
index        = "0" | nonzero-digit, { digit } ;
```

一个条件由 `(可选组名, 字段名)` 唯一标识。同一个组内同一字段只能形成一个条件。M1 不支持为同一字段同时声明两种不同运算符；需要上下界时使用 `Between`。

示例：

```text
age=20
age-op=ge
A.username=Jack
A.username-op=sw
```

控制参数不能带组名前缀。`A.page=1` 被解释为字段条件，并最终因字段不存在而报错。

## 4. 字段值

字段值支持两种形式。

### 4.1 重复键形式

```text
status=1&status=2
```

值顺序按原始 query string 中的出现顺序保存。

### 4.2 编号形式

```text
status-0=1&status-1=2
```

编号必须从 0 开始连续、不得重复。值顺序按编号排序，与参数项在 query string 中的物理顺序无关。

### 4.3 冲突规则

- 同一个条件禁止混用重复键和编号形式。
- `field=value` 与 `field-0=value` 视为两种形式，不能同时出现。
- 编号缺口、重复编号和超出单条件值数量上限均返回参数错误。
- 空字符串是有效字符串值，不等同于缺少值或 SQL NULL。
- HTTP 参数不能直接表达 SQL NULL；NULL 判断必须使用 `nl` 或 `nn`。

## 5. 运算符

### 5.1 运算符表

| 短名 | 名称 | 值数量 | PostgreSQL SQL 语义 |
|---|---|---:|---|
| `eq` | Equal | 1 | `=` |
| `ne` | NotEqual | 1 | `<>` |
| `gt` | GreaterThan | 1 | `>` |
| `ge` | GreaterEqual | 1 | `>=` |
| `lt` | LessThan | 1 | `<` |
| `le` | LessEqual | 1 | `<=` |
| `bt` | Between | 2 | `BETWEEN ... AND ...`，包含边界 |
| `nb` | NotBetween | 2 | `NOT BETWEEN ... AND ...` |
| `il` | InList | 1..N | PostgreSQL `= ANY($n)` |
| `ni` | NotIn | 1..N | PostgreSQL `<> ALL($n)` |
| `nl` | IsNull | 0 | `IS NULL` |
| `nn` | NotNull | 0 | `IS NOT NULL` |
| `ey` | Empty | 0 | `IS NULL OR = ''`，仅文本字段 |
| `ny` | NotEmpty | 0 | `IS NOT NULL AND <> ''`，仅文本字段 |
| `sw` | StartWith | 1 | 字面前缀匹配 |
| `ew` | EndWith | 1 | 字面后缀匹配 |
| `ct` | Contain | 1 | 字面包含匹配 |
| `ol` | OrLike | 1..N | 多个服务端验证后的 LIKE pattern 使用 OR |
| `nk` | NotLike | 1 | `NOT LIKE` pattern |
| `at` | AlwaysTrue | 0 | 常量真条件 |
| `af` | AlwaysFalse | 0 | 常量假条件 |

规则：

- 未提供 `field-op` 时默认为 `eq`。
- 每个条件最多出现一个 `field-op`；重复即返回 `DuplicateParameter`。
- 运算符必须位于字段声明的白名单中。
- 值数量与上表不符时返回 `WrongArity`，禁止静默取第一个或丢弃多余值。
- `nl`、`nn`、`ey`、`ny`、`at` 和 `af` 只通过 `field-op` 构造；同时携带字段值时返回 `WrongArity`。
- `il`、`ni` 和 `ol` 至少需要一个值；空列表不产生隐式语义。
- HTTP 输入不产生包含 NULL 元素的数组。
- `CustomOperator` 只能由服务端构造安全模板。通过 `QueryPolicy::with_custom_operator` 注册名称/类型且字段 `custom_operators` 白名单允许后，HTTP MAY 用 `field-op` 名称激活它；未注册、类型不匹配、参数数量错误或字段不允许 MUST 报错。HTTP MUST NOT 提供模板。注册预算与 RPC 合同见 [10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md](./10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md)。

示例：

```text
deleted-op=nl
age-0=20&age-1=30&age-op=bt
status=1&status=2&status-op=il
```

### 5.2 忽略大小写

`field-ic` 的值只能是 `true` 或 `false`，默认 `false`。

- 每个条件最多出现一次 `field-ic`。
- 文本字段的 `eq`、`ne`、`sw`、`ew`、`ct`、`ol` 和 `nk` 可以使用 `ic=true`。
- 其他字段类型或运算符使用 `ic=true` 时返回 `IgnoreCaseNotAllowed`。
- `ic=false` 与未提供该参数语义相同。

大小写与通配符的准确 SQL 语义见 [03-QUERY-SEMANTICS.md](./03-QUERY-SEMANTICS.md)。

## 6. 字段能力检查

一个字段条件必须同时满足：

1. 字段存在于实体元信息。
2. `filterable = true`。
3. 运算符在字段白名单中。
4. 输入字符串能完整转换为字段的查询值类型。
5. 运算符、字段种类和条件阶段组合合法。

失败时不得降级为字符串比较、不得忽略条件，也不得执行 SQL。

## 7. 分页参数

| 参数 | 默认值 | 规则 |
|---|---:|---|
| `page` | 1 | 整数，必须 `>= 1` |
| `size` | 15 | 整数，必须位于 `0..=max_page_size` |
| `offset` | 无 | 整数，必须位于 `0..=max_offset` |
| `cursor` | 无 | 空值表示首屏；非空值为签名 token，最大 1024 字节 |

规范行为：

- `page`、`offset` 与 `cursor` 三种模式显式互斥。
- 两者都未提供时使用 `page=1`。
- 提供 `offset` 时不再应用 page 默认值。
- page 模式的计算公式是 `(page - 1) * size`，必须使用 checked arithmetic。
- 计算得到的 offset 也必须 `<= max_offset`；不只检查显式 offset。
- `size=0` 时不执行列表 SQL；`search` 仍执行统计 SQL，`search_list` 直接返回空列表。
- 每个分页控制参数最多出现一次。
- `search_first` 对分页参数的特殊行为见查询语义文档。

### 7.1 游标分页

- `cursor=` 表示请求第一张游标页；后续页使用上一页返回的 `next_cursor`。
- token 使用 URL-safe、无 padding 的 Base64，并由 HMAC-SHA256 签名；密钥至少 32 字节。
- token 绑定实体 schema 指纹、完整最终排序、方向、NULL 顺序和排序值，任一不匹配均返回 `InvalidValue`。
- 游标比较是排他的 keyset 比较，不使用 OFFSET；`total_count` 不包含 keyset 条件。
- 当前 M2 对分组实体拒绝 cursor，避免把 WHERE/HAVING 的 keyset 语义混合。
- token 是不透明值，不得由客户端解析或修改，也不得记录到默认日志。

## 8. 排序参数

M1 支持两种互斥形式。

### 8.1 `sort + order`

```text
sort=age,id&order=desc,asc
```

- `sort` 是 1 到 5 个逗号分隔字段名。
- `order` 可省略；省略时所有字段使用 `asc`。
- 提供 `order` 时，方向数量必须与字段数量相同。
- `order` 不能在没有 `sort` 时单独出现。

### 8.2 `orderBy`

```text
orderBy=age:desc,id:asc
```

每个条目必须包含字段名和 `asc` 或 `desc`。字段、方向或条目为空均返回错误。

### 8.3 通用排序规则

- 两种形式同时出现时返回 `ConflictingParameters`。
- `sort`、`order` 和 `orderBy` 各自最多出现一次。
- 排序字段必须存在且 `sortable = true`。
- 同一字段不能重复排序。
- M1 不允许在列表内部使用空白字符；调用方应发送规范形式。
- 客户端排序之后，渲染器可能追加服务端稳定键，见查询语义文档。

## 9. 动态投影

动态投影只适用于 `search_map` 和 `search_map_with`：

```text
onlySelect=id,username
selectExclude=role_name
```

规则：

- `onlySelect` 与 `selectExclude` 互斥。
- 两者各自最多出现一次。
- 列表不能为空，不允许空条目和重复字段。
- 字段必须存在且 `selectable = true`。
- `selectExclude` 排除全部可选字段时返回 `EmptyProjection`。
- `onlySelect` 结果的 RowMap 键顺序按实体声明顺序，不按请求顺序。
- 完整类型查询收到任一投影参数时返回 `ProjectionNotAllowed`。
- DISTINCT 与排序组合还有额外限制，见查询语义文档。

## 10. 逻辑分组

### 10.1 条件组

带组名的字段键使用 `group.field`：

```text
A.username=Jack
B.username=Alice
C.age=20
C.age-op=ge
```

同一组内的条件默认使用 AND。未带组名的条件属于根条件，始终在组表达式外层使用 AND。

### 10.2 gexpr 语法

```ebnf
expr    = or-expr ;
or-expr = and-expr, { "|", and-expr } ;
and-expr = primary, { "&", primary } ;
primary = group-name | "(", expr, ")" ;
```

AND 优先级高于 OR。M1 不支持 NOT、函数、常量和字段名直接出现在 `gexpr` 中。

URL 中的 `&` 是参数分隔符，必须百分号编码。完整示例：

```text
A.username=Jack&B.username=Alice&C.age=20&C.age-op=ge&gexpr=%28A%7CB%29%26C
```

解码后的 gexpr 为：

```text
(A|B)&C
```

### 10.3 分组校验

- `gexpr` 最多出现一次。
- 没有 `gexpr` 时，所有根条件和命名组使用 AND。
- 存在 `gexpr` 时，每个被引用组必须已经定义。
- 每个已定义的命名组必须至少在 `gexpr` 中出现一次；否则返回 `UnusedGroup`，禁止静默忽略。
- 同一个组可以在表达式中重复引用；规范化阶段可以执行有界幂等化简。
- 空组、空表达式、连续运算符和不平衡括号均返回 `InvalidGroupExpr`。
- 根条件不写入 gexpr，始终与整个组表达式做 AND。
- 表达式通过语法校验后还必须通过 WHERE/HAVING 阶段校验；跨阶段 OR 返回 `MixedPredicatePhase`。

ServerScope 不使用组名，也不参与 gexpr，始终位于用户谓词外层。

## 11. 控制参数重复与未知参数

以下参数出现多次必须报错：

```text
page size offset cursor sort order orderBy gexpr onlySelect selectExclude
```

每个条件的 `field-op` 和 `field-ic` 出现多次同样报错。

无法解析为控制参数或合法字段键的参数返回 `UnknownParameter`。能够解析为字段键但字段不存在时返回 `UnknownField`。严格模式不得忽略未知参数。

## 12. 查询预算

默认预算：

| 限制 | 默认值 | 计量时机 |
|---|---:|---|
| 原始 query string 总长度 | 64 KiB | 解码前 |
| 原始参数项数 | 128 | 拆分后 |
| 单个参数名长度 | 160 字节 | 解码后 |
| 单个参数值长度 | 4096 字节 | 解码后 |
| 解码后 key + value 总长度 | 256 KiB | 解码后 |
| 用户条件数 | 32 | 条件归并后 |
| 单条件逻辑值数量 | 100 | 类型转换前 |
| 全部逻辑值数量 | 512 | 类型转换前 |
| 数据库绑定槽位数 | 512 | 计划生成后 |
| 排序字段数 | 5 | 排序解析后 |
| gexpr 编码后长度 | 768 字节 | URL 解码前 |
| gexpr 解码后长度 | 256 字节 | URL 解码后 |
| gexpr AST 深度 | 8 | 解析时 |
| gexpr AST 节点数 | 64 | 解析时 |
| 单页最大条数 | 100 | 分页解析后 |
| 最大 offset | 20000 | 显式或计算后 |
| cursor token 最大长度 | 1024 字节 | 签名验证前 |

数组绑定占用一个数据库绑定槽位，但数组中的每个元素都计入“逻辑值数量”。两种预算不得混用。

超过任一预算返回 `TooComplex`，且不得获取数据库连接。

`QueryPolicy` 可以收紧默认预算。放宽硬上限必须调用明确命名的构造 API，不能通过普通实体属性意外突破全局限制。实体级限制与全局限制同时存在时取更严格者。

## 13. 参数错误

```rust
pub enum ParamError {
    InvalidEncoding,
    InvalidName,
    UnknownParameter,
    UnknownField,
    DuplicateParameter,
    ConflictingParameters,
    NonContiguousIndex,
    WrongArity,
    InvalidValue,
    NotFilterable,
    NotSortable,
    NotSelectable,
    OperatorNotAllowed,
    IgnoreCaseNotAllowed,
    ProjectionNotAllowed,
    EmptyProjection,
    PagingOutOfRange,
    PagingNotAllowed,
    InvalidGroupExpr,
    UndefinedGroup,
    UnusedGroup,
    MixedPredicatePhase,
    UnstablePagination,
    DistinctOrderNotProjected,
    TooComplex,
}
```

公开枚举可以在实现阶段按 `non_exhaustive` 和错误上下文字段调整，但错误类别和“参数错误不执行 SQL”的行为是稳定合同。

## 14. HTTP 错误外显

Web 适配层 SHOULD 将一般 `ParamError` 映射为 HTTP 400；原始查询字节预算超限可映射为 HTTP 413。默认对外使用稳定、无内部信息的错误码或统一消息：

```text
invalid query parameters
```

具体字段名、内部 SQL 表达式和允许运算符列表只进入受控日志或 tracing，不进入默认公开响应。应用可以基于错误类别提供更友好的错误码，但不得直接泄露 SQL、表名和绑定值。
