//! # Facet Plugin System Proof of Concept
//!
//! This demonstrates the macro expansion handshake pattern for facet plugins.
//!
//! ## The Flow
//!
//! 1. User writes `#[derive(FacetPoc)]` with `#[facet_poc(display)]`
//! 2. `derive(FacetPoc)` does NOT parse the struct - it just chains to plugins
//! 3. Each plugin adds its "template" and forwards to the next
//! 4. `__facet_finalize!` parses once and generates all code
//!
//! For this POC, we simulate the templating with direct codegen.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    DeriveInput, Ident, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

/// The main derive macro - this is what users write `#[derive(FacetPoc)]` for.
///
/// Instead of parsing the struct, it:
/// 1. Looks for `#[facet_poc(...)]` attributes to find enabled plugins
/// 2. Chains to the first plugin's `__facet_invoke!`
/// 3. The chain eventually reaches `__facet_finalize!`
#[proc_macro_derive(FacetPoc, attributes(facet_poc))]
pub fn derive_facet_poc(input: TokenStream) -> TokenStream {
    let input_tokens = TokenStream2::from(input.clone());
    let input = parse_macro_input!(input as DeriveInput);

    // Find all #[facet_poc(...)] attributes
    let mut plugins = Vec::new();
    for attr in &input.attrs {
        if attr.path().is_ident("facet_poc") {
            // Parse the contents, e.g., `display` or `display, error`
            let _ = attr.parse_nested_meta(|meta| {
                if let Some(ident) = meta.path.get_ident() {
                    plugins.push(ident.to_string());
                }
                Ok(())
            });
        }
    }

    // Build the chain: each plugin wraps the next, with finalize at the end
    // We process plugins left-to-right: display wraps (debug wraps finalize)

    // Validate plugins first
    for plugin in &plugins {
        match plugin.as_str() {
            "display" | "debug" => {}
            other => {
                return syn::Error::new_spanned(&input.ident, format!("Unknown plugin: {other}"))
                    .to_compile_error()
                    .into();
            }
        }
    }

    if plugins.is_empty() {
        // No plugins - go straight to finalize
        return quote! {
            ::facet_plugin_poc::__facet_finalize! {
                @tokens { #input_tokens }
                @plugins { }
            }
        }
        .into();
    }

    // Build the chain differently: each plugin gets @next as just a path,
    // and @plugins accumulates as we go.
    //
    // For [display, debug], we want:
    // display_invoke! { @tokens {...} @next_plugin { debug } @remaining { } @plugins { } }
    //   -> debug_invoke! { @tokens {...} @next_plugin { } @remaining { } @plugins { "display" } }
    //     -> finalize! { @tokens {...} @plugins { "display", "debug" } }
    //
    // Actually, simpler approach: convert plugin list to a path list and iterate.

    let plugin_paths: Vec<_> = plugins
        .iter()
        .map(|p| match p.as_str() {
            "display" => quote! { ::facet_plugin_poc::__plugin_display_invoke },
            "debug" => quote! { ::facet_plugin_poc::__plugin_debug_invoke },
            _ => unreachable!(),
        })
        .collect();

    // Start the chain with the first plugin
    let first = &plugin_paths[0];
    let rest: Vec<_> = plugin_paths[1..].iter().collect();

    // Pack remaining plugins as a token list
    let remaining = if rest.is_empty() {
        quote! {}
    } else {
        quote! { #(#rest),* }
    };

    quote! {
        #first! {
            @tokens { #input_tokens }
            @remaining { #remaining }
            @plugins { }
        }
    }
    .into()
}

// Helper struct for parsing the internal macro format
struct PluginInvoke {
    tokens: TokenStream2,
    remaining: Vec<TokenStream2>, // Remaining plugin paths to invoke
    plugins: Vec<String>,         // Accumulated plugin names
}

impl Parse for PluginInvoke {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut tokens = None;
        let mut remaining = Vec::new();
        let mut plugins = Vec::new();

        while !input.is_empty() {
            let lookahead = input.lookahead1();
            if lookahead.peek(Token![@]) {
                input.parse::<Token![@]>()?;
                let keyword: Ident = input.parse()?;

                let content;
                syn::braced!(content in input);

                match keyword.to_string().as_str() {
                    "tokens" => {
                        tokens = Some(content.parse()?);
                    }
                    "remaining" => {
                        // Parse comma-separated paths
                        while !content.is_empty() {
                            let path: syn::Path = content.parse()?;
                            remaining.push(quote! { #path });
                            if content.peek(Token![,]) {
                                content.parse::<Token![,]>()?;
                            } else {
                                break;
                            }
                        }
                    }
                    "plugins" => {
                        // Parse comma-separated string literals
                        while !content.is_empty() {
                            if content.peek(LitStr) {
                                let lit: LitStr = content.parse()?;
                                plugins.push(lit.value());
                                if content.peek(Token![,]) {
                                    content.parse::<Token![,]>()?;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    _ => {
                        return Err(syn::Error::new(keyword.span(), "unknown section"));
                    }
                }
            } else {
                return Err(lookahead.error());
            }
        }

        Ok(PluginInvoke {
            tokens: tokens.ok_or_else(|| input.error("missing @tokens"))?,
            remaining,
            plugins,
        })
    }
}

/// Helper to generate the next step in the plugin chain
fn chain_next(invoke: PluginInvoke, plugin_name: &str) -> TokenStream2 {
    let tokens = &invoke.tokens;

    // Add this plugin to the accumulated list
    let mut plugins = invoke.plugins;
    plugins.push(plugin_name.to_string());

    let plugin_strings: Vec<_> = plugins
        .iter()
        .map(|s| LitStr::new(s, proc_macro2::Span::call_site()))
        .collect();

    if invoke.remaining.is_empty() {
        // No more plugins - go to finalize
        quote! {
            ::facet_plugin_poc::__facet_finalize! {
                @tokens { #tokens }
                @plugins { #(#plugin_strings,)* }
            }
        }
    } else {
        // More plugins to process - invoke the next one
        let next_plugin = &invoke.remaining[0];
        let rest: Vec<_> = invoke.remaining[1..].iter().collect();

        let remaining = if rest.is_empty() {
            quote! {}
        } else {
            quote! { #(#rest),* }
        };

        quote! {
            #next_plugin! {
                @tokens { #tokens }
                @remaining { #remaining }
                @plugins { #(#plugin_strings,)* }
            }
        }
    }
}

/// Plugin: display
///
/// This simulates a plugin that generates `impl Display`.
/// In the real design, this would add a template string to the @plugins accumulator.
#[proc_macro]
pub fn __plugin_display_invoke(input: TokenStream) -> TokenStream {
    let invoke = parse_macro_input!(input as PluginInvoke);
    chain_next(invoke, "display").into()
}

/// Plugin: debug
///
/// Another example plugin that generates `impl Debug`.
#[proc_macro]
pub fn __plugin_debug_invoke(input: TokenStream) -> TokenStream {
    let invoke = parse_macro_input!(input as PluginInvoke);
    chain_next(invoke, "debug").into()
}

/// The finalizer - this is where parsing actually happens!
///
/// It receives:
/// - @tokens: the raw token stream of the original struct/enum
/// - @plugins: the accumulated list of plugins that want code generated
///
/// It parses the tokens ONCE and generates all requested code.
#[proc_macro]
pub fn __facet_finalize(input: TokenStream) -> TokenStream {
    let invoke = parse_macro_input!(input as PluginInvoke);

    // NOW we parse the actual struct - this is the one and only parse!
    let item: DeriveInput = match syn::parse2(invoke.tokens) {
        Ok(item) => item,
        Err(e) => return e.to_compile_error().into(),
    };

    let name = &item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let mut generated = TokenStream2::new();

    // Generate code for each plugin
    for plugin in &invoke.plugins {
        match plugin.as_str() {
            "display" => {
                // Generate Display impl based on doc comments
                let display_impl = generate_display(&item);
                generated.extend(display_impl);
            }
            "debug" => {
                // Generate Debug impl
                let debug_impl = quote! {
                    impl #impl_generics ::core::fmt::Debug for #name #ty_generics #where_clause {
                        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                            // Simple debug impl - in real version would be more sophisticated
                            write!(f, stringify!(#name))
                        }
                    }
                };
                generated.extend(debug_impl);
            }
            _ => {}
        }
    }

    generated.into()
}

/// Generate Display impl from doc comments (displaydoc-style)
fn generate_display(item: &DeriveInput) -> TokenStream2 {
    let name = &item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    match &item.data {
        syn::Data::Enum(data) => {
            let match_arms: Vec<_> = data
                .variants
                .iter()
                .map(|v| {
                    let variant_name = &v.ident;

                    // Extract doc comment
                    let doc = v
                        .attrs
                        .iter()
                        .filter_map(|attr| {
                            if attr.path().is_ident("doc") {
                                attr.meta.require_name_value().ok().and_then(|nv| {
                                    if let syn::Expr::Lit(syn::ExprLit {
                                        lit: syn::Lit::Str(s),
                                        ..
                                    }) = &nv.value
                                    {
                                        Some(s.value().trim().to_string())
                                    } else {
                                        None
                                    }
                                })
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");

                    let format_str = if doc.is_empty() {
                        variant_name.to_string()
                    } else {
                        doc
                    };

                    // Generate pattern based on fields
                    let (pattern, format_args) = match &v.fields {
                        syn::Fields::Unit => (quote! {}, quote! {}),
                        syn::Fields::Unnamed(fields) => {
                            // For tuple variants, bind to v0, v1, etc.
                            let field_names: Vec<_> = (0..fields.unnamed.len())
                                .map(|i| Ident::new(&format!("v{i}"), v.ident.span()))
                                .collect();
                            // In Rust 2024, match ergonomics means we don't need `ref`
                            let pattern = quote! { ( #(#field_names),* ) };

                            // Check if format string has {0}, {1} etc - if not, don't pass args
                            // For simplicity, only pass args if there are positional placeholders
                            let has_positional = format_str.contains("{0}");
                            let args = if has_positional {
                                let args: Vec<_> =
                                    field_names.iter().map(|n| quote! { , #n }).collect();
                                quote! { #(#args)* }
                            } else {
                                quote! {}
                            };
                            (pattern, args)
                        }
                        syn::Fields::Named(fields) => {
                            // For struct variants, use field names directly
                            // Named placeholders like {expected} will be looked up from local variables
                            let field_names: Vec<_> = fields
                                .named
                                .iter()
                                .filter_map(|f| f.ident.as_ref())
                                .collect();
                            // In Rust 2024, match ergonomics means we don't need `ref`
                            let pattern = quote! { { #(#field_names),* } };
                            // Don't pass additional args - the format string uses {name} placeholders
                            // which look up variables in scope
                            (pattern, quote! {})
                        }
                    };

                    quote! {
                        Self::#variant_name #pattern => write!(f, #format_str #format_args)
                    }
                })
                .collect();

            quote! {
                impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        match self {
                            #(#match_arms),*
                        }
                    }
                }
            }
        }
        syn::Data::Struct(_) => {
            // Extract doc comment from struct
            let doc = item
                .attrs
                .iter()
                .filter_map(|attr| {
                    if attr.path().is_ident("doc") {
                        attr.meta.require_name_value().ok().and_then(|nv| {
                            if let syn::Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Str(s),
                                ..
                            }) = &nv.value
                            {
                                Some(s.value().trim().to_string())
                            } else {
                                None
                            }
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            let format_str = if doc.is_empty() {
                name.to_string()
            } else {
                doc
            };

            quote! {
                impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        write!(f, #format_str)
                    }
                }
            }
        }
        syn::Data::Union(_) => {
            syn::Error::new_spanned(name, "unions are not supported").to_compile_error()
        }
    }
}
