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
    let mut evaluator = TemplateEvaluator {
        parsed_type,
        output: TokenStream::new(),
    };
    evaluator.evaluate(template);
    evaluator.output
}

/// Template evaluator that processes @ directives
struct TemplateEvaluator<'a> {
    parsed_type: &'a facet_macro_parse::PType,
    output: TokenStream,
}

impl<'a> TemplateEvaluator<'a> {
    /// Evaluate a template token stream
    fn evaluate(&mut self, template: TokenStream) {
        let mut iter = template.into_iter().peekable();

        while let Some(tt) = iter.next() {
            match &tt {
                proc_macro2::TokenTree::Punct(p) if p.as_char() == '@' => {
                    // This is a directive - handle it
                    if let Some(next) = iter.next() {
                        match &next {
                            proc_macro2::TokenTree::Ident(id) => {
                                let directive = id.to_string();

                                match directive.as_str() {
                                    "Self" => self.emit_self_type(),
                                    "for_variant" => self.handle_for_variant(&mut iter),
                                    "if_has_source_field" => {
                                        self.handle_if_has_source_field(&mut iter)
                                    }
                                    "if_has_from_field" => self.handle_if_has_from_field(&mut iter),
                                    "variant_name" => { /* Will be filled by for_variant context */
                                    }
                                    "variant_pattern" => { /* Will be filled by for_variant context */
                                    }
                                    "format_doc_comment" => { /* Will be filled by for_variant context */
                                    }
                                    "source_pattern" => { /* Will be filled by conditional context */
                                    }
                                    "source_expr" => { /* Will be filled by conditional context */ }
                                    "from_field_type" => { /* Will be filled by conditional context */
                                    }
                                    _ => {
                                        // Unknown directive - emit as-is for now
                                        self.output.extend(std::iter::once(tt));
                                        self.output.extend(std::iter::once(next.clone()));
                                    }
                                }
                            }
                            _ => {
                                // Not an identifier after @ - just emit both
                                self.output.extend(std::iter::once(tt));
                                self.output.extend(std::iter::once(next.clone()));
                            }
                        }
                    } else {
                        // @ at end of stream - just emit it
                        self.output.extend(std::iter::once(tt));
                    }
                }
                proc_macro2::TokenTree::Group(g) => {
                    // Recursively evaluate groups
                    let evaluated = {
                        let mut evaluator = TemplateEvaluator {
                            parsed_type: self.parsed_type,
                            output: TokenStream::new(),
                        };
                        evaluator.evaluate(g.stream());
                        evaluator.output
                    };
                    let new_group = proc_macro2::Group::new(g.delimiter(), evaluated);
                    self.output
                        .extend(std::iter::once(proc_macro2::TokenTree::Group(new_group)));
                }
                _ => {
                    // Regular token - pass through
                    self.output.extend(std::iter::once(tt));
                }
            }
        }
    }

    fn emit_self_type(&mut self) {
        let name = self.parsed_type.name();
        self.output.extend(quote! { #name });
    }

    fn handle_for_variant(
        &mut self,
        iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ) {
        // Next should be { ... } containing the loop body
        if let Some(proc_macro2::TokenTree::Group(g)) = iter.next()
            && g.delimiter() == proc_macro2::Delimiter::Brace
        {
            let body = g.stream();

            // Only works for enums
            if let facet_macro_parse::PType::Enum(e) = self.parsed_type {
                for variant in &e.variants {
                    // Evaluate the body with variant context
                    let variant_code = self.evaluate_variant_body(body.clone(), variant);
                    self.output.extend(variant_code);
                }
            }
        }
    }

    fn handle_if_has_source_field(
        &mut self,
        iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ) {
        // TODO: Implement conditional for source fields
        // For now, just skip the block
        if let Some(proc_macro2::TokenTree::Group(_g)) = iter.next() {
            // Skip
        }
    }

    fn handle_if_has_from_field(
        &mut self,
        iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    ) {
        // TODO: Implement conditional for from fields
        // For now, just skip the block
        if let Some(proc_macro2::TokenTree::Group(_g)) = iter.next() {
            // Skip
        }
    }

    fn evaluate_variant_body(
        &self,
        body: TokenStream,
        variant: &facet_macro_parse::PVariant,
    ) -> TokenStream {
        let mut output = TokenStream::new();
        let mut iter = body.into_iter().peekable();

        while let Some(tt) = iter.next() {
            match &tt {
                proc_macro2::TokenTree::Punct(p) if p.as_char() == '@' => {
                    if let Some(next) = iter.next() {
                        if let proc_macro2::TokenTree::Ident(id) = &next {
                            let directive = id.to_string();

                            match directive.as_str() {
                                "variant_name" => {
                                    if let facet_macro_parse::IdentOrLiteral::Ident(name) =
                                        &variant.name.raw
                                    {
                                        output.extend(quote! { #name });
                                    }
                                }
                                "variant_pattern" => {
                                    output.extend(self.make_variant_pattern(variant));
                                }
                                "format_doc_comment" => {
                                    output.extend(self.make_format_doc_comment(variant));
                                }
                                "if_has_source_field" => {
                                    if let Some(proc_macro2::TokenTree::Group(g)) = iter.next()
                                        && self.variant_has_source_field(variant)
                                    {
                                        let evaluated =
                                            self.evaluate_source_body(g.stream(), variant);
                                        output.extend(evaluated);
                                    }
                                }
                                "if_has_from_field" => {
                                    if let Some(proc_macro2::TokenTree::Group(g)) = iter.next()
                                        && self.variant_has_from_field(variant)
                                    {
                                        let evaluated =
                                            self.evaluate_from_body(g.stream(), variant);
                                        output.extend(evaluated);
                                    }
                                }
                                _ => {
                                    // Unknown - emit as-is
                                    output.extend(std::iter::once(tt));
                                    output.extend(std::iter::once(next.clone()));
                                }
                            }
                        } else {
                            // Not an ident after @ - emit both
                            output.extend(std::iter::once(tt));
                            output.extend(std::iter::once(next.clone()));
                        }
                    } else {
                        // @ at end - just emit it
                        output.extend(std::iter::once(tt));
                    }
                }
                proc_macro2::TokenTree::Group(g) => {
                    // Recursively evaluate
                    let evaluated = self.evaluate_variant_body(g.stream(), variant);
                    let new_group = proc_macro2::Group::new(g.delimiter(), evaluated);
                    output.extend(std::iter::once(proc_macro2::TokenTree::Group(new_group)));
                }
                _ => {
                    output.extend(std::iter::once(tt));
                }
            }
        }

        output
    }

    fn make_variant_pattern(&self, variant: &facet_macro_parse::PVariant) -> TokenStream {
        use facet_macro_parse::{IdentOrLiteral, PVariantKind};

        match &variant.kind {
            PVariantKind::Unit => quote! {},
            PVariantKind::Tuple { fields } => {
                let field_names: Vec<_> = (0..fields.len())
                    .map(|i| quote::format_ident!("v{}", i))
                    .collect();
                quote! { ( #(#field_names),* ) }
            }
            PVariantKind::Struct { fields } => {
                let field_names: Vec<_> = fields
                    .iter()
                    .filter_map(|f| {
                        if let IdentOrLiteral::Ident(id) = &f.name.raw {
                            Some(quote! { #id })
                        } else {
                            None
                        }
                    })
                    .collect();
                quote! { { #(#field_names),* } }
            }
        }
    }

    fn make_format_doc_comment(&self, variant: &facet_macro_parse::PVariant) -> TokenStream {
        use facet_macro_parse::PVariantKind;

        let doc = variant.attrs.doc.join(" ").trim().to_string();
        let format_str = if doc.is_empty() {
            variant.name.effective.clone()
        } else {
            doc
        };

        // Check if format string uses positional args like {0}
        match &variant.kind {
            PVariantKind::Unit => {
                quote! { #format_str }
            }
            PVariantKind::Tuple { fields } => {
                if format_str.contains("{0}") {
                    let field_names: Vec<_> = (0..fields.len())
                        .map(|i| quote::format_ident!("v{}", i))
                        .collect();
                    quote! { #format_str, #(#field_names),* }
                } else {
                    quote! { #format_str }
                }
            }
            PVariantKind::Struct { fields: _ } => {
                // For struct variants, Rust will resolve field names in format string
                quote! { #format_str }
            }
        }
    }

    fn variant_has_source_field(&self, variant: &facet_macro_parse::PVariant) -> bool {
        use facet_macro_parse::PVariantKind;

        match &variant.kind {
            PVariantKind::Tuple { fields } if fields.len() == 1 => {
                fields[0].attrs.facet.iter().any(|attr| {
                    if let Some(ns) = &attr.ns {
                        *ns == "error" && (attr.key == "source" || attr.key == "from")
                    } else {
                        false
                    }
                })
            }
            PVariantKind::Struct { fields } => fields.iter().any(|f| {
                f.attrs.facet.iter().any(|attr| {
                    if let Some(ns) = &attr.ns {
                        *ns == "error" && (attr.key == "source" || attr.key == "from")
                    } else {
                        false
                    }
                })
            }),
            _ => false,
        }
    }

    fn variant_has_from_field(&self, variant: &facet_macro_parse::PVariant) -> bool {
        use facet_macro_parse::PVariantKind;

        matches!(&variant.kind, PVariantKind::Tuple { fields } if fields.len() == 1 && fields[0].attrs.facet.iter().any(|attr| {
            if let Some(ns) = &attr.ns {
                *ns == "error" && attr.key == "from"
            } else {
                false
            }
        }))
    }

    fn evaluate_source_body(
        &self,
        body: TokenStream,
        variant: &facet_macro_parse::PVariant,
    ) -> TokenStream {
        use facet_macro_parse::IdentOrLiteral;
        let mut output = TokenStream::new();
        let mut iter = body.into_iter();

        while let Some(tt) = iter.next() {
            match &tt {
                proc_macro2::TokenTree::Punct(p) if p.as_char() == '@' => {
                    if let Some(next) = iter.next()
                        && let proc_macro2::TokenTree::Ident(id) = &next
                    {
                        match id.to_string().as_str() {
                            "variant_name" => {
                                if let IdentOrLiteral::Ident(name) = &variant.name.raw {
                                    output.extend(quote! { #name });
                                }
                            }
                            "source_pattern" => {
                                output.extend(self.make_source_pattern(variant));
                            }
                            "source_expr" => {
                                output.extend(self.make_source_expr(variant));
                            }
                            _ => {
                                output.extend(std::iter::once(tt));
                                output.extend(std::iter::once(next.clone()));
                            }
                        }
                    }
                }
                proc_macro2::TokenTree::Group(g) => {
                    let evaluated = self.evaluate_source_body(g.stream(), variant);
                    let new_group = proc_macro2::Group::new(g.delimiter(), evaluated);
                    output.extend(std::iter::once(proc_macro2::TokenTree::Group(new_group)));
                }
                _ => {
                    output.extend(std::iter::once(tt));
                }
            }
        }

        output
    }

    #[allow(clippy::only_used_in_recursion)]
    fn evaluate_from_body(
        &self,
        body: TokenStream,
        variant: &facet_macro_parse::PVariant,
    ) -> TokenStream {
        use facet_macro_parse::{IdentOrLiteral, PVariantKind};
        let mut output = TokenStream::new();
        let mut iter = body.into_iter();

        while let Some(tt) = iter.next() {
            match &tt {
                proc_macro2::TokenTree::Punct(p) if p.as_char() == '@' => {
                    if let Some(next) = iter.next()
                        && let proc_macro2::TokenTree::Ident(id) = &next
                    {
                        match id.to_string().as_str() {
                            "from_field_type" => {
                                if let PVariantKind::Tuple { fields } = &variant.kind
                                    && let Some(field) = fields.first()
                                {
                                    let ty = &field.ty;
                                    output.extend(quote! { #ty });
                                }
                            }
                            "variant_name" => {
                                if let IdentOrLiteral::Ident(name) = &variant.name.raw {
                                    output.extend(quote! { #name });
                                }
                            }
                            _ => {
                                output.extend(std::iter::once(tt));
                                output.extend(std::iter::once(next.clone()));
                            }
                        }
                    }
                }
                proc_macro2::TokenTree::Group(g) => {
                    let evaluated = self.evaluate_from_body(g.stream(), variant);
                    let new_group = proc_macro2::Group::new(g.delimiter(), evaluated);
                    output.extend(std::iter::once(proc_macro2::TokenTree::Group(new_group)));
                }
                _ => {
                    output.extend(std::iter::once(tt));
                }
            }
        }

        output
    }

    fn make_source_pattern(&self, variant: &facet_macro_parse::PVariant) -> TokenStream {
        use facet_macro_parse::PVariantKind;

        match &variant.kind {
            PVariantKind::Tuple { .. } => {
                quote! { (ref e) }
            }
            PVariantKind::Struct { fields } => {
                // Find the field with error::source or error::from
                for field in fields {
                    if field.attrs.facet.iter().any(|attr| {
                        if let Some(ns) = &attr.ns {
                            *ns == "error" && (attr.key == "source" || attr.key == "from")
                        } else {
                            false
                        }
                    }) && let facet_macro_parse::IdentOrLiteral::Ident(name) = &field.name.raw
                    {
                        return quote! { { #name, .. } };
                    }
                }
                quote! { { .. } }
            }
            _ => quote! {},
        }
    }

    fn make_source_expr(&self, _variant: &facet_macro_parse::PVariant) -> TokenStream {
        // For now, just return 'e' which should be bound by source_pattern
        quote! { e }
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
