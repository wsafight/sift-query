//! Declaration validation shared by entity and projection generation.
pub(crate) fn validate_sql_fragment(
    value: &str,
    label: &str,
    span: proc_macro2::Span,
) -> syn::Result<()> {
    if value.trim().is_empty()
        || value.contains(';')
        || value.contains("--")
        || value.contains("/*")
        || value.contains('$')
    {
        Err(syn::Error::new(
            span,
            format!("{label} contains a forbidden SQL token"),
        ))
    } else {
        Ok(())
    }
}
