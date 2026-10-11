use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::spanned::Spanned;
use syn::{GenericArgument, PathArguments, Type};

pub(crate) fn value_type_tokens(ty: &Type) -> syn::Result<TokenStream2> {
    let ident = outer_type_ident(ty)
        .ok_or_else(|| syn::Error::new(ty.span(), "unsupported query field type in M1"))?;
    let variant = match ident.as_str() {
        "bool" => quote!(Bool),
        "i32" => quote!(I32),
        "i64" => quote!(I64),
        "f32" => quote!(F32),
        "f64" => quote!(F64),
        "String" => quote!(Text),
        "Decimal" => quote!(Decimal),
        "NaiveDate" => quote!(Date),
        "NaiveTime" => quote!(Time),
        "NaiveDateTime" => quote!(DateTime),
        "DateTime" => quote!(Timestamp),
        "Uuid" => quote!(Uuid),
        "Value" => quote!(Json),
        "Int8Range" | "PgInt8Range" => quote!(Int8Range),
        "IpNet" => quote!(Network),
        "Vec" if is_vec_u8(ty) => quote!(Bytes),
        "Vec"
            if first_type_argument(ty)
                .and_then(|ty| unwrap_query_type(ty).ok())
                .and_then(outer_type_ident)
                .as_deref()
                == Some("i64") =>
        {
            quote!(ArrayI64)
        }
        "Vec"
            if first_type_argument(ty)
                .and_then(|ty| unwrap_query_type(ty).ok())
                .and_then(outer_type_ident)
                .as_deref()
                == Some("String") =>
        {
            quote!(ArrayText)
        }
        _ => {
            return Err(syn::Error::new(
                ty.span(),
                format!("unsupported query field type {ident} in M1"),
            ));
        }
    };
    Ok(quote!(::sift_query::__private::ValueType::#variant))
}

pub(crate) fn unwrap_query_type(ty: &Type) -> syn::Result<&Type> {
    match outer_type_ident(ty).as_deref() {
        Some("Option" | "FilterOnly") => first_type_argument(ty).ok_or_else(|| {
            syn::Error::new(ty.span(), "Option and FilterOnly require one type argument")
        }),
        _ => Ok(ty),
    }
}

fn first_type_argument(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    arguments.args.iter().find_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

pub(crate) fn outer_type_ident(ty: &Type) -> Option<String> {
    let Type::Path(path) = ty else {
        return None;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn is_vec_u8(ty: &Type) -> bool {
    first_type_argument(ty)
        .and_then(outer_type_ident)
        .as_deref()
        == Some("u8")
}
