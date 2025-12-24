//! Plugin system for facet derive macro.
//!
//! This module implements the plugin chain pattern that allows external crates
//! to hook into `#[derive(Facet)]` and generate additional trait implementations.
//!
//! ## How it works
//!
//! 1. User writes `#[derive(Facet)]` with `#[facet(derive(Error))]`
//! 2. `facet_macros` detects the `derive(...)` attribute
//! 3. It chains to the first plugin: `::facet_error::__facet_derive!`
//! 4. Each plugin adds itself to the chain and forwards to the next (or finalize)
//! 5. `__facet_finalize!` parses ONCE and generates all code
//!
//! ## Plugin naming convention
//!
//! `#[facet(derive(Foo))]` maps to `::facet_foo::__facet_derive!`
//! (lowercase the trait name, prefix with `facet_`)

use crate::{Attribute, AttributeInner, FacetInner, IParse, Ident, ToTokenIter, TokenStream};
use quote::quote;

/// Extract plugin names from `#[facet(derive(Plugin1, Plugin2, ...))]` attributes.
///
/// Returns a list of plugin names (e.g., `["Error", "Display"]`).
pub fn extract_derive_plugins(attrs: &[Attribute]) -> Vec<String> {
    let mut plugins = Vec::new();

    for attr in attrs {
        if let AttributeInner::Facet(facet_attr) = &attr.body.content {
            for inner in facet_attr.inner.content.iter().map(|d| &d.value) {
                if let FacetInner::Simple(simple) = inner
                    && simple.key == "derive"
                {
                    // Parse the args to get plugin names
                    if let Some(ref args) = simple.args {
                        match args {
                            crate::AttrArgs::Parens(parens) => {
                                // Parse comma-separated identifiers from the parens
                                let content = &parens.content;
                                for token in content.clone() {
                                    if let proc_macro2::TokenTree::Ident(ident) = token {
                                        plugins.push(ident.to_string());
                                    }
                                }
                            }
                            crate::AttrArgs::Equals(_) => {
                                // derive = Something syntax (unusual but handle it)
                            }
                        }
                    }
                }
            }
        }
    }

    plugins
}

/// Convert a plugin name to its crate path.
///
/// `Error` → `::facet_error`
/// `Display` → `::facet_display`
pub fn plugin_to_crate_path(plugin_name: &str) -> TokenStream {
    // Convert PascalCase to snake_case and prefix with facet_
    let snake_case = to_snake_case(plugin_name);
    let crate_name = format!("facet_{snake_case}");
    let crate_ident = quote::format_ident!("{}", crate_name);
    quote! { ::#crate_ident }
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Strip `#[facet(derive(...))]` and plugin-specific attributes from a token stream.
///
/// This filters out the plugin-system-specific attributes before passing
/// the tokens to the normal Facet processing, which would otherwise reject
/// "derive" as an unknown attribute.
///
/// Currently strips:
/// - `#[facet(derive(...))]` - plugin registration
/// - `#[facet(error::from)]` - facet-error plugin attribute
/// - `#[facet(error::source)]` - facet-error plugin attribute
fn strip_derive_attrs(tokens: TokenStream) -> TokenStream {
    let mut result = TokenStream::new();
    let mut iter = tokens.into_iter().peekable();

    while let Some(tt) = iter.next() {
        // Check for # followed by [...]
        if let proc_macro2::TokenTree::Punct(p) = &tt
            && p.as_char() == '#'
            && let Some(proc_macro2::TokenTree::Group(g)) = iter.peek()
            && g.delimiter() == proc_macro2::Delimiter::Bracket
        {
            // This is an attribute - check if it's a plugin attribute
            let inner = g.stream();
            if is_plugin_attr(&inner) {
                // Skip the # and the [...]
                iter.next(); // consume the group
                continue;
            }
        }
        result.extend(std::iter::once(tt));
    }

    result
}

/// Check if an attribute is a plugin-specific attribute that should be stripped.
///
/// Returns true for:
/// - `facet(derive(...))`
/// - `facet(error::from)`
/// - `facet(error::source)`
/// - Any other `facet(namespace::key)` pattern (for future plugins)
fn is_plugin_attr(inner: &TokenStream) -> bool {
    let mut iter = inner.clone().into_iter();

    // Check for "facet"
    if let Some(proc_macro2::TokenTree::Ident(id)) = iter.next() {
        if id != "facet" {
            return false;
        }
    } else {
        return false;
    }

    // Check for (...) containing plugin-specific attributes
    if let Some(proc_macro2::TokenTree::Group(g)) = iter.next() {
        if g.delimiter() != proc_macro2::Delimiter::Parenthesis {
            return false;
        }

        let content = g.stream();
        let mut content_iter = content.into_iter();

        // Check the first identifier
        if let Some(proc_macro2::TokenTree::Ident(id)) = content_iter.next() {
            let first = id.to_string();

            // Check for derive(...)
            if first == "derive" {
                return true;
            }

            // Check for namespace::key pattern (e.g., error::from, error::source)
            if let Some(proc_macro2::TokenTree::Punct(p)) = content_iter.next()
                && p.as_char() == ':'
                && let Some(proc_macro2::TokenTree::Punct(p2)) = content_iter.next()
                && p2.as_char() == ':'
            {
                // This is a namespace::key pattern - strip it
                return true;
            }
        }
    }

    false
}

/// Check if an attribute's inner content is `facet(derive(...))`.
#[deprecated(note = "use is_plugin_attr instead")]
#[allow(dead_code)]
fn is_facet_derive_attr(inner: &TokenStream) -> bool {
    is_plugin_attr(inner)
}

/// Generate the plugin chain invocation.
///
/// If there are plugins, emits a chain starting with the first plugin.
/// If no plugins, returns None (caller should proceed with normal codegen).
pub fn generate_plugin_chain(
    input_tokens: &TokenStream,
    plugins: &[String],
    facet_crate: &TokenStream,
) -> Option<TokenStream> {
    if plugins.is_empty() {
        return None;
    }

    // Build the chain from right to left
    // First plugin gets called with remaining plugins
    let plugin_paths: Vec<TokenStream> = plugins
        .iter()
        .map(|p| {
            let crate_path = plugin_to_crate_path(p);
            quote! { #crate_path::__facet_invoke }
        })
        .collect();

    let first = &plugin_paths[0];
    let rest: Vec<_> = plugin_paths[1..].iter().collect();

    let remaining = if rest.is_empty() {
        quote! {}
    } else {
        quote! { #(#rest),* }
    };

    Some(quote! {
        #first! {
            @tokens { #input_tokens }
            @remaining { #remaining }
            @plugins { }
            @facet_crate { #facet_crate }
        }
    })
}

/// Implementation of `__facet_finalize!` proc macro.
///
/// This is called at the end of the plugin chain. It:
/// 1. Parses the type definition ONCE
/// 2. Generates the base Facet impl
/// 3. Evaluates each plugin's template against the parsed type
pub fn facet_finalize(input: TokenStream) -> TokenStream {
    // Parse the finalize invocation format:
    // @tokens { ... }
    // @plugins { @plugin { @name {...} @template {...} } ... }
    // @facet_crate { ::facet }

    let mut iter = input.to_token_iter();

    let mut tokens: Option<TokenStream> = None;
    let mut plugins_section: Option<TokenStream> = None;
    let mut facet_crate: Option<TokenStream> = None;

    // Parse sections
    while let Ok(section) = iter.parse::<FinalizeSection>() {
        match section.marker.name.to_string().as_str() {
            "tokens" => {
                tokens = Some(section.content.content);
            }
            "plugins" => {
                plugins_section = Some(section.content.content);
            }
            "facet_crate" => {
                facet_crate = Some(section.content.content);
            }
            other => {
                let msg = format!("unknown section in __facet_finalize: @{other}");
                return quote! { compile_error!(#msg); };
            }
        }
    }

    let tokens = match tokens {
        Some(t) => t,
        None => {
            return quote! { compile_error!("__facet_finalize: missing @tokens section"); };
        }
    };

    let facet_crate = facet_crate.unwrap_or_else(|| quote! { ::facet });

    // Strip #[facet(derive(...))] attributes before processing
    let filtered_tokens = strip_derive_attrs(tokens.clone());

    // Parse the type and generate Facet impl
    let mut type_iter = filtered_tokens.clone().to_token_iter();
    let facet_impl = match type_iter.parse::<crate::Cons<crate::AdtDecl, crate::EndOfStream>>() {
        Ok(it) => match it.first {
            crate::AdtDecl::Struct(parsed) => crate::process_struct::process_struct(parsed),
            crate::AdtDecl::Enum(parsed) => crate::process_enum::process_enum(parsed),
        },
        Err(err) => {
            let msg = format!("__facet_finalize: could not parse type: {err}");
            return quote! { compile_error!(#msg); };
        }
    };

    // Extract and evaluate plugin templates
    let plugin_impls = if let Some(plugins_tokens) = plugins_section {
        // For now, just extract the templates - evaluation will come next
        extract_plugin_templates(plugins_tokens, &filtered_tokens, &facet_crate)
    } else {
        vec![]
    };

    quote! {
        #facet_impl
        #(#plugin_impls)*
    }
}

/// Extract plugin templates from the @plugins section
/// For now, this is a placeholder that will be replaced with actual template evaluation
fn extract_plugin_templates(
    _plugins_tokens: TokenStream,
    _type_tokens: &TokenStream,
    _facet_crate: &TokenStream,
) -> Vec<TokenStream> {
    // TODO: Parse @plugin { @name { ... } @template { ... } } sections
    // TODO: Evaluate templates against parsed type
    // For now, return empty - this will make compilation succeed but not generate plugin code
    vec![]
}

// Grammar for parsing finalize sections
crate::unsynn! {
    /// Section marker like `@tokens`, `@plugins`
    struct FinalizeSectionMarker {
        _at: crate::At,
        name: Ident,
    }

    /// A braced section like `@tokens { ... }`
    struct FinalizeSection {
        marker: FinalizeSectionMarker,
        content: crate::BraceGroupContaining<TokenStream>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IParse;
    use quote::quote;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Error"), "error");
        assert_eq!(to_snake_case("Display"), "display");
        assert_eq!(to_snake_case("PartialEq"), "partial_eq");
        assert_eq!(to_snake_case("FromStr"), "from_str");
    }

    #[test]
    fn test_extract_derive_plugins() {
        let input = quote! {
            #[derive(Facet, Debug)]
            #[facet(derive(Error))]
            #[repr(u8)]
            pub enum MyError {
                Disconnect(u32),
            }
        };

        let mut iter = input.to_token_iter();
        let parsed = iter.parse::<crate::Enum>().expect("Failed to parse enum");

        let plugins = extract_derive_plugins(&parsed.attributes);
        assert_eq!(plugins, vec!["Error"]);
    }

    #[test]
    fn test_extract_multiple_plugins() {
        let input = quote! {
            #[facet(derive(Error, Display))]
            pub enum MyError {
                Unknown,
            }
        };

        let mut iter = input.to_token_iter();
        let parsed = iter.parse::<crate::Enum>().expect("Failed to parse enum");

        let plugins = extract_derive_plugins(&parsed.attributes);
        assert_eq!(plugins, vec!["Error", "Display"]);
    }
}
