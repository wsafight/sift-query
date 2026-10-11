use super::syntax::parse_string;
use syn::Attribute;

pub(crate) fn sqlx_rename(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut rename = None;
    for attribute in attrs.iter().filter(|attr| attr.path().is_ident("sqlx")) {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = parse_string(meta.value()?)?;
                if rename.replace(value).is_some() {
                    return Err(meta.error("duplicate sqlx rename"));
                }
            }
            Ok(())
        })?;
    }
    Ok(rename)
}

pub(crate) fn has_sqlx_skip_or_default(attrs: &[Attribute]) -> syn::Result<bool> {
    let mut found = false;
    for attribute in attrs.iter().filter(|attr| attr.path().is_ident("sqlx")) {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") || meta.path.is_ident("default") {
                found = true;
            }
            Ok(())
        })?;
    }
    Ok(found)
}

pub(crate) fn reject_sqlx_rename_all(attrs: &[Attribute]) -> syn::Result<()> {
    for attribute in attrs.iter().filter(|attr| attr.path().is_ident("sqlx")) {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") || meta.path.is_ident("flatten") {
                return Err(
                    meta.error("sqlx rename_all/flatten is not supported by SearchEntity M1")
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}

pub(crate) fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs
        .iter()
        .any(|attribute| attribute.path().is_ident(name))
}
