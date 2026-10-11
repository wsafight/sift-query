//! Compile-time composition of reusable fields and named mapping attributes.
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeMap;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Field, Ident, Token, Visibility};

mod keywords {
    syn::custom_keyword!(fields);
    syn::custom_keyword!(mapping);
    syn::custom_keyword!(include);
}

enum Part {
    Field(Box<Field>),
    Include(Ident),
}
struct Entity {
    attrs: Vec<Attribute>,
    visibility: Visibility,
    name: Ident,
    parts: Vec<Part>,
}
pub(crate) struct Declarations {
    fragments: BTreeMap<String, Vec<Part>>,
    mappings: BTreeMap<String, Vec<Attribute>>,
    entities: Vec<Entity>,
}
fn parts(input: ParseStream<'_>) -> syn::Result<Vec<Part>> {
    let mut output = Vec::new();
    while !input.is_empty() {
        if input.peek(keywords::include) {
            input.parse::<keywords::include>()?;
            output.push(Part::Include(input.parse()?));
            input.parse::<Token![;]>()?;
        } else {
            output.push(Part::Field(Box::new(Field::parse_named(input)?)));
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
    }
    Ok(output)
}
impl Parse for Declarations {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut result = Self {
            fragments: BTreeMap::new(),
            mappings: BTreeMap::new(),
            entities: Vec::new(),
        };
        while !input.is_empty() {
            if input.peek(keywords::fields) {
                input.parse::<keywords::fields>()?;
                let name: Ident = input.parse()?;
                let content;
                syn::braced!(content in input);
                if result
                    .fragments
                    .insert(name.to_string(), parts(&content)?)
                    .is_some()
                {
                    return Err(syn::Error::new(name.span(), "duplicate field fragment"));
                }
            } else if input.peek(keywords::mapping) {
                input.parse::<keywords::mapping>()?;
                let name: Ident = input.parse()?;
                let content;
                syn::braced!(content in input);
                let attrs = content.call(Attribute::parse_outer)?;
                if !content.is_empty()
                    || attrs
                        .iter()
                        .any(|attr| !attr.path().is_ident("search_entity"))
                {
                    return Err(syn::Error::new(
                        name.span(),
                        "mapping accepts only search_entity attributes",
                    ));
                }
                if result.mappings.insert(name.to_string(), attrs).is_some() {
                    return Err(syn::Error::new(name.span(), "duplicate named mapping"));
                }
            } else {
                let attrs = input.call(Attribute::parse_outer)?;
                let visibility = input.parse()?;
                input.parse::<Token![struct]>()?;
                let name = input.parse()?;
                let content;
                syn::braced!(content in input);
                result.entities.push(Entity {
                    attrs,
                    visibility,
                    name,
                    parts: parts(&content)?,
                });
            }
        }
        Ok(result)
    }
}

impl Declarations {
    pub(crate) fn expand(&self) -> syn::Result<TokenStream> {
        let mut output = TokenStream::new();
        for entity in &self.entities {
            // Introduce derives before their helper attributes (Rust 1.99).
            let mut attributes = entity
                .attrs
                .iter()
                .filter(|attr| {
                    !attr.path().is_ident("search_mapping")
                        && !attr.path().is_ident("search_entity")
                })
                .cloned()
                .collect::<Vec<_>>();
            for attr in &entity.attrs {
                if attr.path().is_ident("search_mapping") {
                    let name = attr.parse_args::<Ident>()?;
                    attributes.extend(
                        self.mappings
                            .get(&name.to_string())
                            .ok_or_else(|| syn::Error::new(name.span(), "unknown named mapping"))?
                            .clone(),
                    );
                }
            }
            // Explicit entity options follow presets and take precedence.
            attributes.extend(
                entity
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("search_entity"))
                    .cloned(),
            );
            let mut fields = Vec::new();
            self.expand_fields(&entity.parts, &mut Vec::new(), &mut fields)?;
            let visibility = &entity.visibility;
            let name = &entity.name;
            output.extend(quote! { #(#attributes)* #visibility struct #name { #(#fields),* } });
        }
        Ok(output)
    }
    fn expand_fields(
        &self,
        parts: &[Part],
        stack: &mut Vec<String>,
        output: &mut Vec<Field>,
    ) -> syn::Result<()> {
        for part in parts {
            match part {
                Part::Field(field) => output.push((**field).clone()),
                Part::Include(name) => {
                    let key = name.to_string();
                    if stack.contains(&key) || stack.len() >= 16 {
                        return Err(syn::Error::new(
                            name.span(),
                            "cyclic or too deep field fragment inclusion",
                        ));
                    }
                    let fields = self
                        .fragments
                        .get(&key)
                        .ok_or_else(|| syn::Error::new(name.span(), "unknown field fragment"))?;
                    stack.push(key);
                    self.expand_fields(fields, stack, output)?;
                    stack.pop();
                }
            }
        }
        Ok(())
    }
}
