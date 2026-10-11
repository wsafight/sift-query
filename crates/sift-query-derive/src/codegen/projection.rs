use std::collections::BTreeSet;

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, LitStr, Path};

use crate::attrs::{has_attribute, has_sqlx_skip_or_default, sqlx_rename};
use crate::naming::to_screaming_snake;

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(
            input.generics.span(),
            "SearchProjection does not support generic structs in M2",
        ));
    }
    let projection_ident = input.ident;
    let entity = parse_entity(&input.attrs)?;
    let fields_ident = fields_type(&entity)?;
    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields.named,
            _ => {
                return Err(syn::Error::new(
                    data.struct_token.span,
                    "SearchProjection requires a struct with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new(
                projection_ident.span(),
                "SearchProjection can only be derived for structs",
            ));
        }
    };

    let mut mappings = Vec::new();
    let mut cursor_arms = Vec::new();
    let mut sources = BTreeSet::new();
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
        let ident = field.ident.as_ref().expect("named field");
        let rust_name = ident.to_string().trim_start_matches("r#").to_owned();
        let source = parse_source(&field.attrs)?.unwrap_or_else(|| rust_name.clone());
        let alias = sqlx_rename(&field.attrs)?.unwrap_or(rust_name);
        if alias.len() > 63 {
            return Err(syn::Error::new(
                ident.span(),
                "PostgreSQL projection aliases must be at most 63 bytes",
            ));
        }
        if !sources.insert(source.clone()) {
            return Err(syn::Error::new(
                ident.span(),
                format!("duplicate projection source {source}"),
            ));
        }
        if !aliases.insert(alias.clone()) {
            return Err(syn::Error::new(
                ident.span(),
                format!("duplicate projection alias {alias}"),
            ));
        }
        let source_ident = format_ident!("{}", to_screaming_snake(&source));
        let alias = LitStr::new(&alias, ident.span());
        mappings.push(quote! {
            ::sift_query::__private::ProjectionField {
                source: #fields_ident::#source_ident.id(),
                result_alias: #alias,
            }
        });
        cursor_arms.push(quote! {
            value if value == #fields_ident::#source_ident.id() => Some(
                ::sift_query::IntoCursorValue::to_cursor_value(&self.#ident)
            )
        });
    }
    if mappings.is_empty() {
        return Err(syn::Error::new(
            projection_ident.span(),
            "SearchProjection requires at least one mapped field",
        ));
    }

    Ok(quote! {
        impl ::sift_query::__private::SearchProjection<#entity> for #projection_ident {
            const FIELDS: &'static [::sift_query::__private::ProjectionField] =
                &[#(#mappings),*];

            fn cursor_value(
                &self,
                source: ::sift_query::__private::FieldId,
            ) -> Option<::sift_query::CursorValue> {
                match source {
                    #(#cursor_arms,)*
                    _ => None,
                }
            }
        }
    })
}

fn parse_entity(attrs: &[syn::Attribute]) -> syn::Result<Path> {
    let mut entity = None;
    for attribute in attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("search_projection"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("entity") {
                let value = meta.value()?;
                entity = Some(value.parse::<Path>()?);
                Ok(())
            } else {
                Err(meta.error("unknown search_projection option"))
            }
        })?;
    }
    entity.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "SearchProjection requires #[search_projection(entity = EntityType)]",
        )
    })
}

fn fields_type(entity: &Path) -> syn::Result<Path> {
    let mut fields = entity.clone();
    let Some(last) = fields.segments.last_mut() else {
        return Err(syn::Error::new(entity.span(), "invalid entity path"));
    };
    if !matches!(last.arguments, syn::PathArguments::None) {
        return Err(syn::Error::new(
            last.arguments.span(),
            "generic projection entities are not supported",
        ));
    }
    last.ident = format_ident!("{}Fields", last.ident);
    Ok(fields)
}

fn parse_source(attrs: &[syn::Attribute]) -> syn::Result<Option<String>> {
    let mut source = None;
    for attribute in attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("search_field"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("source") {
                let value = meta.value()?.parse::<LitStr>()?.value();
                if source.replace(value).is_some() {
                    return Err(meta.error("duplicate projection source"));
                }
                Ok(())
            } else {
                Err(meta.error("projection fields only support source"))
            }
        })?;
    }
    Ok(source)
}
