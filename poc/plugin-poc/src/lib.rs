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
//! Now using the proper facet-macro-* crates for parsing and templating.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

// Use the new crates
use facet_macro_parse::{IdentOrLiteral, PType, PVariantKind, parse_type};

// Import unsynn for the internal protocol parsing
use unsynn::{At, BraceGroupContaining, Comma, Ident, Literal, keyword, unsynn};
use unsynn::{IParse, ToTokenIter};

// Define keywords we need
keyword! {
    KFacetPoc = "facet_poc";
}

// Grammar for parsing the internal macro invocation format
unsynn! {
    /// Section marker like `@tokens`, `@remaining`, `@plugins`
    struct SectionMarker {
        at: At,
        name: Ident,
    }

    /// A braced section like `@tokens { ... }`
    struct BracedSection {
        marker: SectionMarker,
        content: BraceGroupContaining<TokenStream2>,
    }

    /// Attribute like `#[...]`
    struct Attribute {
        pound: unsynn::Pound,
        body: unsynn::BracketGroupContaining<TokenStream2>,
    }
}

/// Parsed plugin invocation
struct PluginInvoke {
    tokens: TokenStream2,
    remaining: Vec<TokenStream2>,
    plugins: Vec<String>,
}

impl PluginInvoke {
    fn parse(input: TokenStream2) -> Result<Self, String> {
        let mut iter = input.to_token_iter();
        let mut tokens = None;
        let mut remaining = Vec::new();
        let mut plugins = Vec::new();

        while let Ok(section) = iter.parse::<BracedSection>() {
            let name = section.marker.name.to_string();
            match name.as_str() {
                "tokens" => {
                    tokens = Some(section.content.content);
                }
                "remaining" => {
                    let content = section.content.content;
                    if !content.is_empty() {
                        let mut current = TokenStream2::new();
                        for tt in content {
                            if let proc_macro2::TokenTree::Punct(p) = &tt {
                                if p.as_char() == ',' {
                                    if !current.is_empty() {
                                        remaining.push(current);
                                        current = TokenStream2::new();
                                    }
                                    continue;
                                }
                            }
                            current.extend(std::iter::once(tt));
                        }
                        if !current.is_empty() {
                            remaining.push(current);
                        }
                    }
                }
                "plugins" => {
                    let content = section.content.content;
                    let mut inner = content.to_token_iter();
                    while let Ok(lit) = inner.parse::<Literal>() {
                        let s = lit.to_string();
                        let unquoted = s.trim_matches('"');
                        plugins.push(unquoted.to_string());
                        let _ = inner.parse::<Comma>();
                    }
                }
                _ => {
                    return Err(format!("unknown section: @{name}"));
                }
            }
        }

        Ok(PluginInvoke {
            tokens: tokens.ok_or("missing @tokens section")?,
            remaining,
            plugins,
        })
    }
}

/// The main derive macro - this is what users write `#[derive(FacetPoc)]` for.
#[proc_macro_derive(FacetPoc, attributes(facet_poc))]
pub fn derive_facet_poc(input: TokenStream) -> TokenStream {
    let input_tokens = TokenStream2::from(input.clone());
    let mut iter = input_tokens.clone().to_token_iter();

    // Find all #[facet_poc(...)] attributes
    let mut plugins = Vec::new();

    while let Ok(attr) = iter.parse::<Attribute>() {
        let content = attr.body.content.to_string();
        if content.starts_with("facet_poc") {
            if let Some(start) = content.find('(') {
                if let Some(end) = content.rfind(')') {
                    let inner = &content[start + 1..end];
                    for part in inner.split(',') {
                        let plugin = part.trim();
                        if !plugin.is_empty() {
                            plugins.push(plugin.to_string());
                        }
                    }
                }
            }
        }
    }

    // Validate plugins
    for plugin in &plugins {
        match plugin.as_str() {
            "display" | "debug" => {}
            other => {
                let msg = format!("Unknown plugin: {other}");
                return quote! { compile_error!(#msg); }.into();
            }
        }
    }

    if plugins.is_empty() {
        return quote! {
            ::facet_plugin_poc::__facet_finalize! {
                @tokens { #input_tokens }
                @plugins { }
            }
        }
        .into();
    }

    // Build the chain from right to left
    let plugin_paths: Vec<_> = plugins
        .iter()
        .map(|p| match p.as_str() {
            "display" => quote! { ::facet_plugin_poc::__plugin_display_invoke },
            "debug" => quote! { ::facet_plugin_poc::__plugin_debug_invoke },
            _ => unreachable!(),
        })
        .collect();

    let first = &plugin_paths[0];
    let rest: Vec<_> = plugin_paths[1..].iter().collect();

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

/// Helper to generate the next step in the plugin chain
fn chain_next(invoke: PluginInvoke, plugin_name: &str) -> TokenStream2 {
    let tokens = &invoke.tokens;

    let mut plugins = invoke.plugins;
    plugins.push(plugin_name.to_string());

    let plugin_strings: Vec<_> = plugins
        .iter()
        .map(|s| proc_macro2::Literal::string(s))
        .collect();

    if invoke.remaining.is_empty() {
        quote! {
            ::facet_plugin_poc::__facet_finalize! {
                @tokens { #tokens }
                @plugins { #(#plugin_strings,)* }
            }
        }
    } else {
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
#[proc_macro]
pub fn __plugin_display_invoke(input: TokenStream) -> TokenStream {
    let invoke = match PluginInvoke::parse(input.into()) {
        Ok(i) => i,
        Err(e) => return quote! { compile_error!(#e); }.into(),
    };
    chain_next(invoke, "display").into()
}

/// Plugin: debug
#[proc_macro]
pub fn __plugin_debug_invoke(input: TokenStream) -> TokenStream {
    let invoke = match PluginInvoke::parse(input.into()) {
        Ok(i) => i,
        Err(e) => return quote! { compile_error!(#e); }.into(),
    };
    chain_next(invoke, "debug").into()
}

/// The finalizer - parses using facet-macro-parse and generates code
#[proc_macro]
pub fn __facet_finalize(input: TokenStream) -> TokenStream {
    let invoke = match PluginInvoke::parse(input.into()) {
        Ok(i) => i,
        Err(e) => return quote! { compile_error!(#e); }.into(),
    };

    // Parse the type using facet-macro-parse
    let parsed = match parse_type(invoke.tokens) {
        Ok(p) => p,
        Err(e) => return quote! { compile_error!(#e); }.into(),
    };

    let name = parsed.name();
    let mut generated = TokenStream2::new();

    for plugin in &invoke.plugins {
        match plugin.as_str() {
            "display" => {
                let display_impl = generate_display(&parsed);
                generated.extend(display_impl);
            }
            "debug" => {
                let debug_impl = quote! {
                    impl ::core::fmt::Debug for #name {
                        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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

/// Generate Display impl using the parsed type from facet-macro-parse
fn generate_display(parsed: &PType) -> TokenStream2 {
    let name = parsed.name();

    match parsed {
        PType::Enum(e) => {
            // Build match arms for each variant
            let match_arms: Vec<_> = e
                .variants
                .iter()
                .map(|v| {
                    // Get the raw ident for pattern matching
                    let variant_name = match &v.name.raw {
                        IdentOrLiteral::Ident(id) => id.clone(),
                        IdentOrLiteral::Literal(_) => unreachable!("enum variants are always idents"),
                    };

                    // Get doc comment, joining lines with space
                    let doc = v.attrs.doc.join(" ").trim().to_string();
                    let format_str = if doc.is_empty() {
                        v.name.effective.clone()
                    } else {
                        doc
                    };

                    // Generate pattern based on variant kind
                    match &v.kind {
                        PVariantKind::Unit => {
                            quote! {
                                Self::#variant_name => write!(f, #format_str)
                            }
                        }
                        PVariantKind::Tuple { fields } => {
                            let field_names: Vec<_> = (0..fields.len())
                                .map(|i| {
                                    proc_macro2::Ident::new(&format!("v{i}"), proc_macro2::Span::call_site())
                                })
                                .collect();

                            // Check if format string uses positional args
                            let has_positional = format_str.contains("{0}");
                            let format_args = if has_positional && !field_names.is_empty() {
                                let args: Vec<_> = field_names.iter().map(|n| quote! { , #n }).collect();
                                quote! { #(#args)* }
                            } else {
                                quote! {}
                            };

                            quote! {
                                Self::#variant_name( #(#field_names),* ) => write!(f, #format_str #format_args)
                            }
                        }
                        PVariantKind::Struct { fields } => {
                            let field_names: Vec<_> = fields
                                .iter()
                                .map(|f| {
                                    match &f.name.raw {
                                        IdentOrLiteral::Ident(id) => quote! { #id },
                                        IdentOrLiteral::Literal(_) => unreachable!("struct fields are always idents"),
                                    }
                                })
                                .collect();

                            quote! {
                                Self::#variant_name { #(#field_names),* } => write!(f, #format_str)
                            }
                        }
                    }
                })
                .collect();

            quote! {
                impl ::core::fmt::Display for #name {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        match self {
                            #(#match_arms),*
                        }
                    }
                }
            }
        }
        PType::Struct(s) => {
            // Get doc comment, joining lines with space
            let doc = s.container.attrs.doc.join(" ").trim().to_string();
            let format_str = if doc.is_empty() {
                name.to_string()
            } else {
                doc
            };

            quote! {
                impl ::core::fmt::Display for #name {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        write!(f, #format_str)
                    }
                }
            }
        }
    }
}
