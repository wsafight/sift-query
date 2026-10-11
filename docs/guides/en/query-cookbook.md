# Query cookbook

These examples use the `User` entity from the [quick start](quick-start.md). Submit the string to `RawParams::parse`, then `search.parse::<User>`. Values in real URLs must be percent-encoded.

## Filter by an allowed operator

| Intent | Query string |
| --- | --- |
| Exact name | `name=Alice` |
| Literal name prefix | `name=A&name-op=sw` |
| One of two IDs | `id=1&id=3&id-op=il` |

Omitting `-op` selects equality. Repeated field keys are retained, which is required for list operators; do not convert the raw input into a `HashMap<String, String>` first. Unknown fields, disallowed operators and invalid values are errors.

## Sort and page

```text
name=A&name-op=sw&sort=id&order=desc&page=1&size=10
```

Only fields declared sortable can be used for ordering. `page` begins at 1. SiftQuery applies the stable order from entity metadata and checks the calculated offset against policy limits. Page-number and offset inputs have explicit conflict rules; see [Parameter protocol](protocol.md).

## Request a dynamic projection

```text
name=A&name-op=sw&onlySelect=id,name&size=10
```

Use `search_map` for a dynamic `RowMap` result. Typed `search` returns the complete entity; a dynamic projection cannot turn a required typed field into an omitted field. Only selectable, authorized fields can be returned.

## Decide whether to count

A normal `search` returns page data and total count. Use `search_list` when you only need rows. Cursor entry points such as `search_cursor` and `search_map_cursor` provide keyset paging without COUNT; configure cursor signing and use the returned token for the next request. They are suited to sequential navigation rather than arbitrary page-number jumps.

## Extend the entity deliberately

The quick-start entity does not allow `Contain`, range operators or sorting by name. Add those capabilities on the server before accepting such parameters. For grouped conditions, aggregates, `HAVING`, DISTINCT and signed-cursor requirements, read [Query semantics](semantics.md).
