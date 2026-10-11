use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::ParseStream;
use syn::{LitBool, LitStr, Token};

pub(crate) fn parse_string(input: ParseStream<'_>) -> syn::Result<String> {
    Ok(input.parse::<LitStr>()?.value())
}

pub(crate) fn parse_bool_or_flag(input: ParseStream<'_>, flag_value: bool) -> syn::Result<bool> {
    if input.peek(Token![=]) {
        Ok(input
            .parse::<Token![=]>()
            .and_then(|_| input.parse::<LitBool>())?
            .value)
    } else {
        Ok(flag_value)
    }
}

pub(crate) fn option_string_tokens(value: Option<&str>, span: proc_macro2::Span) -> TokenStream2 {
    value.map_or_else(
        || quote!(None),
        |value| {
            let value = LitStr::new(value, span);
            quote!(Some(#value))
        },
    )
}
