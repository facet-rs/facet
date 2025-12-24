//! # facet-error
//!
//! A `thiserror` replacement powered by facet reflection.
//!
//! ## Usage
//!
//! ```ignore
//! use facet::Facet;
//!
//! #[derive(Facet, Debug)]
//! #[facet(derive(Error))]
//! pub enum MyError {
//!     /// data store disconnected
//!     #[facet(error::from)]
//!     Disconnect(std::io::Error),
//!
//!     /// invalid header (expected {expected}, found {found})
//!     InvalidHeader { expected: String, found: String },
//!
//!     /// unknown error
//!     Unknown,
//! }
//! ```
//!
//! This generates:
//! - `impl Display for MyError` using doc comments as format strings
//! - `impl Error for MyError` with proper `source()` implementation
//! - `impl From<std::io::Error> for MyError` for variants with `#[facet(error::from)]`

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use facet_macro_parse::{IdentOrLiteral, PEnum, PStruct, PType, PVariantKind, parse_type};
use facet_macro_types::{At, BraceGroupContaining, Comma, IParse, Ident, Literal, ToTokenIter};

// ============================================================================
// PLUGIN CHAIN ENTRY POINT
// ============================================================================

/// Plugin chain entry point.
///
/// Called by `#[derive(Facet)]` when `#[facet(derive(Error))]` is present.
/// Adds itself to the plugin list and chains to the next plugin or finalize.
#[proc_macro]
pub fn __facet_derive(input: TokenStream) -> TokenStream {
    let input2: TokenStream2 = input.into();
    let invoke = match PluginInvoke::parse(input2) {
        Ok(i) => i,
        Err(e) => return quote! { compile_error!(#e); }.into(),
    };

    chain_next(invoke, "Error").into()
}

/// Plugin code generator.
///
/// Called by `__facet_finalize!` to generate Display and Error implementations.
#[proc_macro]
pub fn __facet_generate(input: TokenStream) -> TokenStream {
    let input2: TokenStream2 = input.into();
    let invoke = match GenerateInvoke::parse(input2) {
        Ok(i) => i,
        Err(e) => return quote! { compile_error!(#e); }.into(),
    };

    generate_error_impls(invoke).into()
}

// ============================================================================
// PLUGIN INVOKE PARSING
// ============================================================================

facet_macro_types::unsynn! {
    struct SectionMarker {
        _at: At,
        name: Ident,
    }

    struct BracedSection {
        marker: SectionMarker,
        content: BraceGroupContaining<TokenStream2>,
    }
}

struct PluginInvoke {
    tokens: TokenStream2,
    remaining: Vec<TokenStream2>,
    plugins: Vec<String>,
    facet_crate: TokenStream2,
}

impl PluginInvoke {
    fn parse(input: TokenStream2) -> Result<Self, String> {
        let mut iter = input.to_token_iter();
        let mut tokens = None;
        let mut remaining = Vec::new();
        let mut plugins = Vec::new();
        let mut facet_crate = None;

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
                            if let proc_macro2::TokenTree::Punct(p) = &tt
                                && p.as_char() == ','
                            {
                                if !current.is_empty() {
                                    remaining.push(current);
                                    current = TokenStream2::new();
                                }
                                continue;
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
                "facet_crate" => {
                    facet_crate = Some(section.content.content);
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
            facet_crate: facet_crate.unwrap_or_else(|| quote! { ::facet }),
        })
    }
}

struct GenerateInvoke {
    tokens: TokenStream2,
    facet_crate: TokenStream2,
}

impl GenerateInvoke {
    fn parse(input: TokenStream2) -> Result<Self, String> {
        let mut iter = input.to_token_iter();
        let mut tokens = None;
        let mut facet_crate = None;

        while let Ok(section) = iter.parse::<BracedSection>() {
            let name = section.marker.name.to_string();
            match name.as_str() {
                "tokens" => {
                    tokens = Some(section.content.content);
                }
                "facet_crate" => {
                    facet_crate = Some(section.content.content);
                }
                _ => {
                    return Err(format!("unknown section in __facet_generate: @{name}"));
                }
            }
        }

        Ok(GenerateInvoke {
            tokens: tokens.ok_or("missing @tokens section")?,
            facet_crate: facet_crate.unwrap_or_else(|| quote! { ::facet }),
        })
    }
}

// ============================================================================
// PLUGIN CHAIN LOGIC
// ============================================================================

fn chain_next(invoke: PluginInvoke, plugin_name: &str) -> TokenStream2 {
    let tokens = &invoke.tokens;
    let facet_crate = &invoke.facet_crate;

    let mut plugins = invoke.plugins;
    plugins.push(plugin_name.to_string());

    let plugin_strings: Vec<_> = plugins
        .iter()
        .map(|s| proc_macro2::Literal::string(s))
        .collect();

    if invoke.remaining.is_empty() {
        // Last plugin, call finalize
        quote! {
            #facet_crate::__facet_finalize! {
                @tokens { #tokens }
                @plugins { #(#plugin_strings,)* }
                @facet_crate { #facet_crate }
            }
        }
    } else {
        // More plugins, chain to next
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
                @facet_crate { #facet_crate }
            }
        }
    }
}

// ============================================================================
// CODE GENERATION
// ============================================================================

fn generate_error_impls(invoke: GenerateInvoke) -> TokenStream2 {
    let tokens = invoke.tokens;
    let _facet_crate = invoke.facet_crate;

    // Parse the type
    let parsed = match parse_type(tokens.clone()) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("facet-error: failed to parse type: {e}");
            return quote! { compile_error!(#msg); };
        }
    };

    let name = parsed.name();

    // Generate Display impl
    let display_impl = generate_display(&parsed, name);

    // Generate Error impl
    let error_impl = generate_error(&parsed, name);

    // Generate From impls for #[facet(error::from)] variants
    let from_impls = generate_from_impls(&parsed, name);

    quote! {
        #display_impl
        #error_impl
        #from_impls
    }
}

fn generate_display(parsed: &PType, name: &Ident) -> TokenStream2 {
    match parsed {
        PType::Enum(e) => generate_display_enum(e, name),
        PType::Struct(s) => generate_display_struct(s, name),
    }
}

fn generate_display_enum(e: &PEnum, name: &Ident) -> TokenStream2 {
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
                        .map(|i| quote::format_ident!("v{}", i))
                        .collect();

                    // Check if format string uses positional args like {0}
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
                        .map(|f| match &f.name.raw {
                            IdentOrLiteral::Ident(id) => quote! { #id },
                            IdentOrLiteral::Literal(_) => {
                                unreachable!("struct fields are always idents")
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

fn generate_display_struct(s: &PStruct, name: &Ident) -> TokenStream2 {
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

fn generate_error(parsed: &PType, name: &Ident) -> TokenStream2 {
    // For now, generate a simple Error impl
    // TODO: Add source() support via #[facet(error::source)]

    match parsed {
        PType::Enum(e) => {
            // Check each variant for a source field
            let source_arms: Vec<_> = e
                .variants
                .iter()
                .filter_map(|v| {
                    let variant_name = match &v.name.raw {
                        IdentOrLiteral::Ident(id) => id.clone(),
                        IdentOrLiteral::Literal(_) => return None,
                    };

                    // Check for #[facet(error::source)] or #[facet(error::from)] on fields
                    // For now, check if single tuple field implements Error
                    match &v.kind {
                        PVariantKind::Tuple { fields } if fields.len() == 1 => {
                            // Check if field has error::from or error::source attribute
                            let has_source_attr = fields[0].attrs.facet.iter().any(|attr| {
                                if let Some(ns) = &attr.ns {
                                    *ns == "error" && (attr.key == "source" || attr.key == "from")
                                } else {
                                    false
                                }
                            });

                            if has_source_attr {
                                Some(quote! {
                                    Self::#variant_name(source) => Some(source)
                                })
                            } else {
                                Some(quote! {
                                    Self::#variant_name(_) => None
                                })
                            }
                        }
                        PVariantKind::Tuple { fields } => {
                            let underscores: Vec<_> =
                                (0..fields.len()).map(|_| quote! { _ }).collect();
                            Some(quote! {
                                Self::#variant_name(#(#underscores),*) => None
                            })
                        }
                        PVariantKind::Struct { fields } => {
                            // Check for #[facet(error::source)] on struct fields
                            let source_field = fields.iter().find(|f| {
                                f.attrs.facet.iter().any(|attr| {
                                    if let Some(ns) = &attr.ns {
                                        *ns == "error"
                                            && (attr.key == "source" || attr.key == "from")
                                    } else {
                                        false
                                    }
                                })
                            });

                            if let Some(sf) = source_field {
                                let field_name = match &sf.name.raw {
                                    IdentOrLiteral::Ident(id) => id.clone(),
                                    _ => return None,
                                };
                                Some(quote! {
                                    Self::#variant_name { #field_name, .. } => Some(#field_name)
                                })
                            } else {
                                Some(quote! {
                                    Self::#variant_name { .. } => None
                                })
                            }
                        }
                        PVariantKind::Unit => Some(quote! {
                            Self::#variant_name => None
                        }),
                    }
                })
                .collect();

            quote! {
                impl ::std::error::Error for #name {
                    fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> {
                        match self {
                            #(#source_arms),*
                        }
                    }
                }
            }
        }
        PType::Struct(_) => {
            // Structs typically don't have a source
            quote! {
                impl ::std::error::Error for #name {}
            }
        }
    }
}

fn generate_from_impls(parsed: &PType, name: &Ident) -> TokenStream2 {
    match parsed {
        PType::Enum(e) => {
            let from_impls: Vec<_> = e
                .variants
                .iter()
                .filter_map(|v| {
                    let variant_name = match &v.name.raw {
                        IdentOrLiteral::Ident(id) => id.clone(),
                        IdentOrLiteral::Literal(_) => return None,
                    };

                    // Check for #[facet(error::from)] on the variant or its single field
                    match &v.kind {
                        PVariantKind::Tuple { fields } if fields.len() == 1 => {
                            let has_from_attr = fields[0].attrs.facet.iter().any(|attr| {
                                if let Some(ns) = &attr.ns {
                                    *ns == "error" && attr.key == "from"
                                } else {
                                    false
                                }
                            });

                            if has_from_attr {
                                let field_ty = &fields[0].ty;
                                Some(quote! {
                                    impl ::core::convert::From<#field_ty> for #name {
                                        fn from(source: #field_ty) -> Self {
                                            Self::#variant_name(source)
                                        }
                                    }
                                })
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                })
                .collect();

            quote! { #(#from_impls)* }
        }
        PType::Struct(_) => quote! {},
    }
}
