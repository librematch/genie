//! `#[derive(Diff)]` for genie-diff: a struct compares its fields by name, a tuple struct of one
//! field is that field, a tuple struct of more numbers them, an enum differs by its variant and
//! then compares the same variant's fields the same way. A field takes `#[diff(unsaved)]` (only
//! a comparison of every field reads it), `#[diff(gap = "why")]` (a known difference, which the
//! save's comparison leaves out), `#[diff(eq)]` (compared by its equality, as one value) and
//! `#[diff(path = "a.b")]` (its step in the path, for a field that moved but keeps the path it
//! had).

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, Fields, Ident, Index, parse_macro_input, parse_quote};

/// Derives `genie_diff::Diff`.
#[proc_macro_derive(Diff, attributes(diff))]
pub fn derive_diff(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: &DeriveInput) -> syn::Result<Tokens> {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    for p in generics.type_params_mut() {
        p.bounds.push(parse_quote!(::genie_diff::Diff));
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let body = match &input.data {
        Data::Struct(s) => struct_body(&s.fields)?,
        Data::Enum(e) => enum_body(&e.variants.iter().collect::<Vec<_>>())?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "Diff is derived for structs and enums, not unions",
            ));
        }
    };
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::genie_diff::Diff for #name #type_generics #where_clause {
            fn diff(&self, other: &Self, d: &mut ::genie_diff::Differ) {
                #body
            }
        }
    })
}

/// How a field is compared: its attributes.
#[derive(Clone, Default)]
struct How {
    unsaved: bool,
    gap: bool,
    eq: bool,
    /// Its step in the path instead of its name.
    path: Option<String>,
}

fn how(field: &syn::Field) -> syn::Result<How> {
    let mut how = How::default();
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("diff")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("unsaved") {
                how.unsaved = true;
                Ok(())
            } else if meta.path.is_ident("eq") {
                how.eq = true;
                Ok(())
            } else if meta.path.is_ident("gap") {
                // the reason is for the reader; the walk only needs to know it is a gap
                let reason: syn::LitStr = meta.value()?.parse()?;
                if reason.value().trim().is_empty() {
                    return Err(meta.error("a gap names its reason: `gap = \"why\"`"));
                }
                how.gap = true;
                Ok(())
            } else if meta.path.is_ident("path") {
                let path: syn::LitStr = meta.value()?.parse()?;
                let steps = path.value();
                let named = |step: &str| {
                    step.chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                        && step.chars().all(|c| c.is_alphanumeric() || c == '_')
                };
                if !steps.split('.').all(named) {
                    return Err(
                        meta.error("a path is field names joined by dots: `path = \"a.b\"`")
                    );
                }
                how.path = Some(steps);
                Ok(())
            } else {
                Err(meta.error("expected `unsaved`, `gap = \"why\"`, `eq` or `path = \"a.b\"`"))
            }
        })?;
    }
    Ok(how)
}

/// One field's comparison: `left` and `right` are references to its two values, `name` its step
/// in the path (None: the field is its value's, as a newtype's).
fn compare(how: &How, name: Option<&str>, left: &Tokens, right: &Tokens) -> Tokens {
    let name = how.path.as_deref().or(name);
    let call = match (name, how.eq) {
        (Some(n), false) => quote! { d.field(#n, #left, #right); },
        (Some(n), true) => quote! { d.equal_field(#n, #left, #right); },
        (None, false) => quote! { ::genie_diff::Diff::diff(#left, #right, d); },
        (None, true) => quote! { d.equal(#left, #right); },
    };
    match (how.unsaved, how.gap) {
        (true, _) => quote! { if d.compares_unsaved() { #call } },
        (false, true) => quote! { if d.compares_gaps() { #call } },
        (false, false) => call,
    }
}

/// The step names of `fields`: their names, their positions, or None for a single unnamed one.
fn step_names(fields: &Fields) -> Vec<Option<String>> {
    match fields {
        Fields::Named(f) => f
            .named
            .iter()
            .map(|f| f.ident.as_ref().map(|i| i.unraw().to_string()))
            .collect(),
        Fields::Unnamed(f) if f.unnamed.len() == 1 => vec![None],
        Fields::Unnamed(f) => (0..f.unnamed.len()).map(|i| Some(i.to_string())).collect(),
        Fields::Unit => Vec::new(),
    }
}

fn struct_body(fields: &Fields) -> syn::Result<Tokens> {
    let mut out = Vec::new();
    for ((i, field), name) in fields.iter().enumerate().zip(step_names(fields)) {
        let member = field.ident.as_ref().map_or_else(
            || {
                let index = Index::from(i);
                quote!(#index)
            },
            |ident| quote!(#ident),
        );
        let (left, right) = (quote!(&self.#member), quote!(&other.#member));
        out.push(compare(&how(field)?, name.as_deref(), &left, &right));
    }
    if out.is_empty() {
        out.push(quote! { let _ = (other, d); });
    }
    Ok(quote! { #(#out)* })
}

/// The bindings of a variant's fields on one side (`left_hp`, `right_0`) and its pattern.
fn bindings(variant: &syn::Variant, side: &str) -> (Vec<Ident>, Tokens) {
    let name = &variant.ident;
    let idents: Vec<Ident> = variant
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            f.ident.as_ref().map_or_else(
                || format_ident!("{side}_{i}"),
                |ident| format_ident!("{side}_{}", ident.unraw()),
            )
        })
        .collect();
    let pattern = match &variant.fields {
        Fields::Named(f) => {
            let members = f.named.iter().map(|f| &f.ident);
            quote! { Self::#name { #(#members: #idents),* } }
        }
        Fields::Unnamed(_) => quote! { Self::#name(#(#idents),*) },
        Fields::Unit => quote! { Self::#name },
    };
    (idents, pattern)
}

fn enum_body(variants: &[&syn::Variant]) -> syn::Result<Tokens> {
    if variants.is_empty() {
        return Ok(quote! { let _ = (other, d); match *self {} });
    }
    let names = variants.iter().map(|v| {
        let ident = &v.ident;
        let text = ident.unraw().to_string();
        let pattern = match &v.fields {
            Fields::Named(_) => quote! { Self::#ident { .. } },
            Fields::Unnamed(_) => quote! { Self::#ident(..) },
            Fields::Unit => quote! { Self::#ident },
        };
        quote! { #pattern => #text }
    });
    let mut arms = Vec::new();
    for v in variants.iter().filter(|v| !v.fields.is_empty()) {
        let (left, left_pattern) = bindings(v, "left");
        let (right, right_pattern) = bindings(v, "right");
        let mut body = Vec::new();
        for ((field, name), (l, r)) in v
            .fields
            .iter()
            .zip(step_names(&v.fields))
            .zip(left.iter().zip(&right))
        {
            body.push(compare(
                &how(field)?,
                name.as_deref(),
                &quote!(#l),
                &quote!(#r),
            ));
        }
        arms.push(quote! { (#left_pattern, #right_pattern) => { #(#body)* } });
    }
    let variant = Ident::new("variant", Span::call_site());
    let differs = quote! {
        let #variant = |v: &Self| -> &'static str { match v { #(#names),* } };
        if ::core::mem::discriminant(self) != ::core::mem::discriminant(other) {
            d.variants(#variant(self), #variant(other));
            return;
        }
    };
    if arms.is_empty() {
        return Ok(differs);
    }
    // the variants without fields, and the pairs the discriminant test rules out
    if arms.len() < variants.len() || 1 < variants.len() {
        arms.push(quote! { _ => {} });
    }
    Ok(quote! {
        #differs
        match (self, other) { #(#arms)* }
    })
}
