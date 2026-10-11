use super::FieldConfig;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn parse_order_list(
    input: Option<&str>,
    label: &str,
    fields: &BTreeMap<String, &FieldConfig>,
    span: proc_macro2::Span,
) -> syn::Result<Vec<(u16, bool)>> {
    let Some(input) = input else {
        return Ok(Vec::new());
    };
    if input.is_empty() {
        return Err(syn::Error::new(span, format!("{label} cannot be empty")));
    }
    let mut output = Vec::new();
    let mut seen = BTreeSet::new();
    for item in input.split(',') {
        let (name, direction) = item.split_once(':').ok_or_else(|| {
            syn::Error::new(span, format!("{label} item {item} has no direction"))
        })?;
        let field = fields.get(name).ok_or_else(|| {
            syn::Error::new(span, format!("{label} references unknown field {name}"))
        })?;
        if !field.sortable {
            return Err(syn::Error::new(
                span,
                format!("{label} field {name} is not sortable"),
            ));
        }
        if !seen.insert(name) {
            return Err(syn::Error::new(
                span,
                format!("{label} contains duplicate field {name}"),
            ));
        }
        let ascending = match direction {
            "asc" => true,
            "desc" => false,
            _ => {
                return Err(syn::Error::new(
                    span,
                    format!("{label} has invalid direction {direction}"),
                ));
            }
        };
        output.push((field.index, ascending));
    }
    Ok(output)
}

pub(crate) fn order_tokens(order: &[(u16, bool)]) -> Vec<TokenStream2> {
    order
        .iter()
        .map(|(field, ascending)| {
            let direction = if *ascending {
                quote!(::sift_query::__private::SortOrder::Asc)
            } else {
                quote!(::sift_query::__private::SortOrder::Desc)
            };
            quote! {
                ::sift_query::__private::OrderMeta {
                    field: ::sift_query::__private::FieldId(#field),
                    direction: #direction,
                }
            }
        })
        .collect()
}
