use crate::{ToTokens, *};
use proc_macro2::Delimiter;
use quote::{TokenStreamExt as _, quote};

use crate::plugin::{extract_derive_plugins, generate_plugin_chain};
use crate::{LifetimeName, RenameRule, process_enum, process_struct};

/// Recursively flattens transparent groups (groups with `Delimiter::None`) in a token stream.
///
/// When macros like `macro_rules_attribute` process metavariables like `$vis:vis`, they wrap
/// the captured tokens in a `Group` with `Delimiter::None`. This function unwraps such groups
/// so that the inner tokens can be parsed normally.
///
/// For example, if a `$vis:vis` captures `pub`, the token stream might contain:
/// ```text
/// Group { delimiter: None, stream: TokenStream [Ident { ident: "pub" }] }
/// ```
///
/// After flattening, this becomes just:
/// ```text
/// Ident { ident: "pub" }
/// ```
fn flatten_transparent_groups(input: TokenStream) -> TokenStream {
    input
        .into_iter()
        .flat_map(|tt| match tt {
            TokenTree::Group(group) if group.delimiter() == Delimiter::None => {
                // Recursively flatten the contents of the transparent group
                flatten_transparent_groups(group.stream())
            }
            TokenTree::Group(group) => {
                // For non-transparent groups, recursively flatten their contents
                // but keep the group structure
                let flattened_stream = flatten_transparent_groups(group.stream());
                let mut new_group = proc_macro2::Group::new(group.delimiter(), flattened_stream);
                new_group.set_span(group.span());
                std::iter::once(TokenTree::Group(new_group)).collect()
            }
            other => std::iter::once(other).collect(),
        })
        .collect()
}

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
    // Flatten transparent groups (Delimiter::None) before parsing.
    // This handles macros like `macro_rules_attribute` that wrap metavariables
    // like `$vis:vis` in transparent groups.
    let input = flatten_transparent_groups(input);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flatten_transparent_groups_simple() {
        // Test that regular tokens pass through unchanged
        let input: TokenStream = quote::quote! { pub struct Foo; };
        let flattened = flatten_transparent_groups(input.clone());
        assert_eq!(flattened.to_string(), input.to_string());
    }

    #[test]
    fn test_flatten_transparent_groups_with_none_delimiter() {
        // Simulate what macro_rules_attribute does with $vis:vis
        // Create a Group with None delimiter containing "pub"
        let pub_token: TokenStream = quote::quote! { pub };
        let none_group = proc_macro2::Group::new(proc_macro2::Delimiter::None, pub_token.clone());

        let mut input = TokenStream::new();
        input.extend(std::iter::once(TokenTree::Group(none_group)));
        input.extend(quote::quote! { struct Cat; });

        let flattened = flatten_transparent_groups(input);

        // After flattening, should be "pub struct Cat;"
        let expected: TokenStream = quote::quote! { pub struct Cat; };
        assert_eq!(flattened.to_string(), expected.to_string());
    }

    #[test]
    fn test_flatten_transparent_groups_preserves_braces() {
        // Test that normal braces are preserved
        let input: TokenStream = quote::quote! { struct Foo { x: u32 } };
        let flattened = flatten_transparent_groups(input.clone());
        assert_eq!(flattened.to_string(), input.to_string());
    }

    #[test]
    fn test_flatten_transparent_groups_nested() {
        // Test nested transparent groups
        let inner: TokenStream = quote::quote! { pub };
        let inner_group = proc_macro2::Group::new(proc_macro2::Delimiter::None, inner);
        let outer_stream: TokenStream = std::iter::once(TokenTree::Group(inner_group)).collect();
        let outer_group = proc_macro2::Group::new(proc_macro2::Delimiter::None, outer_stream);

        let mut input = TokenStream::new();
        input.extend(std::iter::once(TokenTree::Group(outer_group)));
        input.extend(quote::quote! { struct Cat; });

        let flattened = flatten_transparent_groups(input);

        let expected: TokenStream = quote::quote! { pub struct Cat; };
        assert_eq!(flattened.to_string(), expected.to_string());
    }

    #[test]
    fn test_flatten_transparent_groups_inside_braces() {
        // Test that transparent groups inside braces are also flattened
        let pub_token: TokenStream = quote::quote! { pub };
        let none_group = proc_macro2::Group::new(proc_macro2::Delimiter::None, pub_token);

        let mut brace_content = TokenStream::new();
        brace_content.extend(std::iter::once(TokenTree::Group(none_group)));
        brace_content.extend(quote::quote! { x: u32 });

        let brace_group = proc_macro2::Group::new(proc_macro2::Delimiter::Brace, brace_content);

        let mut input: TokenStream = quote::quote! { struct Foo };
        input.extend(std::iter::once(TokenTree::Group(brace_group)));

        let flattened = flatten_transparent_groups(input);

        let expected: TokenStream = quote::quote! { struct Foo { pub x: u32 } };
        assert_eq!(flattened.to_string(), expected.to_string());
    }

    #[test]
    fn test_parse_struct_with_transparent_group_visibility() {
        // Simulate the exact scenario from the issue: $vis:vis wrapped in None-delimited group
        let pub_token: TokenStream = quote::quote! { pub };
        let none_group = proc_macro2::Group::new(proc_macro2::Delimiter::None, pub_token);

        let mut input = TokenStream::new();
        input.extend(std::iter::once(TokenTree::Group(none_group)));
        input.extend(quote::quote! { struct Cat; });

        // This should now succeed after flattening
        let flattened = flatten_transparent_groups(input);
        let mut iter = flattened.to_token_iter();
        let result = iter.parse::<Cons<AdtDecl, EndOfStream>>();

        assert!(
            result.is_ok(),
            "Parsing should succeed after flattening transparent groups"
        );
    }
}
