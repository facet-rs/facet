use facet_macros_parse::{ToTokens, *};
use quote::{TokenStreamExt as _, quote};

use crate::{LifetimeName, RenameRule, process_enum, process_struct};

pub fn facet_macros(input: TokenStream) -> TokenStream {
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

/// Generate the `type_name` function for the `ValueVTable`,
/// displaying realized generics if present.
pub(crate) fn generate_type_name_fn(
    type_name: &Ident,
    generics: Option<&GenericParams>,
) -> TokenStream {
    let type_name_str = type_name.to_string();

    let write_generics = generics.and_then(|generics| {
        let params = generics.params.0.iter();
        let write_each = params.filter_map(|param| match &param.value {
            // Lifetimes not shown by `std::any::type_name`, this is parity.
            GenericParam::Lifetime { .. } => None,
            GenericParam::Const { name, .. } => Some(quote! {
                write!(f, "{:?}", #name)?;
            }),
            GenericParam::Type { name, .. } => Some(quote! {
                <#name as ::facet::Facet>::SHAPE.vtable.type_name()(f, opts)?;
            }),
        });
        // TODO: is there a way to construct a DelimitedVec from an iterator?
        let mut tokens = TokenStream::new();
        tokens.append_separated(write_each, quote! { write!(f, ", ")?; });
        if tokens.is_empty() {
            None
        } else {
            Some(tokens)
        }
    });

    if let Some(write_generics) = write_generics {
        quote! {
            |f, opts| {
                write!(f, #type_name_str)?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    #write_generics
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            }
        }
    } else {
        quote! {
            |f, _opts| ::core::fmt::Write::write_str(f, #type_name_str)
        }
    }
}
