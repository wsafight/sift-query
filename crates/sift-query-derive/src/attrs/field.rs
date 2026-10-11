use super::syntax::{option_string_tokens, parse_bool_or_flag, parse_string};
use super::{EntityConfig, has_sqlx_skip_or_default, sqlx_rename};
use crate::naming::{to_screaming_snake, to_snake_case, valid_name};
use crate::types::{outer_type_ident, unwrap_query_type, value_type_tokens};
use crate::validation::validate_sql_fragment;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use std::collections::BTreeSet;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Field, Ident, LitStr, Path, Token, Type};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Kind {
    Scalar,
    GroupKey,
    Aggregate,
    FilterOnly,
}

impl Kind {
    fn meta_tokens(self) -> TokenStream2 {
        let variant = match self {
            Self::Scalar => quote!(Scalar),
            Self::GroupKey => quote!(GroupKey),
            Self::Aggregate => quote!(Aggregate),
            Self::FilterOnly => quote!(FilterOnly),
        };
        quote!(::sift_query::__private::FieldKind::#variant)
    }

    pub(crate) fn marker_tokens(self) -> TokenStream2 {
        match self {
            Self::Scalar => quote!(::sift_query::__private::ScalarKind),
            Self::GroupKey => quote!(::sift_query::__private::GroupKeyKind),
            Self::Aggregate => quote!(::sift_query::__private::AggregateKind),
            Self::FilterOnly => quote!(::sift_query::__private::FilterOnlyKind),
        }
    }
}

// These flags describe independent field capabilities, not state-machine states.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct FieldConfig {
    pub(crate) index: u16,
    pub(crate) ident: Ident,
    pub(crate) const_ident: Ident,
    pub(crate) public_name: String,
    pub(crate) column: String,
    pub(crate) result_alias: Option<String>,
    pub(crate) value_type: TokenStream2,
    pub(crate) rust_value_type: Type,
    pub(crate) kind: Kind,
    pub(crate) filterable: bool,
    pub(crate) selectable: bool,
    pub(crate) sortable: bool,
    pub(crate) cursor_non_null: bool,
    pub(crate) operators: Vec<TokenStream2>,
    pub(crate) codec: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) sort_from: Option<String>,
    pub(crate) sort_operator: Option<String>,
    pub(crate) sort_template: Option<String>,
    pub(crate) custom_operators: Vec<String>,
}

impl FieldConfig {
    pub(crate) fn meta_tokens(&self) -> TokenStream2 {
        let index = self.index;
        let public_name = LitStr::new(&self.public_name, self.ident.span());
        let column = LitStr::new(&self.column, self.ident.span());
        let result_alias = option_string_tokens(self.result_alias.as_deref(), self.ident.span());
        let value_type = &self.value_type;
        let kind = self.kind.meta_tokens();
        let filterable = self.filterable;
        let selectable = self.selectable;
        let sortable = self.sortable;
        let operators = &self.operators;
        quote! {
            ::sift_query::__private::FieldMeta {
                id: ::sift_query::__private::FieldId(#index),
                public_name: #public_name,
                column: #column,
                result_alias: #result_alias,
                value_type: #value_type,
                kind: #kind,
                filterable: #filterable,
                selectable: #selectable,
                sortable: #sortable,
                operators: ::sift_query::__private::OperatorSet(&[#(#operators),*]),
            }
        }
    }
}

pub(crate) fn parse_field(
    field: &Field,
    index: u16,
    entity: &EntityConfig,
) -> syn::Result<FieldConfig> {
    let ident = field.ident.clone().expect("named field");
    let rust_name = ident.to_string().trim_start_matches("r#").to_owned();
    let sqlx_rename = sqlx_rename(&field.attrs)?;
    let mut public_name = rust_name.clone();
    let name = match entity.column_case.as_str() {
        "upper_snake" => to_screaming_snake(&rust_name),
        "verbatim" => rust_name.clone(),
        _ => to_snake_case(&rust_name),
    };
    let mut column = format!("{}{name}", entity.column_prefix);
    let mut explicit_alias = None;
    let mut kind = if outer_type_ident(&field.ty).as_deref() == Some("FilterOnly") {
        Kind::FilterOnly
    } else {
        Kind::Scalar
    };
    let mut filterable = true;
    let mut selectable = !matches!(kind, Kind::FilterOnly);
    let mut sortable = false;
    let mut cursor_non_null = false;
    let mut operators = vec![operator_tokens("Eq", ident.span())?];
    let mut operator_names = vec!["Eq".to_owned()];
    let mut codec = None;
    let mut label = None;
    let mut custom_operators = Vec::new();
    let mut sort_from = None;
    let mut sort_operator = None;
    let mut sort_template = None;

    for attribute in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("search_field"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                public_name = parse_string(meta.value()?)?;
            } else if meta.path.is_ident("column") {
                column = parse_string(meta.value()?)?;
            } else if meta.path.is_ident("result_alias") {
                explicit_alias = Some(parse_string(meta.value()?)?);
            } else if meta.path.is_ident("codec") {
                let name = parse_string(meta.value()?)?;
                if !valid_name(&name) || codec.is_some() {
                    return Err(meta.error("codec must be a unique ASCII name"));
                }
                codec = Some(name);
            } else if meta.path.is_ident("label") {
                let key = parse_string(meta.value()?)?;
                if !valid_name(&key) || label.is_some() {
                    return Err(meta.error("label must be a unique ASCII dictionary name"));
                }
                label = Some(key);
            } else if meta.path.is_ident("custom_operators") {
                let input = meta.value()?;
                let content;
                syn::bracketed!(content in input);
                let names = Punctuated::<LitStr, Token![,]>::parse_terminated(&content)?;
                for name in names {
                    let value = name.value();
                    if !valid_name(&value) || value.len() > 64 || custom_operators.contains(&value)
                    {
                        return Err(meta.error("custom_operators requires unique ASCII names"));
                    }
                    custom_operators.push(value);
                }
            } else if meta.path.is_ident("sort_from") {
                sort_from = Some(parse_string(meta.value()?)?);
            } else if meta.path.is_ident("sort_operator") {
                sort_operator = Some(parse_string(meta.value()?)?);
            } else if meta.path.is_ident("sort_template") {
                let template = parse_string(meta.value()?)?;
                validate_sql_fragment(&template, "sort_template", ident.span())?;
                let rest = template.replace("{field}", "").replace("{0}", "");
                if !template.contains("{field}")
                    || !template.contains("{0}")
                    || rest.contains(['{', '}'])
                {
                    return Err(
                        meta.error("sort_template requires {field} and {0}, no other placeholders")
                    );
                }
                sort_template = Some(template);
            } else if meta.path.is_ident("kind") {
                let kind_ident: Ident = meta.value()?.parse()?;
                kind = match kind_ident.to_string().as_str() {
                    "Scalar" => Kind::Scalar,
                    "GroupKey" => Kind::GroupKey,
                    "Aggregate" => Kind::Aggregate,
                    "FilterOnly" => Kind::FilterOnly,
                    _ => return Err(meta.error("unknown field kind")),
                };
            } else if meta.path.is_ident("filterable") {
                filterable = parse_bool_or_flag(meta.input, true)?;
            } else if meta.path.is_ident("selectable") {
                selectable = parse_bool_or_flag(meta.input, true)?;
            } else if meta.path.is_ident("sortable") {
                sortable = parse_bool_or_flag(meta.input, true)?;
            } else if meta.path.is_ident("cursor_non_null") {
                cursor_non_null = parse_bool_or_flag(meta.input, true)?;
            } else if meta.path.is_ident("operators") {
                let input = meta.value()?;
                let content;
                syn::bracketed!(content in input);
                let paths = Punctuated::<Path, Token![,]>::parse_terminated(&content)?;
                operator_names = paths
                    .iter()
                    .map(|path| {
                        let segment = path
                            .segments
                            .last()
                            .ok_or_else(|| meta.error("invalid operator"))?;
                        Ok(segment.ident.to_string())
                    })
                    .collect::<syn::Result<Vec<_>>>()?;
                if operator_names.is_empty() {
                    return Err(meta.error("operators must not be empty"));
                }
                let mut unique = BTreeSet::new();
                for operator in &operator_names {
                    if !unique.insert(operator) {
                        return Err(meta.error(format!("duplicate operator {operator}")));
                    }
                }
                operators = paths
                    .iter()
                    .zip(&operator_names)
                    .map(|(path, operator)| operator_tokens(operator, path.span()))
                    .collect::<syn::Result<Vec<_>>>()?;
            } else {
                return Err(meta.error("unknown search_field option"));
            }
            Ok(())
        })?;
    }

    if !valid_name(&public_name) || public_name.len() > 64 {
        return Err(syn::Error::new(
            ident.span(),
            "public field name must match [A-Za-z][A-Za-z0-9_]* and be at most 64 bytes",
        ));
    }
    validate_sql_fragment(&column, "column", ident.span())?;
    let is_filter_only_type = outer_type_ident(&field.ty).as_deref() == Some("FilterOnly");
    if matches!(kind, Kind::FilterOnly) != is_filter_only_type {
        return Err(syn::Error::new(
            field.ty.span(),
            "kind = FilterOnly requires the Rust type FilterOnly<T>, and vice versa",
        ));
    }
    if matches!(kind, Kind::FilterOnly) {
        if selectable {
            return Err(syn::Error::new(
                ident.span(),
                "FilterOnly fields cannot be selectable",
            ));
        }
        if !has_sqlx_skip_or_default(&field.attrs)? {
            return Err(syn::Error::new(
                ident.span(),
                "FilterOnly fields require #[sqlx(skip)] or #[sqlx(default)]",
            ));
        }
    }
    if label.is_some() && !selectable {
        return Err(syn::Error::new(
            ident.span(),
            "label bindings require a selectable source field",
        ));
    }

    let rust_value_type = unwrap_query_type(&field.ty)?.clone();
    if cursor_non_null && (!sortable || !selectable || matches!(kind, Kind::FilterOnly)) {
        return Err(syn::Error::new(
            ident.span(),
            "cursor_non_null requires a sortable selectable result field",
        ));
    }
    if [
        sort_from.is_some(),
        sort_operator.is_some(),
        sort_template.is_some(),
    ]
    .iter()
    .any(|value| *value)
        && (!sortable
            || !matches!(kind, Kind::FilterOnly)
            || sort_from.is_none()
            || sort_operator.is_none()
            || sort_template.is_none())
    {
        return Err(syn::Error::new(
            ident.span(),
            "computed ordering requires a sortable FilterOnly field and all sort_from/sort_operator/sort_template options",
        ));
    }
    let value_type = value_type_tokens(&rust_value_type)?;
    let value_name = outer_type_ident(&rust_value_type).unwrap_or_default();
    if operator_names.iter().any(|operator| {
        matches!(
            operator.as_str(),
            "StartWith" | "EndWith" | "Contain" | "OrLike" | "NotLike" | "Empty" | "NotEmpty"
        )
    }) && value_name != "String"
    {
        return Err(syn::Error::new(
            ident.span(),
            "string matching and empty operators require a String field",
        ));
    }
    if value_name == "Value" && filterable && custom_operators.is_empty() {
        return Err(syn::Error::new(
            ident.span(),
            "JSON fields are decode-only in M1; set filterable = false or expose a scalar cast",
        ));
    }
    if operator_names
        .iter()
        .any(|operator| matches!(operator.as_str(), "InList" | "NotIn"))
        && matches!(value_name.as_str(), "Vec" | "Value")
    {
        return Err(syn::Error::new(
            ident.span(),
            "InList is not supported for Bytes or JSON fields in M1",
        ));
    }
    let result_alias = if matches!(kind, Kind::FilterOnly) {
        None
    } else {
        match (explicit_alias, sqlx_rename) {
            (Some(left), Some(right)) if left != right => {
                return Err(syn::Error::new(
                    ident.span(),
                    "result_alias must match #[sqlx(rename)]",
                ));
            }
            (Some(value), _) | (_, Some(value)) => Some(value),
            (None, None) => Some(rust_name),
        }
    };
    Ok(FieldConfig {
        index,
        const_ident: format_ident!("{}", to_screaming_snake(&ident.to_string())),
        ident,
        public_name,
        column,
        result_alias,
        value_type,
        rust_value_type,
        kind,
        filterable,
        selectable,
        sortable,
        cursor_non_null,
        operators,
        codec,
        label,
        sort_from,
        sort_operator,
        sort_template,
        custom_operators,
    })
}

fn operator_tokens(name: &str, span: proc_macro2::Span) -> syn::Result<TokenStream2> {
    let variant = match name {
        "Eq" => quote!(Eq),
        "Ne" => quote!(Ne),
        "Gt" => quote!(Gt),
        "Ge" => quote!(Ge),
        "Lt" => quote!(Lt),
        "Le" => quote!(Le),
        "Between" => quote!(Between),
        "NotBetween" => quote!(NotBetween),
        "InList" => quote!(InList),
        "NotIn" => quote!(NotIn),
        "IsNull" => quote!(IsNull),
        "NotNull" => quote!(NotNull),
        "Empty" => quote!(Empty),
        "NotEmpty" => quote!(NotEmpty),
        "StartWith" => quote!(StartWith),
        "EndWith" => quote!(EndWith),
        "Contain" => quote!(Contain),
        "OrLike" => quote!(OrLike),
        "NotLike" => quote!(NotLike),
        "AlwaysTrue" => quote!(AlwaysTrue),
        "AlwaysFalse" => quote!(AlwaysFalse),
        _ => return Err(syn::Error::new(span, format!("unknown operator {name}"))),
    };
    Ok(quote!(::sift_query::__private::Operator::#variant))
}
