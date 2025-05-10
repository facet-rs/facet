use facet_derive_parse::{ToTokens, *};
use quote::quote;

use crate::{LifetimeName, RenameRule, process_enum, process_struct};

pub fn facet_derive(input: TokenStream) -> TokenStream {
    let mut i = input.to_token_iter();

    // Parse as TypeDecl
    match i.parse::<Cons<AdtDecl, EndOfStream>>() {
        Ok(it) => match it.first {
            AdtDecl::Struct(parsed) => process_struct::process_struct(parsed),
            AdtDecl::Enum(parsed) => process_enum::process_enum(parsed),
        },
        Err(err) => {
            panic!(
                "Could not parse type declaration: {}\nError: {}",
                input, err
            );
        }
    }
}

/// Generate a static declaration that exports the crate
pub(crate) fn generate_static_decl(type_name: &Ident) -> TokenStream {
    let type_name_str = type_name.to_string();
    let screaming_snake_name = RenameRule::ScreamingSnakeCase.apply(&type_name_str);

    let static_name_ident = quote::format_ident!("{}_SHAPE", screaming_snake_name);

    quote! {
        #[used]
        static #static_name_ident: &'static ::facet::Shape = <#type_name as ::facet::Facet>::SHAPE;
    }
}

pub(crate) fn build_where_clauses(
    where_clauses: Option<&WhereClauses>,
    generics: Option<&GenericParams>,
) -> TokenStream {
    let mut where_clause_tokens = TokenStream::new();
    let mut has_clauses = false;

    if let Some(wc) = where_clauses {
        for c in &wc.clauses.0 {
            if has_clauses {
                where_clause_tokens.extend(quote! { , });
            }
            where_clause_tokens.extend(c.value.to_token_stream());
            has_clauses = true;
        }
    }

    if let Some(generics) = generics {
        for p in &generics.params.0 {
            match &p.value {
                GenericParam::Lifetime { name, .. } => {
                    let facet_lifetime = LifetimeName(quote::format_ident!("{}", "__facet"));
                    let lifetime = LifetimeName(name.name.clone());
                    if has_clauses {
                        where_clause_tokens.extend(quote! { , });
                    }
                    where_clause_tokens
                        .extend(quote! { #lifetime: #facet_lifetime, #facet_lifetime: #lifetime });

                    has_clauses = true;
                }
                GenericParam::Const { .. } => {
                    // ignore for now
                }
                GenericParam::Type { name, .. } => {
                    if has_clauses {
                        where_clause_tokens.extend(quote! { , });
                    }
                    where_clause_tokens.extend(quote! { #name: ::facet::Facet<'__facet> });
                    has_clauses = true;
                }
            }
        }
    }

    if !has_clauses {
        quote! {}
    } else {
        quote! { where #where_clause_tokens }
    }
}

pub(crate) fn build_type_params(generics: Option<&GenericParams>) -> TokenStream {
    let mut type_params = Vec::new();
    if let Some(generics) = generics {
        for p in &generics.params.0 {
            match &p.value {
                GenericParam::Lifetime { .. } => {
                    // ignore for now
                }
                GenericParam::Const { .. } => {
                    // ignore for now
                }
                GenericParam::Type { name, .. } => {
                    let name_str = name.to_string();
                    type_params.push(quote! {
                        ::facet::TypeParam {
                            name: #name_str,
                            shape: || <#name as ::facet::Facet>::SHAPE
                        }
                    });
                }
            }
        }
    }

    if type_params.is_empty() {
        quote! {}
    } else {
        quote! { .type_params(&[#(#type_params),*]) }
    }
}
