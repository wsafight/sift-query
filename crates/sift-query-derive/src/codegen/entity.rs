use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, LitStr};

use crate::attrs::{
    DEFAULT_CONTROL_NAMES, FieldConfig, Kind, has_attribute, has_sqlx_skip_or_default,
    option_string_tokens, order_tokens, parse_entity_config, parse_field, parse_order_list,
    reject_sqlx_rename_all,
};
use crate::codegen::fingerprint::schema_fingerprint;
use crate::validation::validate_sql_fragment;

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(
            input.generics.span(),
            "SearchEntity does not support generic structs in M1",
        ));
    }
    reject_sqlx_rename_all(&input.attrs)?;
    let ident = input.ident;
    let visibility = input.vis;
    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields.named,
            _ => {
                return Err(syn::Error::new(
                    data.struct_token.span,
                    "SearchEntity requires a struct with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new(
                ident.span(),
                "SearchEntity can only be derived for structs",
            ));
        }
    };

    let entity = parse_entity_config(&input.attrs, &ident)?;
    if entity.max_page_size < 15 {
        return Err(syn::Error::new(
            ident.span(),
            "max_page_size must be at least the default page size of 15",
        ));
    }
    validate_sql_fragment(&entity.from, "from", ident.span())?;
    for join in &entity.joins {
        validate_sql_fragment(join, "joins", ident.span())?;
    }
    if let Some(value) = &entity.base_where {
        validate_sql_fragment(value, "base_where", ident.span())?;
    }
    if let Some(value) = &entity.fixed_having {
        validate_sql_fragment(value, "fixed_having", ident.span())?;
    }

    let mut parsed_fields = Vec::new();
    let mut public_names = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    for field in fields {
        if has_attribute(&field.attrs, "search_ignore") {
            if !has_sqlx_skip_or_default(&field.attrs)? {
                return Err(syn::Error::new(
                    field.span(),
                    "search_ignore requires #[sqlx(skip)] or #[sqlx(default)]",
                ));
            }
            continue;
        }
        let index = u16::try_from(parsed_fields.len()).map_err(|_| {
            syn::Error::new(field.span(), "SearchEntity supports at most 65536 fields")
        })?;
        let parsed = parse_field(&field, index, &entity)?;
        if !public_names.insert(parsed.public_name.clone()) {
            return Err(syn::Error::new(
                parsed.ident.span(),
                format!("duplicate public field name {}", parsed.public_name),
            ));
        }
        if DEFAULT_CONTROL_NAMES.contains(&parsed.public_name.as_str()) {
            return Err(syn::Error::new(
                parsed.ident.span(),
                format!(
                    "field name {} conflicts with a default control parameter",
                    parsed.public_name
                ),
            ));
        }
        if let Some(alias) = &parsed.result_alias {
            if alias.len() > 63 {
                return Err(syn::Error::new(
                    parsed.ident.span(),
                    "PostgreSQL result aliases must be at most 63 bytes",
                ));
            }
            if !aliases.insert(alias.clone()) {
                return Err(syn::Error::new(
                    parsed.ident.span(),
                    format!("duplicate result alias {alias}"),
                ));
            }
        }
        parsed_fields.push(parsed);
    }
    if parsed_fields.is_empty() {
        return Err(syn::Error::new(
            ident.span(),
            "SearchEntity requires at least one query field",
        ));
    }

    let grouped = parsed_fields
        .iter()
        .any(|field| matches!(field.kind, Kind::GroupKey | Kind::Aggregate));
    if grouped
        && parsed_fields
            .iter()
            .any(|field| field.result_alias.is_some() && matches!(field.kind, Kind::Scalar))
    {
        return Err(syn::Error::new(
            ident.span(),
            "grouped entities cannot return Scalar fields; use GroupKey, Aggregate, or FilterOnly",
        ));
    }

    let field_lookup = parsed_fields
        .iter()
        .map(|field| (field.public_name.clone(), field))
        .collect::<BTreeMap<_, _>>();
    let default_order = parse_order_list(
        entity.default_order.as_deref(),
        "default_order",
        &field_lookup,
        ident.span(),
    )?;
    let stable_order = parse_order_list(
        entity.stable_order.as_deref(),
        "stable_order",
        &field_lookup,
        ident.span(),
    )?;
    if stable_order.is_empty() {
        return Err(syn::Error::new(
            ident.span(),
            "stable_order is required for M1 entities",
        ));
    }

    let fields_ident = format_ident!("{}Fields", ident);
    let condition_order_arms=parsed_fields.iter().filter_map(|field|field.sort_from.as_ref().map(|source|(field,source))).map(|(field,source)| {
        let source=field_lookup.get(source).ok_or_else(||syn::Error::new(field.ident.span(),"sort_from references an unknown public field"))?;
        if matches!(source.kind,Kind::Aggregate) { return Err(syn::Error::new(field.ident.span(),"sort_from requires a row phase field")); }
        let index=field.index; let source_index=source.index;
        let operator=field.sort_operator.as_ref().expect("validated computed order");
        if !source.custom_operators.contains(operator) { return Err(syn::Error::new(field.ident.span(),"sort_operator must be in the source field custom operator whitelist")); }
        let template=field.sort_template.as_ref().expect("validated computed order");
        Ok(quote! { ::sift_query::__private::FieldId(#index)=>Some(::sift_query::__private::ConditionOrder { source: ::sift_query::__private::FieldId(#source_index),operator:#operator,expression: ::sift_query::__private::CustomOperator::new("condition_order",#template,1).expect("validated order template") }) })
    }).collect::<syn::Result<Vec<_>>>()?;
    let custom_operator_arms = parsed_fields
        .iter()
        .filter(|field| !field.custom_operators.is_empty())
        .map(|field| {
            let index = field.index;
            let names = &field.custom_operators;
            quote! { ::sift_query::__private::FieldId(#index) => &[#(#names),*] }
        });
    let field_meta = parsed_fields.iter().map(FieldConfig::meta_tokens);
    let non_null_arms = parsed_fields
        .iter()
        .filter(|field| field.cursor_non_null)
        .map(|field| {
            let index = field.index;
            quote! { ::sift_query::__private::FieldId(#index) => true }
        });
    let codec_arms = parsed_fields.iter().filter_map(|field| {
        field.codec.as_ref().map(|codec| {
            let index = field.index;
            quote! { ::sift_query::__private::FieldId(#index) => Some(#codec) }
        })
    });
    let label_bindings = parsed_fields.iter().filter_map(|field| {
        field.label.as_ref().map(|key| {
            let source = LitStr::new(&field.public_name, field.ident.span());
            let key = LitStr::new(key, field.ident.span());
            quote! { (#source, #key) }
        })
    });
    let field_constants = parsed_fields.iter().map(|field| {
        let const_ident = &field.const_ident;
        let value_type = &field.rust_value_type;
        let kind = field.kind.marker_tokens();
        let index = field.index;
        quote! {
            pub const #const_ident: ::sift_query::__private::FieldRef<#ident, #value_type, #kind> =
                ::sift_query::__private::FieldRef::new(::sift_query::__private::FieldId(#index));
        }
    });
    let cursor_arms = parsed_fields
        .iter()
        .filter(|field| field.sortable && field.result_alias.is_some())
        .map(|field| {
            let rust_field = &field.ident;
            let index = field.index;
            quote! {
                ::sift_query::__private::FieldId(#index) => Some(
                    ::sift_query::IntoCursorValue::to_cursor_value(&self.#rust_field)
                )
            }
        });
    let default_order_tokens = order_tokens(&default_order);
    let stable_order_tokens = order_tokens(&stable_order);
    let from = LitStr::new(&entity.from, ident.span());
    let entity_name = LitStr::new(&ident.to_string(), ident.span());
    let base_where = option_string_tokens(entity.base_where.as_deref(), ident.span());
    let fixed_having = option_string_tokens(entity.fixed_having.as_deref(), ident.span());
    let distinct = entity.distinct;
    let max_page_size = entity.max_page_size;
    let max_offset = entity.max_offset;
    let timeout_ms = entity.timeout_ms;
    let fingerprint = schema_fingerprint(&ident.to_string(), &entity, &parsed_fields);

    Ok(quote! {
        #visibility struct #fields_ident;

        impl #fields_ident {
            #(#field_constants)*
        }

        impl ::sift_query::__private::SearchEntity for #ident {
            fn cursor_key_non_null(field: ::sift_query::__private::FieldId) -> bool {
                match field { #(#non_null_arms,)* _ => false }
            }
            fn condition_order(field: ::sift_query::__private::FieldId)->Option<::sift_query::__private::ConditionOrder> { match field { #(#condition_order_arms,)* _=>None } }
            fn custom_operator_names(field: ::sift_query::__private::FieldId) -> &'static [&'static str] {
                match field { #(#custom_operator_arms,)* _ => &[] }
            }
            fn codec_name(field: ::sift_query::__private::FieldId) -> Option<&'static str> {
                match field {
                    #(#codec_arms,)*
                    _ => None,
                }
            }
            fn label_bindings() -> &'static [(&'static str, &'static str)] {
                &[#(#label_bindings),*]
            }
            const META: &'static ::sift_query::__private::EntityMeta =
                &::sift_query::__private::EntityMeta {
                    entity_name: #entity_name,
                    from: #from,
                    base_where: #base_where,
                    fixed_having: #fixed_having,
                    distinct: #distinct,
                    fields: &[#(#field_meta),*],
                    default_order: &[#(#default_order_tokens),*],
                    stable_order: &[#(#stable_order_tokens),*],
                    limits: ::sift_query::__private::EntityLimits {
                        max_page_size: #max_page_size,
                        max_offset: #max_offset,
                        timeout_ms: #timeout_ms,
                    },
                    schema_fingerprint: #fingerprint,
                };

            fn cursor_value(
                &self,
                field: ::sift_query::__private::FieldId,
            ) -> Option<::sift_query::CursorValue> {
                match field {
                    #(#cursor_arms,)*
                    _ => None,
                }
            }
        }
    })
}
