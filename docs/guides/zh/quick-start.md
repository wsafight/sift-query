# 快速开始

本例使用内存 SQLite，不需要外部数据库。先完成[安装](installation.md)，再把 `src/main.rs` 替换为以下完整程序。

## 声明并执行查询

```rust
use sift_query::{RawParams, SearchEntity, ServerScope, SqliteSiftQuery};
use sqlx::sqlite::SqlitePoolOptions;

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(from = "users", default_order = "id:asc", stable_order = "id:asc")]
struct User {
    #[search_field(sortable, cursor_non_null, operators = [Eq, InList])]
    id: i64,
    #[search_field(operators = [Eq, StartWith])]
    name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;

    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO users (id, name) VALUES (?, ?), (?, ?), (?, ?)")
        .bind(1_i64).bind("Alice")
        .bind(2_i64).bind("Bob")
        .bind(3_i64).bind("Ari")
        .execute(&pool)
        .await?;

    let search = SqliteSiftQuery::new(pool);
    let raw = RawParams::parse("name=A&name-op=sw&size=10")?;
    let query = search.parse::<User>(&raw)?;
    let page = search.search(&query, &ServerScope::unrestricted()).await?;

    assert_eq!(page.total_count, 2);
    for user in page.data {
        println!("{}: {}", user.id, user.name);
    }
    Ok(())
}
```

示例表是公开演示数据，因此使用 `ServerScope::unrestricted()`。接入私有业务数据时，应先根据已认证身份和授权结果构造 Scope，再执行查询。

## 运行

```bash
cargo run
```

预期输出：

```text
1: Alice
3: Ari
```

`name-op=sw` 表示字面前缀匹配，`size=10` 限制单页条数。`search` 返回结果行及总数。稳定主键排序让分页顺序确定；连接池限制为一个连接，让内存数据库示例共用同一份数据。

## 理解实体声明

`SearchEntity` 生成静态字段元数据，`sqlx::FromRow` 负责解码结果行。`id` 允许等值、列表匹配和排序，`name` 允许等值与前缀匹配。请求不能自行增加实体未允许的运算符。

`cursor_non_null` 是服务端对数据库排序表达式非空性的断言。本例使用未修改的主键。不能仅根据 Rust 类型推断非空，也不能把该断言用于可能为空的外连接表达式。

## 继续阅读

尝试[查询示例](query-cookbook.md)，对比[数据库后端](backend-selection.md)，或阅读 [Rust API](../../04-RUST-API.md)。Schema 迁移、连接池和授权由应用维护；SiftQuery 提供只读查询管线。
