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

/// A plugin reference - either a simple name or a full path.
///
/// - `Error` → convention-based lookup (`::facet_error`)
/// - `some_crate::SomeTrait` → explicit path (`::some_crate`)
#[derive(Debug, Clone)]
pub enum PluginRef {
    /// Simple name like `Error` - uses convention `::facet_{snake_case}`
    Simple(String),
    /// Explicit path like `some_crate::SomeTrait` - uses the crate part directly
    Path {
        /// The crate name (e.g., `some_crate`)
        crate_name: String,
        /// The plugin/trait name (e.g., `SomeTrait`)
        plugin_name: String,
    },
}

impl PluginRef {
    /// Get the crate path for this plugin reference.
    pub fn crate_path(&self) -> TokenStream {
        match self {
            PluginRef::Simple(name) => {
                let snake_case = to_snake_case(name);
                let crate_name = format!("facet_{snake_case}");
                let crate_ident = quote::format_ident!("{}", crate_name);
                quote! { ::#crate_ident }
            }
            PluginRef::Path { crate_name, .. } => {
                let crate_ident = quote::format_ident!("{}", crate_name);
                quote! { ::#crate_ident }
            }
        }
    }
}

/// Extract plugin references from `#[facet(derive(Plugin1, Plugin2, ...))]` attributes.
///
/// Supports both simple names and explicit paths:
/// - `#[facet(derive(Error))]` → `PluginRef::Simple("Error")`
/// - `#[facet(derive(some_crate::SomeTrait))]` → `PluginRef::Path { crate_name: "some_crate", plugin_name: "SomeTrait" }`
pub fn extract_derive_plugins(attrs: &[Attribute]) -> Vec<PluginRef> {
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
                                // Parse comma-separated items (either idents or paths)
                                plugins.extend(parse_plugin_list(&parens.content));
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

/// Parse a comma-separated list of plugin references (idents or paths).
fn parse_plugin_list(tokens: &[crate::TokenTree]) -> Vec<PluginRef> {
    let mut plugins = Vec::new();
    let mut iter = tokens.iter().cloned().peekable();

    while iter.peek().is_some() {
        // Collect tokens until comma or end
        let mut item_tokens = Vec::new();
        while let Some(tt) = iter.peek() {
            if let proc_macro2::TokenTree::Punct(p) = tt
                && p.as_char() == ','
            {
                iter.next(); // consume comma
                break;
            }
            item_tokens.push(iter.next().unwrap());
        }

        // Parse the collected tokens as either a simple ident or a path
        if let Some(plugin_ref) = parse_plugin_ref(&item_tokens) {
            plugins.push(plugin_ref);
        }
    }

    plugins
}

/// Parse a single plugin reference from a sequence of tokens.
fn parse_plugin_ref(tokens: &[proc_macro2::TokenTree]) -> Option<PluginRef> {
    if tokens.is_empty() {
        return None;
    }

    // Check if it's a path (contains ::)
    let has_path_sep = tokens.windows(2).any(|w| {
        matches!((&w[0], &w[1]),
            (proc_macro2::TokenTree::Punct(p1), proc_macro2::TokenTree::Punct(p2))
            if p1.as_char() == ':' && p2.as_char() == ':')
    });

    if has_path_sep {
        // Parse as path: crate_name::PluginName
        // For now, just support single-segment crate name
        let mut iter = tokens.iter();

        // First ident is crate name
        let crate_name = match iter.next() {
            Some(proc_macro2::TokenTree::Ident(id)) => id.to_string(),
            _ => return None,
        };

        // Skip ::
        match (iter.next(), iter.next()) {
            (Some(proc_macro2::TokenTree::Punct(p1)), Some(proc_macro2::TokenTree::Punct(p2)))
                if p1.as_char() == ':' && p2.as_char() == ':' => {}
            _ => return None,
        }

        // Last ident is plugin name
        let plugin_name = match iter.next() {
            Some(proc_macro2::TokenTree::Ident(id)) => id.to_string(),
            _ => return None,
        };

        Some(PluginRef::Path {
            crate_name,
            plugin_name,
        })
    } else {
        // Simple ident
        match tokens.first() {
            Some(proc_macro2::TokenTree::Ident(id)) => Some(PluginRef::Simple(id.to_string())),
            _ => None,
        }
    }
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
/// - `derive(...)` - plugin registration
/// - `error::from` - facet-error plugin attribute
/// - `error::source` - facet-error plugin attribute
/// - Any `namespace::key` pattern (for future plugins)
///
/// Handles combined attributes like `#[facet(rename_all = "...", derive(Default))]`
/// by removing only the plugin-specific parts and keeping other attributes.
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
            // This is an attribute - check if it's a facet attribute
            let inner = g.stream();
            if let Some(filtered) = strip_plugin_items_from_facet_attr(&inner) {
                if filtered.is_empty() {
                    // All items were stripped - skip the entire attribute
                    iter.next(); // consume the group
                    continue;
                } else {
                    // Some items remain - emit the filtered attribute
                    result.extend(std::iter::once(tt));
                    iter.next(); // consume the original group
                    result.extend(std::iter::once(proc_macro2::TokenTree::Group(
                        proc_macro2::Group::new(proc_macro2::Delimiter::Bracket, filtered),
                    )));
                    continue;
                }
            }
        }
        result.extend(std::iter::once(tt));
    }

    result
}

/// Strip plugin-specific items from inside a facet attribute.
///
/// Returns Some(filtered_tokens) if this is a facet attribute, None otherwise.
/// The filtered_tokens will have plugin items removed (derive, namespace::key).
/// If all items are plugin items, returns Some(empty stream).
fn strip_plugin_items_from_facet_attr(inner: &TokenStream) -> Option<TokenStream> {
    let mut iter = inner.clone().into_iter().peekable();

    // Check for "facet" identifier
    let facet_ident = match iter.next() {
        Some(proc_macro2::TokenTree::Ident(id)) if id == "facet" => id,
        _ => return None,
    };

    // Check for (...) group
    let group = match iter.next() {
        Some(proc_macro2::TokenTree::Group(g))
            if g.delimiter() == proc_macro2::Delimiter::Parenthesis =>
        {
            g
        }
        _ => return None,
    };

    // Parse and filter the items inside facet(...)
    let filtered_content = strip_plugin_items_from_content(group.stream());

    // Reconstruct the attribute
    let mut result = TokenStream::new();
    result.extend(std::iter::once(proc_macro2::TokenTree::Ident(facet_ident)));
    result.extend(std::iter::once(proc_macro2::TokenTree::Group(
        proc_macro2::Group::new(proc_macro2::Delimiter::Parenthesis, filtered_content),
    )));

    Some(result)
}

/// Strip plugin-specific items from the content of a facet(...) attribute.
///
/// Items are comma-separated. Plugin items are:
/// - `derive(...)` - plugin registration
/// - `namespace::key` patterns (e.g., error::from, error::source)
fn strip_plugin_items_from_content(content: TokenStream) -> TokenStream {
    let mut items: Vec<TokenStream> = Vec::new();

    // Parse comma-separated items
    let mut current_item = TokenStream::new();
    let tokens: Vec<proc_macro2::TokenTree> = content.into_iter().collect();

    for tt in &tokens {
        // Check for comma separator
        if let proc_macro2::TokenTree::Punct(p) = tt
            && p.as_char() == ','
        {
            // End of current item
            if !current_item.is_empty() && !is_plugin_item(&current_item) {
                items.push(current_item);
            }
            current_item = TokenStream::new();
            continue;
        }

        current_item.extend(std::iter::once(tt.clone()));
    }

    // Don't forget the last item
    if !current_item.is_empty() && !is_plugin_item(&current_item) {
        items.push(current_item);
    }

    // Reconstruct with commas
    let mut result = TokenStream::new();
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            result.extend(std::iter::once(proc_macro2::TokenTree::Punct(
                proc_macro2::Punct::new(',', proc_macro2::Spacing::Alone),
            )));
        }
        result.extend(item.clone());
    }

    result
}

/// Check if an item within facet(...) is a plugin-specific item.
///
/// Returns true for:
/// - `derive(...)` - plugin registration
///
/// NOTE: We intentionally do NOT strip `namespace::key` patterns here.
/// Extension attributes like `args::positional`, `dibs::table`, `error::from`
/// must be preserved so they can be processed by the Facet derive and stored
/// in the Shape's attributes field.
fn is_plugin_item(item: &TokenStream) -> bool {
    let mut iter = item.clone().into_iter();

    if let Some(proc_macro2::TokenTree::Ident(id)) = iter.next() {
        let name = id.to_string();

        // Check for derive(...)
        if name == "derive" {
            return true;
        }
    }

    false
}

/// Generate the plugin chain invocation.
///
/// If there are plugins, emits a chain starting with the first plugin.
/// If no plugins, returns None (caller should proceed with normal codegen).
pub fn generate_plugin_chain(
    input_tokens: &TokenStream,
    plugins: &[PluginRef],
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
            let crate_path = p.crate_path();
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

/// Represents a parsed plugin with its template
struct PluginTemplate {
    #[allow(dead_code)] // Will be used for debugging/diagnostics
    name: String,
    template: TokenStream,
}

/// Extract plugin templates from the @plugins section
fn extract_plugin_templates(
    plugins_tokens: TokenStream,
    type_tokens: &TokenStream,
    facet_crate: &TokenStream,
) -> Vec<TokenStream> {
    // Parse plugin sections
    let plugins = parse_plugin_sections(plugins_tokens);

    // Parse the type once for all plugins
    let parsed_type = match facet_macro_parse::parse_type(type_tokens.clone()) {
        Ok(ty) => ty,
        Err(e) => {
            let msg = format!("failed to parse type for plugin templates: {e}");
            return vec![quote! { compile_error!(#msg); }];
        }
    };

    // Evaluate each plugin's template
    plugins
        .into_iter()
        .map(|plugin| evaluate_template(plugin.template, &parsed_type, facet_crate))
        .collect()
}

/// Parse @plugin { @name {...} @template {...} } sections
fn parse_plugin_sections(tokens: TokenStream) -> Vec<PluginTemplate> {
    let mut plugins = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    while let Some(tt) = iter.next() {
        // Look for @plugin marker
        if let proc_macro2::TokenTree::Punct(p) = &tt
            && p.as_char() == '@'
        {
            // Next should be 'plugin' identifier
            if let Some(proc_macro2::TokenTree::Ident(id)) = iter.peek()
                && *id == "plugin"
            {
                iter.next(); // consume 'plugin'

                // Next should be { ... } containing @name and @template
                if let Some(proc_macro2::TokenTree::Group(g)) = iter.next()
                    && g.delimiter() == proc_macro2::Delimiter::Brace
                    && let Some(plugin) = parse_plugin_content(g.stream())
                {
                    plugins.push(plugin);
                }
            }
        }
    }

    plugins
}

/// Parse the content of a @plugin { ... } section
fn parse_plugin_content(tokens: TokenStream) -> Option<PluginTemplate> {
    let mut name: Option<String> = None;
    let mut template: Option<TokenStream> = None;
    let mut iter = tokens.into_iter().peekable();

    while let Some(tt) = iter.next() {
        if let proc_macro2::TokenTree::Punct(p) = &tt
            && p.as_char() == '@'
            && let Some(proc_macro2::TokenTree::Ident(id)) = iter.peek()
        {
            let key = id.to_string();
            iter.next(); // consume identifier

            // Next should be { ... }
            if let Some(proc_macro2::TokenTree::Group(g)) = iter.next()
                && g.delimiter() == proc_macro2::Delimiter::Brace
            {
                match key.as_str() {
                    "name" => {
                        // Extract string literal from group
                        let content = g.stream().into_iter().collect::<Vec<_>>();
                        if let Some(proc_macro2::TokenTree::Literal(lit)) = content.first() {
                            let s = lit.to_string();
                            name = Some(s.trim_matches('"').to_string());
                        }
                    }
                    "template" => {
                        template = Some(g.stream());
                    }
                    _ => {}
                }
            }
        }
    }

    match (name, template) {
        (Some(n), Some(t)) => Some(PluginTemplate {
            name: n,
            template: t,
        }),
        _ => None,
    }
}

/// Evaluate a template against a parsed type
fn evaluate_template(
    template: TokenStream,
    parsed_type: &facet_macro_parse::PType,
    _facet_crate: &TokenStream,
) -> TokenStream {
    let mut ctx = EvalContext::new(parsed_type);
    evaluate_with_context(template, &mut ctx)
}

// ============================================================================
// CONTEXT STACK
// ============================================================================

/// The evaluation context - a stack of nested scopes.
///
/// As we enter `@for_variant`, `@for_field`, `@if_attr`, etc., we push
/// context frames. Directives like `@field_name` look up the stack to find
/// the relevant context.
struct EvalContext<'a> {
    /// The parsed type we're generating code for
    parsed_type: &'a facet_macro_parse::PType,

    /// Stack of context frames (innermost last)
    stack: Vec<ContextFrame<'a>>,
}

/// A single frame in the context stack
enum ContextFrame<'a> {
    /// We're inside a `@for_variant { ... }` loop
    Variant {
        variant: &'a facet_macro_parse::PVariant,
    },

    /// We're inside a `@for_field { ... }` loop
    Field {
        field: &'a facet_macro_parse::PStructField,
        /// Index of this field (for tuple patterns like `__v0`, `__v1`)
        index: usize,
    },

    /// We're inside a `@if_attr(ns::key) { ... }` block
    Attr {
        /// The matched attribute
        attr: &'a facet_macro_parse::PFacetAttr,
    },
}

impl<'a> EvalContext<'a> {
    const fn new(parsed_type: &'a facet_macro_parse::PType) -> Self {
        Self {
            parsed_type,
            stack: Vec::new(),
        }
    }

    fn push(&mut self, frame: ContextFrame<'a>) {
        self.stack.push(frame);
    }

    fn pop(&mut self) {
        self.stack.pop();
    }

    /// Find the current variant context (if any)
    fn current_variant(&self) -> Option<&'a facet_macro_parse::PVariant> {
        self.stack.iter().rev().find_map(|f| match f {
            ContextFrame::Variant { variant } => Some(*variant),
            _ => None,
        })
    }

    /// Find the current field context (if any)
    fn current_field(&self) -> Option<(&'a facet_macro_parse::PStructField, usize)> {
        self.stack.iter().rev().find_map(|f| match f {
            ContextFrame::Field { field, index } => Some((*field, *index)),
            _ => None,
        })
    }

    /// Find the current attr context (if any)
    fn current_attr(&self) -> Option<&'a facet_macro_parse::PFacetAttr> {
        self.stack.iter().rev().find_map(|f| match f {
            ContextFrame::Attr { attr } => Some(*attr),
            _ => None,
        })
    }

    /// Get the fields of the current context (variant's fields or struct's fields)
    fn current_fields(&self) -> Option<&'a [facet_macro_parse::PStructField]> {
        // First check if we're in a variant
        if let Some(variant) = self.current_variant() {
            return match &variant.kind {
                facet_macro_parse::PVariantKind::Tuple { fields } => Some(fields),
                facet_macro_parse::PVariantKind::Struct { fields } => Some(fields),
                facet_macro_parse::PVariantKind::Unit => None,
            };
        }

        // Otherwise, check if we're in a struct
        if let facet_macro_parse::PType::Struct(s) = self.parsed_type {
            return match &s.kind {
                facet_macro_parse::PStructKind::Struct { fields } => Some(fields),
                facet_macro_parse::PStructKind::TupleStruct { fields } => Some(fields),
                facet_macro_parse::PStructKind::UnitStruct => None,
            };
        }

        None
    }

    /// Get the attrs of the current context (field, variant, or container)
    fn current_attrs(&self) -> &'a facet_macro_parse::PAttrs {
        // Check field first (most specific)
        if let Some((field, _)) = self.current_field() {
            return &field.attrs;
        }

        // Then variant
        if let Some(variant) = self.current_variant() {
            return &variant.attrs;
        }

        // Finally container
        match self.parsed_type {
            facet_macro_parse::PType::Struct(s) => &s.container.attrs,
            facet_macro_parse::PType::Enum(e) => &e.container.attrs,
        }
    }
}

// ============================================================================
// ATTRIBUTE QUERY
// ============================================================================

/// Parsed attribute query like `error::source` or `diagnostic::label`
struct AttrQuery {
    ns: String,
    key: String,
}

impl AttrQuery {
    /// Parse from tokens like `error::source` or `diagnostic::label`
    fn parse(tokens: TokenStream) -> Option<Self> {
        let mut iter = tokens.into_iter();

        // First: namespace ident
        let ns = match iter.next() {
            Some(proc_macro2::TokenTree::Ident(id)) => id.to_string(),
            _ => return None,
        };

        // Then: ::
        match (iter.next(), iter.next()) {
            (Some(proc_macro2::TokenTree::Punct(p1)), Some(proc_macro2::TokenTree::Punct(p2)))
                if p1.as_char() == ':' && p2.as_char() == ':' => {}
            _ => return None,
        }

        // Then: key ident
        let key = match iter.next() {
            Some(proc_macro2::TokenTree::Ident(id)) => id.to_string(),
            _ => return None,
        };

        Some(AttrQuery { ns, key })
    }

    /// Check if an attribute matches this query
    fn matches(&self, attr: &facet_macro_parse::PFacetAttr) -> bool {
        if let Some(ref ns) = attr.ns {
            *ns == self.ns && attr.key == self.key
        } else {
            false
        }
    }

    /// Find matching attribute in a list
    fn find_in<'a>(
        &self,
        attrs: &'a [facet_macro_parse::PFacetAttr],
    ) -> Option<&'a facet_macro_parse::PFacetAttr> {
        attrs.iter().find(|a| self.matches(a))
    }
}

// ============================================================================
// TEMPLATE EVALUATION
// ============================================================================

/// Evaluate a template with the given context
fn evaluate_with_context(template: TokenStream, ctx: &mut EvalContext<'_>) -> TokenStream {
    let mut output = TokenStream::new();
    let mut iter = template.into_iter().peekable();

    while let Some(tt) = iter.next() {
        match &tt {
            proc_macro2::TokenTree::Punct(p) if p.as_char() == '@' => {
                handle_directive(&mut iter, ctx, &mut output);
            }
            proc_macro2::TokenTree::Group(g) => {
                // Recursively evaluate groups
                let inner = evaluate_with_context(g.stream(), ctx);
                let new_group = proc_macro2::Group::new(g.delimiter(), inner);
                output.extend(std::iter::once(proc_macro2::TokenTree::Group(new_group)));
            }
            _ => {
                output.extend(std::iter::once(tt));
            }
        }
    }

    output
}

/// Handle a directive after seeing `@`
fn handle_directive(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(next) = iter.next() else {
        // @ at end of stream - emit it
        output.extend(quote! { @ });
        return;
    };

    let proc_macro2::TokenTree::Ident(directive_ident) = &next else {
        // Not an ident - emit @ and the token
        output.extend(quote! { @ });
        output.extend(std::iter::once(next));
        return;
    };

    let directive = directive_ident.to_string();

    match directive.as_str() {
        // === Type-level directives ===
        "Self" => emit_self_type(ctx, output),

        // === Looping directives ===
        "for_variant" => handle_for_variant(iter, ctx, output),
        "for_field" => handle_for_field(iter, ctx, output),

        // === Conditional directives ===
        "if_attr" => handle_if_attr(iter, ctx, output),
        "if_field_attr" => handle_if_field_attr(iter, ctx, output),
        "if_any_field_attr" => handle_if_any_field_attr(iter, ctx, output),
        "if_struct" => handle_if_struct(iter, ctx, output),
        "if_enum" => handle_if_enum(iter, ctx, output),
        "if_unit_variant" => handle_if_unit_variant(iter, ctx, output),
        "if_tuple_variant" => handle_if_tuple_variant(iter, ctx, output),
        "if_struct_variant" => handle_if_struct_variant(iter, ctx, output),

        // === Context accessors ===
        "variant_name" => emit_variant_name(ctx, output),
        "variant_pattern" => emit_variant_pattern(ctx, output),
        "variant_pattern_only" => handle_variant_pattern_only(iter, ctx, output),
        "field_name" => emit_field_name(ctx, output),
        "field_type" => emit_field_type(ctx, output),
        "field_expr" => emit_field_expr(ctx, output),
        "attr_args" => emit_attr_args(ctx, output),
        "doc" => emit_doc(ctx, output),

        // === Default-related directives ===
        "field_default_expr" => emit_field_default_expr(ctx, output),
        "variant_default_construction" => emit_variant_default_construction(ctx, output),

        // === Display-related directives ===
        "format_doc_comment" => emit_format_doc_comment(ctx, output),

        // === Unknown directive ===
        _ => {
            // Emit as-is (might be user code with @ symbol)
            output.extend(quote! { @ });
            output.extend(std::iter::once(next.clone()));
        }
    }
}

// ============================================================================
// TYPE-LEVEL DIRECTIVES
// ============================================================================

fn emit_self_type(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    let name = ctx.parsed_type.name();
    output.extend(quote! { #name });
}

// ============================================================================
// LOOPING DIRECTIVES
// ============================================================================

/// `@for_variant { ... }` - loop over enum variants
fn handle_for_variant(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return; // Malformed - no body
    };

    let body = body_group.stream();

    // Only works for enums
    let facet_macro_parse::PType::Enum(e) = ctx.parsed_type else {
        return;
    };

    for variant in &e.variants {
        ctx.push(ContextFrame::Variant { variant });
        let expanded = evaluate_with_context(body.clone(), ctx);
        output.extend(expanded);
        ctx.pop();
    }
}

/// `@for_field { ... }` - loop over fields of current context
fn handle_for_field(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    let body = body_group.stream();

    let Some(fields) = ctx.current_fields() else {
        return;
    };

    // Need to collect to avoid borrow issues
    let fields: Vec<_> = fields.iter().enumerate().collect();

    for (index, field) in fields {
        ctx.push(ContextFrame::Field { field, index });
        let expanded = evaluate_with_context(body.clone(), ctx);
        output.extend(expanded);
        ctx.pop();
    }
}

// ============================================================================
// CONDITIONAL DIRECTIVES
// ============================================================================

/// `@if_attr(ns::key) { ... }` - conditional on current context having attr
fn handle_if_attr(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    // Parse (ns::key)
    let Some(proc_macro2::TokenTree::Group(query_group)) = iter.next() else {
        return;
    };

    // Parse { body }
    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    let Some(query) = AttrQuery::parse(query_group.stream()) else {
        return;
    };

    let attrs = ctx.current_attrs();

    if let Some(matched_attr) = query.find_in(&attrs.facet) {
        ctx.push(ContextFrame::Attr { attr: matched_attr });
        let expanded = evaluate_with_context(body_group.stream(), ctx);
        output.extend(expanded);
        ctx.pop();
    }
}

/// `@if_field_attr(ns::key) { ... }` - find field with attr, bind field context
///
/// This is a combined "find + bind" - it searches all fields in current context
/// for one with the given attribute, and if found, enters both field and attr context.
fn handle_if_field_attr(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(query_group)) = iter.next() else {
        return;
    };

    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    let Some(query) = AttrQuery::parse(query_group.stream()) else {
        return;
    };

    let Some(fields) = ctx.current_fields() else {
        return;
    };

    // Need to collect to avoid borrow issues
    let fields: Vec<_> = fields.iter().enumerate().collect();

    // Find first field with matching attr
    for (index, field) in fields {
        if let Some(matched_attr) = query.find_in(&field.attrs.facet) {
            ctx.push(ContextFrame::Field { field, index });
            ctx.push(ContextFrame::Attr { attr: matched_attr });
            let expanded = evaluate_with_context(body_group.stream(), ctx);
            output.extend(expanded);
            ctx.pop(); // attr
            ctx.pop(); // field
            return; // Only emit once for first match
        }
    }
}

/// `@if_any_field_attr(ns::key) { ... }` - conditional if ANY field has attr
///
/// Unlike `@if_field_attr`, this doesn't bind a specific field - it just checks
/// if any field has the attribute. Useful for wrapping a `@for_field` that will
/// check each field individually.
fn handle_if_any_field_attr(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(query_group)) = iter.next() else {
        return;
    };

    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    let Some(query) = AttrQuery::parse(query_group.stream()) else {
        return;
    };

    let Some(fields) = ctx.current_fields() else {
        return;
    };

    // Check if any field has the attr
    let has_any = fields
        .iter()
        .any(|f| query.find_in(&f.attrs.facet).is_some());

    if has_any {
        let expanded = evaluate_with_context(body_group.stream(), ctx);
        output.extend(expanded);
    }
}

/// `@if_struct { ... }` - emit body only for struct types
fn handle_if_struct(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    if matches!(ctx.parsed_type, facet_macro_parse::PType::Struct(_)) {
        let expanded = evaluate_with_context(body_group.stream(), ctx);
        output.extend(expanded);
    }
}

/// `@if_enum { ... }` - emit body only for enum types
fn handle_if_enum(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    if matches!(ctx.parsed_type, facet_macro_parse::PType::Enum(_)) {
        let expanded = evaluate_with_context(body_group.stream(), ctx);
        output.extend(expanded);
    }
}

/// `@if_unit_variant { ... }` - conditional on current variant being a unit variant
fn handle_if_unit_variant(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    let Some(variant) = ctx.current_variant() else {
        return;
    };

    if matches!(variant.kind, facet_macro_parse::PVariantKind::Unit) {
        let expanded = evaluate_with_context(body_group.stream(), ctx);
        output.extend(expanded);
    }
}

/// `@if_tuple_variant { ... }` - conditional on current variant being a tuple variant
fn handle_if_tuple_variant(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    let Some(variant) = ctx.current_variant() else {
        return;
    };

    if matches!(variant.kind, facet_macro_parse::PVariantKind::Tuple { .. }) {
        let expanded = evaluate_with_context(body_group.stream(), ctx);
        output.extend(expanded);
    }
}

/// `@if_struct_variant { ... }` - conditional on current variant being a struct variant
fn handle_if_struct_variant(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &mut EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(body_group)) = iter.next() else {
        return;
    };

    let Some(variant) = ctx.current_variant() else {
        return;
    };

    if matches!(variant.kind, facet_macro_parse::PVariantKind::Struct { .. }) {
        let expanded = evaluate_with_context(body_group.stream(), ctx);
        output.extend(expanded);
    }
}

// ============================================================================
// CONTEXT ACCESSORS
// ============================================================================

fn emit_variant_name(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    if let Some(variant) = ctx.current_variant()
        && let facet_macro_parse::IdentOrLiteral::Ident(name) = &variant.name.raw
    {
        output.extend(quote! { #name });
    }
}

fn emit_variant_pattern(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    let Some(variant) = ctx.current_variant() else {
        return;
    };

    use facet_macro_parse::{IdentOrLiteral, PVariantKind};

    match &variant.kind {
        PVariantKind::Unit => {
            // No pattern needed for unit variants
        }
        PVariantKind::Tuple { fields } => {
            // Use v0, v1, etc. for legacy compatibility with @format_doc_comment
            // (which uses {0}, {1} placeholders that expect v0, v1, etc.)
            let names: Vec<_> = (0..fields.len())
                .map(|i| quote::format_ident!("v{}", i))
                .collect();
            output.extend(quote! { ( #(#names),* ) });
        }
        PVariantKind::Struct { fields } => {
            let names: Vec<_> = fields
                .iter()
                .filter_map(|f| {
                    if let IdentOrLiteral::Ident(id) = &f.name.raw {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();
            output.extend(quote! { { #(#names),* } });
        }
    }
}

/// `@variant_pattern_only(ns::key) { ... }` - generate pattern binding only fields with attribute
fn handle_variant_pattern_only(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ctx: &EvalContext<'_>,
    output: &mut TokenStream,
) {
    let Some(proc_macro2::TokenTree::Group(query_group)) = iter.next() else {
        return;
    };

    let Some(query) = AttrQuery::parse(query_group.stream()) else {
        return;
    };

    let Some(variant) = ctx.current_variant() else {
        return;
    };

    use facet_macro_parse::{IdentOrLiteral, PVariantKind};

    match &variant.kind {
        PVariantKind::Unit => {
            // No pattern needed for unit variants
        }
        PVariantKind::Tuple { fields } => {
            let patterns: Vec<_> = fields
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    if query.find_in(&f.attrs.facet).is_some() {
                        let name = quote::format_ident!("v{}", i);
                        quote! { #name }
                    } else {
                        quote! { _ }
                    }
                })
                .collect();
            output.extend(quote! { ( #(#patterns),* ) });
        }
        PVariantKind::Struct { fields } => {
            let bindings: Vec<_> = fields
                .iter()
                .filter_map(|f| {
                    if let IdentOrLiteral::Ident(id) = &f.name.raw {
                        if query.find_in(&f.attrs.facet).is_some() {
                            Some(quote! { #id })
                        } else {
                            Some(quote! { #id: _ })
                        }
                    } else {
                        None
                    }
                })
                .collect();
            output.extend(quote! { { #(#bindings),* } });
        }
    }
}

fn emit_field_name(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    let Some((field, index)) = ctx.current_field() else {
        return;
    };

    use facet_macro_parse::IdentOrLiteral;

    match &field.name.raw {
        IdentOrLiteral::Ident(id) => {
            output.extend(quote! { #id });
        }
        IdentOrLiteral::Literal(_) => {
            // Tuple field - use generated name matching @variant_pattern (v0, v1, etc.)
            let name = quote::format_ident!("v{}", index);
            output.extend(quote! { #name });
        }
    }
}

fn emit_field_type(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    if let Some((field, _)) = ctx.current_field() {
        let ty = &field.ty;
        output.extend(quote! { #ty });
    }
}

fn emit_field_expr(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    // Same as field_name for now, but could be different for self.field access
    emit_field_name(ctx, output);
}

fn emit_attr_args(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    if let Some(attr) = ctx.current_attr() {
        let args = &attr.args;
        output.extend(args.clone());
    }
}

fn emit_doc(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    let attrs = ctx.current_attrs();
    let doc = attrs.doc.join(" ").trim().to_string();
    if !doc.is_empty() {
        output.extend(quote! { #doc });
    }
}

// ============================================================================
// DEFAULT-RELATED DIRECTIVES
// ============================================================================

/// `@field_default_expr` - emit the default expression for the current field
///
/// Checks for:
/// - `#[facet(default = literal)]` (builtin) → `literal` (direct value)
/// - `#[facet(default)]` (builtin, no value) → `::core::default::Default::default()`
/// - `#[facet(default::value = literal)]` → `literal.into()`
/// - `#[facet(default::func = "path")]` → `path()`
/// - No attribute → `::core::default::Default::default()`
fn emit_field_default_expr(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    let Some((field, _)) = ctx.current_field() else {
        return;
    };

    // Check for builtin #[facet(default = ...)] attribute (ns is None, key is "default")
    if let Some(attr) = field
        .attrs
        .facet
        .iter()
        .find(|a| a.ns.is_none() && a.key == "default")
    {
        let args = &attr.args;
        if args.is_empty() {
            // #[facet(default)] without value - use Default::default()
            output.extend(quote! { ::core::default::Default::default() });
        } else {
            // #[facet(default = value)] - emit the value directly
            output.extend(quote! { #args });
        }
        return;
    }

    // No default attribute - use Default::default()
    output.extend(quote! { ::core::default::Default::default() });
}

/// `@variant_default_construction` - emit the construction for a default variant
///
/// - Unit variant → nothing
/// - Tuple variant → (Default::default(), ...)
/// - Struct variant → { field: Default::default(), ... }
fn emit_variant_default_construction(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    use facet_macro_parse::{IdentOrLiteral, PVariantKind};

    let Some(variant) = ctx.current_variant() else {
        return;
    };

    match &variant.kind {
        PVariantKind::Unit => {
            // Nothing to emit
        }
        PVariantKind::Tuple { fields } => {
            let defaults: Vec<_> = fields.iter().map(field_default_tokens).collect();
            output.extend(quote! { ( #(#defaults),* ) });
        }
        PVariantKind::Struct { fields } => {
            let field_inits: Vec<_> = fields
                .iter()
                .filter_map(|f| {
                    if let IdentOrLiteral::Ident(name) = &f.name.raw {
                        let default_expr = field_default_tokens(f);
                        Some(quote! { #name: #default_expr })
                    } else {
                        None
                    }
                })
                .collect();
            output.extend(quote! { { #(#field_inits),* } });
        }
    }
}

/// Helper to generate the default expression tokens for a field
fn field_default_tokens(field: &facet_macro_parse::PStructField) -> TokenStream {
    // Check for builtin #[facet(default = ...)] attribute (ns is None, key is "default")
    if let Some(attr) = field
        .attrs
        .facet
        .iter()
        .find(|a| a.ns.is_none() && a.key == "default")
    {
        let args = &attr.args;
        if args.is_empty() {
            // #[facet(default)] without value - use Default::default()
            return quote! { ::core::default::Default::default() };
        } else {
            // #[facet(default = value)] - emit the value directly
            return quote! { #args };
        }
    }

    // No default attribute - use Default::default()
    quote! { ::core::default::Default::default() }
}

// ============================================================================
// LEGACY DIRECTIVES (for backwards compatibility with existing facet-error)
// ============================================================================

/// Legacy `@format_doc_comment` - emits doc comment as format string with args
fn emit_format_doc_comment(ctx: &EvalContext<'_>, output: &mut TokenStream) {
    use facet_macro_parse::PVariantKind;

    let Some(variant) = ctx.current_variant() else {
        return;
    };

    let doc = variant.attrs.doc.join(" ").trim().to_string();
    let format_str = if doc.is_empty() {
        variant.name.original.clone()
    } else {
        doc
    };

    // Check if format string uses positional args like {0}
    match &variant.kind {
        PVariantKind::Unit => {
            output.extend(quote! { #format_str });
        }
        PVariantKind::Tuple { fields } => {
            if format_str.contains("{0}") {
                // Use v0, v1, etc. to match legacy @variant_pattern
                let field_names: Vec<_> = (0..fields.len())
                    .map(|i| quote::format_ident!("v{}", i))
                    .collect();
                output.extend(quote! { #format_str, #(#field_names),* });
            } else {
                output.extend(quote! { #format_str });
            }
        }
        PVariantKind::Struct { fields: _ } => {
            // For struct variants, Rust will resolve field names in format string
            output.extend(quote! { #format_str });
        }
    }
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
        assert_eq!(plugins.len(), 1);
        assert!(matches!(&plugins[0], PluginRef::Simple(name) if name == "Error"));
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
        assert_eq!(plugins.len(), 2);
        assert!(matches!(&plugins[0], PluginRef::Simple(name) if name == "Error"));
        assert!(matches!(&plugins[1], PluginRef::Simple(name) if name == "Display"));
    }

    #[test]
    fn test_extract_path_plugins() {
        let input = quote! {
            #[facet(derive(Error, facet_default::Default))]
            pub enum MyError {
                Unknown,
            }
        };

        let mut iter = input.to_token_iter();
        let parsed = iter.parse::<crate::Enum>().expect("Failed to parse enum");

        let plugins = extract_derive_plugins(&parsed.attributes);
        assert_eq!(plugins.len(), 2);
        assert!(matches!(&plugins[0], PluginRef::Simple(name) if name == "Error"));
        assert!(
            matches!(&plugins[1], PluginRef::Path { crate_name, plugin_name } if crate_name == "facet_default" && plugin_name == "Default")
        );
    }

    #[test]
    fn test_plugin_ref_crate_path() {
        let simple = PluginRef::Simple("Error".to_string());
        assert_eq!(simple.crate_path().to_string(), ":: facet_error");

        let path = PluginRef::Path {
            crate_name: "facet_default".to_string(),
            plugin_name: "Default".to_string(),
        };
        assert_eq!(path.crate_path().to_string(), ":: facet_default");
    }

    /// Test for issue #1679: derive(Default) combined with other attributes on the same line
    #[test]
    fn test_extract_derive_plugins_combined_attrs() {
        // This is the failing case from the issue: derive(Default) combined with rename_all
        let input = quote! {
            #[derive(Debug, Facet)]
            #[facet(rename_all = "kebab-case", derive(Default))]
            struct PreCommitConfig {
                generate_readmes: bool,
            }
        };

        let mut iter = input.to_token_iter();
        let parsed = iter
            .parse::<crate::Struct>()
            .expect("Failed to parse struct");

        let plugins = extract_derive_plugins(&parsed.attributes);
        assert_eq!(
            plugins.len(),
            1,
            "should extract derive(Default) even when combined with other attrs"
        );
        assert!(matches!(&plugins[0], PluginRef::Simple(name) if name == "Default"));
    }

    /// Test for issue #1679: strip_derive_attrs should strip only derive part, keeping other attrs
    #[test]
    fn test_strip_derive_attrs_combined() {
        // Input with derive(Default) combined with rename_all
        let input = quote! {
            #[derive(Debug, Facet)]
            #[facet(rename_all = "kebab-case", derive(Default))]
            struct PreCommitConfig {
                generate_readmes: bool,
            }
        };

        let stripped = strip_derive_attrs(input);
        let stripped_str = stripped.to_string();

        // Should keep #[derive(Debug, Facet)]
        assert!(
            stripped_str.contains("derive"),
            "should keep #[derive(Debug, Facet)]"
        );

        // Should keep rename_all in the facet attribute
        assert!(
            stripped_str.contains("rename_all"),
            "should keep rename_all attribute"
        );

        // Should NOT contain derive(Default) in facet attribute
        // The original has facet(rename_all = "kebab-case", derive(Default))
        // After stripping, it should have facet(rename_all = "kebab-case")
        assert!(
            !stripped_str.contains("facet (rename_all = \"kebab-case\" , derive (Default))"),
            "should strip derive(Default) from combined attribute"
        );
    }

    /// Test strip_derive_attrs with only derive in facet attribute
    #[test]
    fn test_strip_derive_attrs_only_derive() {
        let input = quote! {
            #[facet(derive(Default))]
            struct Foo {}
        };

        let stripped = strip_derive_attrs(input);
        let stripped_str = stripped.to_string();

        // The entire facet attribute should be stripped (or result in empty facet())
        // Since the facet attribute only contains derive(Default)
        assert!(
            !stripped_str.contains("derive (Default)"),
            "derive(Default) should be stripped"
        );
    }
}
