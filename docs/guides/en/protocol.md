# Protocol

This English guide summarizes the [complete Chinese contract](../../02-PROTOCOL.md). Switch the page language to Chinese to read the full specification.

## Preserve raw input

Use `RawParams::parse` on the original URL query string or bounded form body. Preserve repeated keys for list values. Names and values are decoded once; malformed encoding, duplicate control parameters and incompatible input forms are rejected.

## Fields and operators

`name=Alice` uses equality. `name=A&name-op=sw` uses a literal prefix match. Operators must be allowed by the entity and policy.

| Operator | Meaning | Value count |
| --- | --- | --- |
| `eq`, `ne`, `gt`, `ge`, `lt`, `le` | Scalar comparisons | 1 |
| `bt`, `nb` | Inclusive between / not between | 2 |
| `il`, `ni` | List membership / exclusion | 1 or more |
| `nl`, `nn` | SQL null / non-null | 0 |
| `ey`, `ny` | Empty / non-empty text | 0 |
| `sw`, `ew`, `ct` | Literal prefix / suffix / substring | 1 |
| `ol`, `nk` | Allowed LIKE patterns | 1 or more / 1 |
| `at`, `af` | Constant true / false | 0 |

Custom operators require server registration plus field allowlisting. HTTP selects an operator by name and supplies values; it never supplies its SQL template.

## Paging, projection and grouping

`page` starts at 1, `size` defaults to 15, and the default maximum size is 100. `page`, `offset` and `cursor` are mutually exclusive. The default maximum calculated offset is 20,000. Cursor tokens have a 1,024-byte limit.

Order through `sort` plus `order`, or through `orderBy`, using sortable fields. Dynamic map projection uses `onlySelect` or `selectExclude`; these are mutually exclusive. Named groups and `gexpr` build bounded logical expressions. OR branches cannot mix WHERE and HAVING phases.

## Budgets and errors

Policy limits bound raw bytes, decoded bytes, parameter items, conditions, values, bind slots, ordering fields and expression depth. Validation errors must happen before database execution. Web responses should expose structured errors without private SQL or authorization data.

See the [query cookbook](query-cookbook.md) for concrete requests and [query semantics](semantics.md) for execution behavior.
