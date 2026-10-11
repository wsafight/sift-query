# 查询示例

以下示例使用[快速开始](quick-start.md)中的 `User` 实体。把字符串交给 `RawParams::parse`，再用 `search.parse::<User>` 校验。真实 URL 中的值需要百分号编码。

## 按允许的运算符筛选

| 需求 | 查询字符串 |
| --- | --- |
| 名称等于 Alice | `name=Alice` |
| 名称以 A 开头 | `name=A&name-op=sw` |
| ID 属于 1 或 3 | `id=1&id=3&id-op=il` |

省略 `-op` 时使用等值比较。列表运算符依赖重复字段键，不能先把原始输入转换成 `HashMap<String, String>`。未知字段、未允许的运算符和非法值都会报错。

## 排序和分页

```text
name=A&name-op=sw&sort=id&order=desc&page=1&size=10
```

只有声明 `sortable` 的字段可用于排序。`page` 从 1 开始。实体的稳定排序参与最终排序，计算出的 offset 仍受预算约束。页码与 offset 的冲突规则见[参数协议](../../02-PROTOCOL.md)。

## 动态投影

```text
name=A&name-op=sw&onlySelect=id,name&size=10
```

动态 `RowMap` 结果使用 `search_map`。类型化 `search` 返回完整实体，动态投影不能让必需的类型化字段缺失。仅允许返回可投影且获授权的字段。

## 是否需要总数

普通 `search` 返回结果和总数。只需要行时可选 `search_list`。`search_cursor`、`search_map_cursor` 提供无需 COUNT 的 keyset 分页；配置签名后，用返回的 token 获取下一页。适合连续翻页，不用于任意跳转到某个页码。

## 显式扩展实体能力

入门实体没有允许 `Contain`、范围运算符或按 name 排序。应先在服务端声明能力，再接受相应请求。逻辑分组、聚合、HAVING、DISTINCT 和签名游标的要求见[查询语义](../../03-QUERY-SEMANTICS.md)。
