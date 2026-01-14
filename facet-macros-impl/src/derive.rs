use crate::{ToTokens, *};
use quote::{TokenStreamExt as _, quote};

use crate::plugin::{extract_derive_plugins, generate_plugin_chain};
use crate::{LifetimeName, RenameRule, process_enum, process_struct};

/// Generate a static declaration that pre-evaluates `<T as Facet>::SHAPE`.
/// Only emitted in release builds to avoid slowing down debug compile times.
/// Skipped for generic types since we can't create a static for an unmonomorphized type.
pub(crate) fn generate_static_decl(
    type_name: &Ident,
    facet_crate: &TokenStream,
    has_type_or_const_generics: bool,
) -> TokenStream {
    // Can't generate a static for generic types - the type parameters aren't concrete
    if has_type_or_const_generics {
        return quote! {};
    }

    let type_name_str = type_name.to_string();
    let screaming_snake_name = RenameRule::ScreamingSnakeCase.apply(&type_name_str);

    let static_name_ident = quote::format_ident!("{}_SHAPE", screaming_snake_name);

    quote! {
        #[cfg(not(debug_assertions))]
        static #static_name_ident: &'static #facet_crate::Shape = <#type_name as #facet_crate::Facet>::SHAPE;
    }
}

/// Main entry point for the `#[derive(Facet)]` macro. Parses type declarations and generates Facet trait implementations.
///
/// If `#[facet(derive(...))]` is present, chains to plugins before generating.
pub fn facet_macros(input: TokenStream) -> TokenStream {
    let mut i = input.clone().to_token_iter();

    // Parse as TypeDecl
    match i.parse::<Cons<AdtDecl, EndOfStream>>() {
        Ok(it) => {
            // Extract attributes to check for plugins
            let attrs = match &it.first {
                AdtDecl::Struct(s) => &s.attributes,
                AdtDecl::Enum(e) => &e.attributes,
            };

            // Check for #[facet(derive(...))] plugins
            let plugins = extract_derive_plugins(attrs);

            if !plugins.is_empty() {
                // Get the facet crate path from attributes
                let facet_crate = {
                    let parsed_attrs = PAttrs::parse(attrs);
                    parsed_attrs.facet_crate()
                };

                // Generate plugin chain
                if let Some(chain) = generate_plugin_chain(&input, &plugins, &facet_crate) {
                    return chain;
                }
            }

            // No plugins, proceed with normal codegen
            match it.first {
                AdtDecl::Struct(parsed) => process_struct::process_struct(parsed),
                AdtDecl::Enum(parsed) => process_enum::process_enum(parsed),
            }
        }
        Err(err) => {
            panic!("Could not parse type declaration: {input}\nError: {err}");
        }
    }
}

pub(crate) fn build_where_clauses(
    where_clauses: Option<&WhereClauses>,
    generics: Option<&GenericParams>,
    opaque: bool,
    facet_crate: &TokenStream,
    custom_bounds: &[TokenStream],
) -> TokenStream {
    let mut where_clause_tokens = TokenStream::new();
    let mut has_clauses = false;

    if let Some(wc) = where_clauses {
        for c in wc.clauses.iter() {
            if has_clauses {
                where_clause_tokens.extend(quote! { , });
            }
            where_clause_tokens.extend(c.value.to_token_stream());
            has_clauses = true;
        }
    }

    if let Some(generics) = generics {
        for p in generics.params.iter() {
            match &p.value {
                GenericParam::Lifetime { name, .. } => {
                    let facet_lifetime = LifetimeName(quote::format_ident!("{}", "ʄ"));
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
                    // Only specify lifetime bound for opaque containers
                    if opaque {
                        where_clause_tokens.extend(quote! { #name: 'ʄ });
                    } else {
                        where_clause_tokens.extend(quote! { #name: #facet_crate::Facet<'ʄ> });
                    }
                    has_clauses = true;
                }
            }
        }
    }

    // Add custom bounds from #[facet(bound = "...")]
    for bound in custom_bounds {
        if has_clauses {
            where_clause_tokens.extend(quote! { , });
        }
        where_clause_tokens.extend(bound.clone());
        has_clauses = true;
    }

    if !has_clauses {
        quote! {}
    } else {
        quote! { where #where_clause_tokens }
    }
}

/// Build the `.type_params(...)` builder call, returning empty if no type params.
pub(crate) fn build_type_params_call(
    generics: Option<&GenericParams>,
    opaque: bool,
    facet_crate: &TokenStream,
) -> TokenStream {
    if opaque {
        return quote! {};
    }

    let mut type_params = Vec::new();
    if let Some(generics) = generics {
        for p in generics.params.iter() {
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
                        #facet_crate::TypeParam {
                            name: #name_str,
                            shape: <#name as #facet_crate::Facet>::SHAPE
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
    opaque: bool,
    facet_crate: &TokenStream,
) -> TokenStream {
    let type_name_str = type_name.to_string();

    let write_generics = (!opaque)
        .then_some(generics)
        .flatten()
        .and_then(|generics| {
            let params = generics.params.iter();
            let write_each = params.filter_map(|param| match &param.value {
                // Lifetimes not shown by `std::any::type_name`, this is parity.
                GenericParam::Lifetime { .. } => None,
                GenericParam::Const { name, .. } => Some(quote! {
                    write!(f, "{:?}", #name)?;
                }),
                GenericParam::Type { name, .. } => Some(quote! {
                    <#name as #facet_crate::Facet>::SHAPE.write_type_name(f, opts)?;
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

    match write_generics {
        Some(write_generics) => {
            quote! {
                |_shape, f, opts| {
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
        }
        None => quote! { |_shape, f, _opts| ::core::fmt::Write::write_str(f, #type_name_str) },
    }
}
