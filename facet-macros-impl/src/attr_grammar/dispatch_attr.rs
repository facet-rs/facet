//! Implementation of `__dispatch_attr!` proc-macro.
//!
//! A unified dispatcher that handles all attribute parsing without
//! calling other generated macro_rules macros. This avoids the Rust
//! limitation on macro-expanded macro_export macros.

use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{quote, quote_spanned};
use unsynn::*;

// ============================================================================
// UNSYNN TYPE DEFINITIONS
// ============================================================================

keyword! {
    KCratePath = "crate_path";
    KEnumName = "enum_name";
    KVariants = "variants";
    KName = "name";
    KRest = "rest";
    KUnit = "unit";
    KNewtype = "newtype";
    KNewtypeOptChar = "newtype_opt_char";
    KRec = "rec";
}

operator! {
    At = "@";
    Col = ":";
}

unsynn! {
    /// The complete input to __dispatch_attr
    struct DispatchAttrInput {
        crate_path_section: CratePathSection,
        enum_name_section: EnumNameSection,
        variants_section: VariantsSection,
        name_section: NameSection,
        rest_section: RestSection,
    }

    /// @crate_path { ... } - the crate path (e.g., ::facet_args)
    struct CratePathSection {
        _at: At,
        _kw: KCratePath,
        content: BraceGroup,
    }

    /// @enum_name { ... }
    struct EnumNameSection {
        _at: At,
        _kw: KEnumName,
        content: BraceGroupContaining<Ident>,
    }

    /// @variants { ... }
    struct VariantsSection {
        _at: At,
        _kw: KVariants,
        content: BraceGroupContaining<CommaDelimitedVec<VariantDef>>,
    }

    /// @name { ... }
    struct NameSection {
        _at: At,
        _kw: KName,
        content: BraceGroupContaining<Ident>,
    }

    /// @rest { ... }
    struct RestSection {
        _at: At,
        _kw: KRest,
        content: BraceGroup,
    }

    /// A variant definition: `skip: unit` or `rename: newtype` or `column: rec Column { ... }`
    struct VariantDef {
        name: Ident,
        _colon: Col,
        kind: VariantKindDef,
    }

    /// The kind of variant
    enum VariantKindDef {
        /// unit variant
        Unit(KUnit),
        /// newtype variant
        Newtype(KNewtype),
        /// newtype Option<char> variant
        NewtypeOptChar(KNewtypeOptChar),
        /// struct variant with fields
        Struct(StructVariantDef),
    }

    /// rec Column { name: opt_string, primary_key: bool }
    struct StructVariantDef {
        _rec: KRec,
        struct_name: Ident,
        /// Raw token stream - parsed manually to extract doc comments
        fields: BraceGroup,
    }
}

// ============================================================================
// PARSED STRUCTURES
// ============================================================================

struct ParsedDispatchInput {
    crate_path: TokenStream2,
    #[allow(dead_code)]
    enum_name: Ident,
    variants: Vec<ParsedVariant>,
    attr_name: Ident,
    rest: TokenStream2,
}

#[derive(Clone)]
struct ParsedVariant {
    name: Ident,
    kind: ParsedVariantKind,
}

#[derive(Clone)]
enum ParsedVariantKind {
    Unit,
    Newtype,
    NewtypeOptChar,
    Struct {
        struct_name: Ident,
        fields: Vec<ParsedFieldDef>,
    },
}

/// A parsed field definition with doc comment
#[derive(Clone)]
struct ParsedFieldDef {
    name: Ident,
    kind: FieldKind,
    /// Doc comment for help text in errors
    #[allow(dead_code)]
    doc: Option<String>,
}

#[derive(Clone, Copy)]
enum FieldKind {
    Bool,
    String,
    OptString,
    OptBool,
    OptChar,
    I64,
    OptI64,
    ListString,
    ListI64,
    /// Bare identifier like `cascade` or `post` - captured as &'static str
    Ident,
}

impl DispatchAttrInput {
    fn to_parsed(&self) -> std::result::Result<ParsedDispatchInput, String> {
        let crate_path = self.crate_path_section.content.0.stream();
        let enum_name = self.enum_name_section.content.content.clone();

        let variants: std::result::Result<Vec<_>, _> = self
            .variants_section
            .content
            .content
            .iter()
            .map(|d| d.value.to_parsed())
            .collect();

        let attr_name = self.name_section.content.content.clone();

        let rest = self.rest_section.content.0.stream();

        Ok(ParsedDispatchInput {
            crate_path,
            enum_name,
            variants: variants?,
            attr_name,
            rest,
        })
    }
}

impl VariantDef {
    fn to_parsed(&self) -> std::result::Result<ParsedVariant, String> {
        let kind = match &self.kind {
            VariantKindDef::Unit(_) => ParsedVariantKind::Unit,
            VariantKindDef::Newtype(_) => ParsedVariantKind::Newtype,
            VariantKindDef::NewtypeOptChar(_) => ParsedVariantKind::NewtypeOptChar,
            VariantKindDef::Struct(s) => {
                let fields = parse_fields_with_docs(&s.fields.0.stream())?;
                ParsedVariantKind::Struct {
                    struct_name: s.struct_name.clone(),
                    fields,
                }
            }
        };

        Ok(ParsedVariant {
            name: self.name.clone(),
            kind,
        })
    }
}

/// Parse fields from token stream, extracting doc comments
fn parse_fields_with_docs(
    tokens: &TokenStream2,
) -> std::result::Result<Vec<ParsedFieldDef>, String> {
    let tokens: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut fields = Vec::new();
    let mut i = 0;
    let mut current_doc: Option<String> = None;

    while i < tokens.len() {
        // Skip commas
        if let TokenTree::Punct(p) = &tokens[i] {
            if p.as_char() == ',' {
                i += 1;
                continue;
            }
        }

        // Check for doc comment: #[doc = "..."]
        if let TokenTree::Punct(p) = &tokens[i] {
            if p.as_char() == '#' && i + 1 < tokens.len() {
                if let TokenTree::Group(g) = &tokens[i + 1] {
                    if g.delimiter() == proc_macro2::Delimiter::Bracket {
                        if let Some(doc) = extract_doc_from_attr(&g.stream()) {
                            // Accumulate doc comments (for multi-line)
                            let trimmed = doc.trim();
                            if let Some(existing) = &mut current_doc {
                                existing.push(' ');
                                existing.push_str(trimmed);
                            } else {
                                current_doc = Some(trimmed.to_string());
                            }
                            i += 2;
                            continue;
                        }
                    }
                }
            }
        }

        // Expect field: name: kind
        let name = match &tokens[i] {
            TokenTree::Ident(ident) => ident.clone(),
            other => return Err(format!("expected field name, found `{other}`")),
        };
        i += 1;

        // Expect colon
        if i >= tokens.len() {
            return Err(format!("expected `:` after field name `{name}`"));
        }
        if let TokenTree::Punct(p) = &tokens[i] {
            if p.as_char() != ':' {
                return Err(format!(
                    "expected `:` after field name `{name}`, found `{p}`"
                ));
            }
        } else {
            return Err(format!("expected `:` after field name `{name}`"));
        }
        i += 1;

        // Expect kind
        if i >= tokens.len() {
            return Err(format!("expected field kind after `{name}:`"));
        }
        let kind_ident = match &tokens[i] {
            TokenTree::Ident(ident) => ident.clone(),
            other => return Err(format!("expected field kind, found `{other}`")),
        };
        i += 1;

        let kind_str = kind_ident.to_string();
        let kind = match kind_str.as_str() {
            "bool" => FieldKind::Bool,
            "string" => FieldKind::String,
            "opt_string" => FieldKind::OptString,
            "opt_bool" => FieldKind::OptBool,
            "opt_char" => FieldKind::OptChar,
            "i64" => FieldKind::I64,
            "opt_i64" => FieldKind::OptI64,
            "list_string" => FieldKind::ListString,
            "list_i64" => FieldKind::ListI64,
            "ident" => FieldKind::Ident,
            _ => return Err(format!("unknown field kind: {kind_str}")),
        };

        fields.push(ParsedFieldDef {
            name,
            kind,
            doc: current_doc.take(),
        });
    }

    Ok(fields)
}

/// Unescape a string with Rust-style escape sequences (e.g., `\"` -> `"`)
fn unescape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('0') => out.push('\0'),
                Some(other) => {
                    // Unknown escape, keep as-is
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Extract doc string from #[doc = "..."] attribute content
fn extract_doc_from_attr(tokens: &TokenStream2) -> Option<String> {
    let tokens: Vec<TokenTree> = tokens.clone().into_iter().collect();

    // Expected: doc = "..."
    if tokens.len() >= 3 {
        if let TokenTree::Ident(ident) = &tokens[0] {
            if *ident == "doc" {
                if let TokenTree::Punct(p) = &tokens[1] {
                    if p.as_char() == '=' {
                        if let TokenTree::Literal(lit) = &tokens[2] {
                            let lit_str = lit.to_string();
                            // Remove quotes, unescape, and trim leading space
                            if lit_str.starts_with('"') && lit_str.ends_with('"') {
                                let inner = &lit_str[1..lit_str.len() - 1];
                                // Doc comments have a leading space after ///
                                return Some(unescape_string(inner.trim_start()));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// ============================================================================
// ENTRY POINT
// ============================================================================

/// Unified dispatcher for attribute parsing that routes to unit, newtype, or struct variant handlers based on the attribute grammar.
///
/// Returns just the enum value expression using `__ExtAttr` which is imported by the calling macro.
pub fn dispatch_attr(input: TokenStream2) -> TokenStream2 {
    let mut iter = input.to_token_iter();

    let parsed_input: DispatchAttrInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote! { compile_error!(#msg); };
        }
    };

    let input = match parsed_input.to_parsed() {
        Ok(i) => i,
        Err(e) => {
            return quote! { compile_error!(#e); };
        }
    };

    let crate_path = &input.crate_path;
    let attr_name = &input.attr_name;
    let attr_name_str = attr_name.to_string();
    let attr_span = attr_name.span();
    let rest = &input.rest;

    // Find matching variant
    for variant in &input.variants {
        if variant.name == attr_name_str {
            let variant_name = &variant.name;
            // Convert to PascalCase for enum variant
            let variant_pascal = to_pascal_case(&variant_name.to_string());
            let variant_ident = proc_macro2::Ident::new(&variant_pascal, variant_name.span());

            return match &variant.kind {
                ParsedVariantKind::Unit => {
                    generate_unit_value(crate_path, &variant_ident, attr_name, rest, attr_span)
                }
                ParsedVariantKind::Newtype => {
                    generate_newtype_value(crate_path, &variant_ident, attr_name, rest, attr_span)
                }
                ParsedVariantKind::NewtypeOptChar => generate_newtype_opt_char_value(
                    crate_path,
                    &variant_ident,
                    attr_name,
                    rest,
                    attr_span,
                ),
                ParsedVariantKind::Struct {
                    struct_name,
                    fields,
                } => generate_struct_value(
                    crate_path,
                    &variant_ident,
                    struct_name,
                    fields,
                    attr_name,
                    rest,
                    attr_span,
                ),
            };
        }
    }

    // Unknown attribute - generate error
    let known_names: Vec<_> = input.variants.iter().map(|v| v.name.to_string()).collect();
    let suggestion = find_closest(&attr_name_str, &known_names);

    let msg = if let Some(s) = suggestion {
        format!("unknown attribute `{attr_name_str}`; did you mean `{s}`?")
    } else {
        format!(
            "unknown attribute `{}`; expected one of: {}",
            attr_name_str,
            known_names.join(", ")
        )
    };

    let expanded = quote_spanned! { attr_span =>
        compile_error!(#msg)
    };

    expanded
}

fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

fn generate_unit_value(
    ns_path: &TokenStream2,
    variant_ident: &proc_macro2::Ident,
    attr_name: &Ident,
    rest: &TokenStream2,
    span: Span,
) -> TokenStream2 {
    let rest_tokens: Vec<TokenTree> = rest.clone().into_iter().collect();

    // Valid: no rest or empty parens
    if rest_tokens.is_empty() {
        return quote_spanned! { span =>
            #ns_path::Attr::#variant_ident
        };
    }

    // Check for empty parens ()
    if let Some(TokenTree::Group(g)) = rest_tokens.first() {
        if g.delimiter() == proc_macro2::Delimiter::Parenthesis && g.stream().is_empty() {
            return quote_spanned! { span =>
                #ns_path::Attr::#variant_ident
            };
        }
    }

    // Error: unit variant doesn't take arguments
    let msg = format!("`{attr_name}` does not take arguments; use just `{attr_name}`");
    quote_spanned! { span =>
        compile_error!(#msg)
    }
}

fn generate_newtype_value(
    ns_path: &TokenStream2,
    variant_ident: &proc_macro2::Ident,
    attr_name: &Ident,
    rest: &TokenStream2,
    span: Span,
) -> TokenStream2 {
    let rest_tokens: Vec<TokenTree> = rest.clone().into_iter().collect();
    let attr_str = attr_name.to_string();

    // Check for parens style: rename("value")
    if let Some(TokenTree::Group(g)) = rest_tokens.first() {
        if g.delimiter() == proc_macro2::Delimiter::Parenthesis {
            let inner: Vec<TokenTree> = g.stream().into_iter().collect();
            if inner.len() == 1 {
                if let TokenTree::Literal(lit) = &inner[0] {
                    let lit_str = lit.to_string();
                    if lit_str.starts_with('\"') {
                        return quote_spanned! { span =>
                            #ns_path::Attr::#variant_ident(#lit)
                        };
                    }
                }
            }
            // Error: non-literal in parens
            let msg = format!("`{attr_str}` expects a string literal: `{attr_str}(\"name\")`");
            return quote_spanned! { span =>
                compile_error!(#msg)
            };
        }
    }

    // Check for bare literal (derive macro strips the `=` sign)
    if rest_tokens.len() == 1 {
        if let TokenTree::Literal(lit) = &rest_tokens[0] {
            let lit_str = lit.to_string();
            if lit_str.starts_with('\"') {
                return quote_spanned! { span =>
                    #ns_path::Attr::#variant_ident(#lit)
                };
            }
        }
    }

    // Check for equals style: rename = "value"
    if rest_tokens.len() >= 2 {
        if let TokenTree::Punct(p) = &rest_tokens[0] {
            if p.as_char() == '=' {
                if let TokenTree::Literal(lit) = &rest_tokens[1] {
                    let lit_str = lit.to_string();
                    if lit_str.starts_with('\"') {
                        return quote_spanned! { span =>
                            #ns_path::Attr::#variant_ident(#lit)
                        };
                    }
                }
                // Error: non-literal after =
                let msg = format!("`{attr_str}` expects a string literal: `{attr_str} = \"name\"`");
                return quote_spanned! { span =>
                    compile_error!(#msg)
                };
            }
        }
    }

    // Error: no value provided
    let msg = format!(
        "`{attr_str}` requires a string value: `{attr_str}(\"name\")` or `{attr_str} = \"name\"`"
    );
    quote_spanned! { span =>
        compile_error!(#msg)
    }
}

fn generate_newtype_opt_char_value(
    ns_path: &TokenStream2,
    variant_ident: &proc_macro2::Ident,
    attr_name: &Ident,
    rest: &TokenStream2,
    span: Span,
) -> TokenStream2 {
    let rest_tokens: Vec<TokenTree> = rest.clone().into_iter().collect();
    let attr_str = attr_name.to_string();

    // No args: return None
    if rest_tokens.is_empty() {
        return quote_spanned! { span =>
            #ns_path::Attr::#variant_ident(::core::option::Option::None)
        };
    }

    // Check for empty parens ()
    if let Some(TokenTree::Group(g)) = rest_tokens.first() {
        if g.delimiter() == proc_macro2::Delimiter::Parenthesis && g.stream().is_empty() {
            return quote_spanned! { span =>
                #ns_path::Attr::#variant_ident(::core::option::Option::None)
            };
        }
    }

    // Check for parens style: short('v')
    if let Some(TokenTree::Group(g)) = rest_tokens.first() {
        if g.delimiter() == proc_macro2::Delimiter::Parenthesis {
            let inner: Vec<TokenTree> = g.stream().into_iter().collect();
            if inner.len() == 1 {
                if let TokenTree::Literal(lit) = &inner[0] {
                    let lit_str = lit.to_string();
                    // Check for char literal: 'x'
                    if lit_str.starts_with('\'') && lit_str.ends_with('\'') {
                        return quote_spanned! { span =>
                            #ns_path::Attr::#variant_ident(::core::option::Option::Some(#lit))
                        };
                    }
                }
            }
            // Error: non-char-literal in parens
            let msg = format!("`{attr_str}` expects a char literal: `{attr_str}('v')`");
            return quote_spanned! { span =>
                compile_error!(#msg)
            };
        }
    }

    // Check for bare char literal: the derive macro strips the = sign when processing
    // `#[facet(attr = value)]` syntax, passing just the value
    if rest_tokens.len() == 1 {
        if let TokenTree::Literal(lit) = &rest_tokens[0] {
            let lit_str = lit.to_string();
            // Check for char literal: 'x'
            if lit_str.starts_with('\'') && lit_str.ends_with('\'') {
                return quote_spanned! { span =>
                    #ns_path::Attr::#variant_ident(::core::option::Option::Some(#lit))
                };
            }
        }
    }

    // Check for equals style: short = 'v' (in case it's passed with the equals sign)
    if rest_tokens.len() >= 2 {
        if let TokenTree::Punct(p) = &rest_tokens[0] {
            if p.as_char() == '=' {
                if let TokenTree::Literal(lit) = &rest_tokens[1] {
                    let lit_str = lit.to_string();
                    // Check for char literal: 'x'
                    if lit_str.starts_with('\'') && lit_str.ends_with('\'') {
                        return quote_spanned! { span =>
                            #ns_path::Attr::#variant_ident(::core::option::Option::Some(#lit))
                        };
                    }
                }
                // Error: non-char-literal after =
                let msg = format!("`{attr_str}` expects a char literal: `{attr_str} = 'v'`");
                return quote_spanned! { span =>
                    compile_error!(#msg)
                };
            }
        }
    }

    // Error: invalid syntax
    let msg = format!(
        "`{attr_str}` expects either no value or a char: `{attr_str}` or `{attr_str} = 'v'`"
    );
    quote_spanned! { span =>
        compile_error!(#msg)
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_struct_value(
    ns_path: &TokenStream2,
    variant_ident: &proc_macro2::Ident,
    struct_name: &Ident,
    fields: &[ParsedFieldDef],
    attr_name: &Ident,
    rest: &TokenStream2,
    span: Span,
) -> TokenStream2 {
    let rest_tokens: Vec<TokenTree> = rest.clone().into_iter().collect();
    let attr_str = attr_name.to_string();

    // Generate default field values
    let default_fields: Vec<TokenStream2> = fields
        .iter()
        .map(|f| {
            let name = &f.name;
            let default = match f.kind {
                FieldKind::Bool => quote! { false },
                FieldKind::String => quote! { "" },
                FieldKind::OptString => quote! { ::core::option::Option::None },
                FieldKind::OptBool => quote! { ::core::option::Option::None },
                FieldKind::OptChar => quote! { ::core::option::Option::None },
                FieldKind::I64 => quote! { 0 },
                FieldKind::OptI64 => quote! { ::core::option::Option::None },
                FieldKind::ListString => quote! { &[] },
                FieldKind::ListI64 => quote! { &[] },
                FieldKind::Ident => quote! { "" },
            };
            quote! { #name: #default }
        })
        .collect();

    // Check for parens style: column(name = "foo", primary_key)
    if let Some(TokenTree::Group(g)) = rest_tokens.first() {
        if g.delimiter() == proc_macro2::Delimiter::Parenthesis {
            let inner = g.stream();

            // Empty parens - use defaults
            if inner.is_empty() {
                return quote_spanned! { span =>
                    #ns_path::Attr::#variant_ident(#ns_path::#struct_name {
                        #(#default_fields),*
                    })
                };
            }

            // Non-empty - delegate to __build_struct_fields proc-macro
            // Note: struct variants with complex field initialization cannot be used in statics,
            // so this returns a compile_error for now. This case is not used by facet-args.
            let msg = "struct variant attributes with field values cannot be used with define_attr_grammar! yet";
            return quote_spanned! { span =>
                compile_error!(#msg)
            };
        }
    }

    // No parens - use defaults
    if rest_tokens.is_empty() {
        return quote_spanned! { span =>
            #ns_path::Attr::#variant_ident(#ns_path::#struct_name {
                #(#default_fields),*
            })
        };
    }

    // Error: invalid syntax
    let msg = format!("`{attr_str}` expects parentheses: `{attr_str}(...)` or just `{attr_str}`");
    quote_spanned! { span =>
        compile_error!(#msg)
    }
}

fn find_closest<'a>(target: &str, candidates: &'a [String]) -> Option<&'a str> {
    candidates
        .iter()
        .filter_map(|c| {
            let dist = strsim::levenshtein(target, c);
            if dist <= 3 {
                Some((c.as_str(), dist))
            } else {
                None
            }
        })
        .min_by_key(|(_, d)| *d)
        .map(|(s, _)| s)
}
