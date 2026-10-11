use super::syntax::parse_string;
use crate::naming::to_snake_case;
use syn::{Attribute, Ident, LitBool, LitInt, LitStr, Token};

pub(crate) const DEFAULT_CONTROL_NAMES: &[&str] = &[
    "page",
    "size",
    "offset",
    "sort",
    "order",
    "orderBy",
    "gexpr",
    "onlySelect",
    "selectExclude",
];

pub(crate) struct EntityConfig {
    pub(crate) from: String,
    pub(crate) joins: Vec<String>,
    pub(crate) base_where: Option<String>,
    pub(crate) fixed_having: Option<String>,
    pub(crate) distinct: bool,
    pub(crate) default_order: Option<String>,
    pub(crate) stable_order: Option<String>,
    pub(crate) max_page_size: u32,
    pub(crate) max_offset: u64,
    pub(crate) timeout_ms: u64,
    pub(crate) column_prefix: String,
    pub(crate) column_case: String,
}

pub(crate) fn parse_entity_config(attrs: &[Attribute], ident: &Ident) -> syn::Result<EntityConfig> {
    let mut config = EntityConfig {
        from: to_snake_case(&ident.to_string()),
        joins: Vec::new(),
        base_where: None,
        fixed_having: None,
        distinct: false,
        default_order: None,
        stable_order: None,
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 5_000,
        column_prefix: String::new(),
        column_case: "snake".to_owned(),
    };
    let mut explicit_from = false;
    let mut table_prefix = String::new();
    let mut strip_suffix = String::new();
    for attribute in attrs
        .iter()
        .filter(|attr| attr.path().is_ident("search_entity"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("from") {
                config.from = parse_string(meta.value()?)?;
                explicit_from = true;
            } else if meta.path.is_ident("joins") {
                let input = meta.value()?;
                let content;
                syn::bracketed!(content in input);
                let joins =
                    syn::punctuated::Punctuated::<LitStr, Token![,]>::parse_terminated(&content)?;
                for join in joins {
                    let value = join.value();
                    let upper = value.trim_start().to_ascii_uppercase();
                    if ![
                        "JOIN ",
                        "INNER JOIN ",
                        "LEFT JOIN ",
                        "LEFT OUTER JOIN ",
                        "RIGHT JOIN ",
                        "RIGHT OUTER JOIN ",
                        "FULL JOIN ",
                        "FULL OUTER JOIN ",
                        "CROSS JOIN ",
                    ]
                    .iter()
                    .any(|prefix| upper.starts_with(prefix))
                    {
                        return Err(
                            meta.error("joins entries must start with a server-owned JOIN variant")
                        );
                    }
                    config.joins.push(value);
                }
            } else if meta.path.is_ident("table_prefix") {
                table_prefix = parse_string(meta.value()?)?;
            } else if meta.path.is_ident("strip_suffix") {
                strip_suffix = parse_string(meta.value()?)?;
            } else if meta.path.is_ident("column_prefix") {
                config.column_prefix = parse_string(meta.value()?)?;
            } else if meta.path.is_ident("column_case") {
                config.column_case = parse_string(meta.value()?)?;
                if !matches!(
                    config.column_case.as_str(),
                    "snake" | "upper_snake" | "verbatim"
                ) {
                    return Err(meta.error("column_case must be snake, upper_snake or verbatim"));
                }
            } else if meta.path.is_ident("base_where") {
                config.base_where = Some(parse_string(meta.value()?)?);
            } else if meta.path.is_ident("fixed_having") {
                config.fixed_having = Some(parse_string(meta.value()?)?);
            } else if meta.path.is_ident("distinct") {
                config.distinct = if meta.input.peek(Token![=]) {
                    meta.value()?.parse::<LitBool>()?.value
                } else {
                    true
                };
            } else if meta.path.is_ident("default_order") {
                config.default_order = Some(parse_string(meta.value()?)?);
            } else if meta.path.is_ident("stable_order") {
                config.stable_order = Some(parse_string(meta.value()?)?);
            } else if meta.path.is_ident("max_page_size") {
                config.max_page_size = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            } else if meta.path.is_ident("max_offset") {
                config.max_offset = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            } else if meta.path.is_ident("timeout_ms") {
                config.timeout_ms = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            } else if meta.path.is_ident("group_by") {
                return Err(
                    meta.error("group_by was removed in v0.4; mark fields with kind = GroupKey")
                );
            } else {
                return Err(meta.error("unknown search_entity option"));
            }
            Ok(())
        })?;
    }
    if !explicit_from {
        let name = ident.to_string();
        let name = if strip_suffix.is_empty() {
            name.as_str()
        } else {
            name.strip_suffix(&strip_suffix).unwrap_or(&name)
        };
        config.from = format!("{table_prefix}{}", to_snake_case(name));
    }
    if !config.joins.is_empty() {
        config.from.push(' ');
        config.from.push_str(&config.joins.join(" "));
    }
    Ok(config)
}
