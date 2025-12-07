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
//! Uses unsynn for parsing instead of syn.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

// Import unsynn but avoid shadowing std::result::Result
use unsynn::{
    At, BraceGroupContaining, BracketGroupContaining, Colon, Comma, Gt, Ident, Literal, Lt,
    ParenthesisGroupContaining, Pound, TokenTree, keyword, unsynn,
};
use unsynn::{IParse, ToTokenIter};

// Define keywords we need
keyword! {
    KFacetPoc = "facet_poc";
    KPub = "pub";
    KEnum = "enum";
    KStruct = "struct";
}

// Grammar for parsing the internal macro invocation format (private, not pub)
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
                    // Parse comma-separated paths - just collect token trees
                    let content = section.content.content;
                    if !content.is_empty() {
                        // Split by comma and collect each path
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
                    // Parse comma-separated string literals
                    let content = section.content.content;
                    let mut inner = content.to_token_iter();
                    while let Ok(lit) = inner.parse::<Literal>() {
                        // Extract string value from literal
                        let s = lit.to_string();
                        // Remove quotes
                        let unquoted = s.trim_matches('"');
                        plugins.push(unquoted.to_string());
                        // Skip comma if present
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

/// Parsed struct/enum for code generation
struct ParsedType {
    name: Ident,
    kind: TypeKind,
}

enum TypeKind {
    Enum(Vec<ParsedVariant>),
    Struct { doc: String },
}

struct ParsedVariant {
    name: Ident,
    doc: String,
    fields: VariantFields,
}

enum VariantFields {
    Unit,
    Tuple(usize),
    Named(Vec<Ident>),
}

// Grammar for attributes and visibility (private)
unsynn! {
    /// Attribute like `#[doc = "..."]` or `#[facet_poc(display)]`
    struct Attribute {
        pound: Pound,
        body: BracketGroupContaining<TokenStream2>,
    }

    /// Visibility
    enum Vis {
        Pub(KPub),
    }

    /// Generics like `<T, U>`
    struct Generics {
        lt: Lt,
        content: TokenStream2,
        gt: Gt,
    }
}

impl ParsedType {
    fn parse(tokens: TokenStream2) -> Result<Self, String> {
        let mut iter = tokens.clone().to_token_iter();

        // Skip attributes and visibility
        while iter.parse::<Attribute>().is_ok() {}
        let _ = iter.parse::<Vis>();

        // Check if enum or struct
        if iter.parse::<KEnum>().is_ok() {
            let name: Ident = iter
                .parse()
                .map_err(|e| format!("expected enum name: {e}"))?;
            // Skip generics if present
            let _ = iter.parse::<Generics>();

            let body: BraceGroupContaining<TokenStream2> = iter
                .parse()
                .map_err(|e| format!("expected enum body: {e}"))?;

            let variants = Self::parse_enum_variants(body.content)?;

            Ok(ParsedType {
                name,
                kind: TypeKind::Enum(variants),
            })
        } else if iter.parse::<KStruct>().is_ok() {
            // For struct, just get the name and doc
            let name: Ident = iter
                .parse()
                .map_err(|e| format!("expected struct name: {e}"))?;

            // Re-parse to get doc from attributes
            let mut doc_iter = tokens.to_token_iter();
            let mut doc = String::new();
            while let Ok(attr) = doc_iter.parse::<Attribute>() {
                if let Some(d) = extract_doc(&attr) {
                    if !doc.is_empty() {
                        doc.push(' ');
                    }
                    doc.push_str(&d);
                }
            }

            Ok(ParsedType {
                name,
                kind: TypeKind::Struct { doc },
            })
        } else {
            Err("expected enum or struct".to_string())
        }
    }

    fn parse_enum_variants(body: TokenStream2) -> Result<Vec<ParsedVariant>, String> {
        let mut variants = Vec::new();
        let mut iter = body.to_token_iter();

        loop {
            // Collect doc comments for this variant
            let mut doc = String::new();
            while let Ok(attr) = iter.parse::<Attribute>() {
                if let Some(d) = extract_doc(&attr) {
                    if !doc.is_empty() {
                        doc.push(' ');
                    }
                    doc.push_str(&d);
                }
            }

            // Try to parse variant name
            let name: Ident = match iter.parse() {
                Ok(n) => n,
                Err(_) => break, // No more variants
            };

            // Determine variant kind
            let fields = if let Ok(group) = iter.parse::<ParenthesisGroupContaining<TokenStream2>>()
            {
                // Tuple variant - count fields by counting commas + 1
                let content = group.content.to_string();
                let field_count = if content.trim().is_empty() {
                    0
                } else {
                    content.matches(',').count() + 1
                };
                VariantFields::Tuple(field_count)
            } else if let Ok(group) = iter.parse::<BraceGroupContaining<TokenStream2>>() {
                // Struct variant - extract field names
                let mut field_names = Vec::new();
                let mut field_iter = group.content.to_token_iter();
                loop {
                    // Skip attributes on fields
                    while field_iter.parse::<Attribute>().is_ok() {}

                    let field_name: Ident = match field_iter.parse() {
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    field_names.push(field_name);

                    // Skip : Type
                    let _ = field_iter.parse::<Colon>();
                    while field_iter.parse::<TokenTree>().is_ok() {
                        // Consume type tokens until comma or end
                        if field_iter.parse::<Comma>().is_ok() {
                            break;
                        }
                    }
                }
                VariantFields::Named(field_names)
            } else {
                VariantFields::Unit
            };

            variants.push(ParsedVariant { name, doc, fields });

            // Skip comma between variants
            let _ = iter.parse::<Comma>();
        }

        Ok(variants)
    }
}

/// Extract doc comment text from an attribute
fn extract_doc(attr: &Attribute) -> Option<String> {
    let content = attr.body.content.to_string();
    // Check if it starts with "doc"
    if content.starts_with("doc") {
        // Extract the string after `doc = `
        if let Some(idx) = content.find('=') {
            let rest = content[idx + 1..].trim();
            // Remove surrounding quotes
            let unquoted = rest.trim_matches('"').trim();
            return Some(unquoted.to_string());
        }
    }
    None
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
            // Extract plugin names from facet_poc(plugin1, plugin2, ...)
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
        // No plugins - go straight to finalize
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

/// The finalizer - parses and generates code
#[proc_macro]
pub fn __facet_finalize(input: TokenStream) -> TokenStream {
    let invoke = match PluginInvoke::parse(input.into()) {
        Ok(i) => i,
        Err(e) => return quote! { compile_error!(#e); }.into(),
    };

    let parsed = match ParsedType::parse(invoke.tokens) {
        Ok(p) => p,
        Err(e) => return quote! { compile_error!(#e); }.into(),
    };

    let name = &parsed.name;
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

/// Generate Display impl from doc comments
fn generate_display(parsed: &ParsedType) -> TokenStream2 {
    let name = &parsed.name;

    match &parsed.kind {
        TypeKind::Enum(variants) => {
            let match_arms: Vec<_> = variants
                .iter()
                .map(|v| {
                    let variant_name = &v.name;
                    let format_str = if v.doc.is_empty() {
                        variant_name.to_string()
                    } else {
                        v.doc.clone()
                    };

                    let (pattern, format_args) = match &v.fields {
                        VariantFields::Unit => (quote! {}, quote! {}),
                        VariantFields::Tuple(count) => {
                            let field_names: Vec<_> = (0..*count)
                                .map(|i| {
                                    proc_macro2::Ident::new(
                                        &format!("v{i}"),
                                        proc_macro2::Span::call_site(),
                                    )
                                })
                                .collect();
                            let pattern = quote! { ( #(#field_names),* ) };
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
                        VariantFields::Named(field_names) => {
                            let pattern = quote! { { #(#field_names),* } };
                            (pattern, quote! {})
                        }
                    };

                    quote! {
                        Self::#variant_name #pattern => write!(f, #format_str #format_args)
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
        TypeKind::Struct { doc } => {
            let format_str = if doc.is_empty() {
                name.to_string()
            } else {
                doc.clone()
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
