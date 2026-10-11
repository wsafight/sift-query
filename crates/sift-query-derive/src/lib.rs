//! Derive macros for `SiftQuery` entities.

mod attrs;
mod codegen;
mod mapping;
mod naming;
mod types;
mod validation;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Declares entities from reusable field fragments and named mapping presets.
#[proc_macro]
pub fn search_entities(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as mapping::Declarations);
    input
        .expand()
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(
    SearchEntity,
    attributes(search_entity, search_field, search_ignore, sqlx)
)]
pub fn derive_search_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    codegen::entity::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(
    SearchProjection,
    attributes(search_projection, search_field, search_ignore, sqlx)
)]
pub fn derive_search_projection(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    codegen::projection::expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
