//! Implementation of `__build_struct_fields!` proc-macro.
//!
//! This proc-macro handles all struct field parsing in one shot,
//! avoiding the need for recursive macro_rules calls.

use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use quote::quote_spanned;
use unsynn::*;

/// Error with span information for better diagnostics
struct SpannedError {
    message: String,
    span: Span,
    /// Optional help text (from doc comment)
    help: Option<String>,
}

// ============================================================================
// UNSYNN TYPE DEFINITIONS
// ============================================================================

keyword! {
    KKrate = "krate";
    KEnumName = "enum_name";
    KVariantName = "variant_name";
    KStructName = "struct_name";
    KFields = "fields";
    KInput = "input";
}

operator! {
    At = "@";
    Col = ":";
}

unsynn! {
    /// The complete input to __build_struct_fields
    struct BuildStructFieldsInput {
        krate_section: KrateSection,
        enum_name_section: EnumNameSection,
        variant_name_section: VariantNameSection,
        struct_name_section: StructNameSection,
        fields_section: FieldsSection,
        input_section: InputSection,
    }

    /// @krate { ... }
    struct KrateSection {
        _at: At,
        _kw: KKrate,
        content: BraceGroup,
    }

    /// @enum_name { ... }
    struct EnumNameSection {
        _at: At,
        _kw: KEnumName,
        content: BraceGroupContaining<Ident>,
    }

    /// @variant_name { ... }
    struct VariantNameSection {
        _at: At,
        _kw: KVariantName,
        content: BraceGroupContaining<Ident>,
    }

    /// @struct_name { ... }
    struct StructNameSection {
        _at: At,
        _kw: KStructName,
        content: BraceGroupContaining<Ident>,
    }

    /// @fields { name: opt_string, primary_key: bool }
    /// May include doc comments: #[doc = "..."] name: opt_string
    struct FieldsSection {
        _at: At,
        _kw: KFields,
        /// Raw tokens - parsed manually to extract doc comments
        content: BraceGroup,
    }

    /// @input { ... }
    struct InputSection {
        _at: At,
        _kw: KInput,
        content: BraceGroup,
    }
}

// ============================================================================
// PARSED STRUCTURES
// ============================================================================

struct ParsedBuildInput {
    krate_path: TokenStream2,
    enum_name: Ident,
    variant_name: Ident,
    struct_name: Ident,
    fields: Vec<ParsedFieldDef>,
    input: TokenStream2,
}

#[derive(Clone)]
struct ParsedFieldDef {
    name: Ident,
    kind: FieldKind,
    /// Doc comment for help text in errors
    doc: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
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

/// Parsed field value from input
struct ParsedField {
    name: String,
    #[allow(dead_code)]
    name_span: Span,
    value: FieldValue,
}

enum FieldValue {
    /// String literal: `name = "foo"`
    String(String),
    /// Bool literal: `primary_key = true`
    Bool(bool),
    /// Char literal: `short = 'v'`
    Char(char),
    /// Integer literal: `min = 0`
    I64(i64),
    /// List of strings: `columns = ["id", "name"]`
    ListString(Vec<String>),
    /// List of integers: `values = [1, 2, 3]`
    ListI64(Vec<i64>),
    /// Bare identifier: `method = post`
    Ident(String),
    /// Flag (no value): `primary_key`
    Flag,
}

impl BuildStructFieldsInput {
    fn to_parsed(&self) -> std::result::Result<ParsedBuildInput, String> {
        let krate_path = self.krate_section.content.0.stream();
        let enum_name = self.enum_name_section.content.content.clone();
        let variant_name = self.variant_name_section.content.content.clone();
        let struct_name = self.struct_name_section.content.content.clone();

        // Parse fields manually to extract doc comments
        let fields = parse_field_defs_with_docs(&self.fields_section.content.0.stream())?;

        let input = self.input_section.content.0.stream();

        Ok(ParsedBuildInput {
            krate_path,
            enum_name,
            variant_name,
            struct_name,
            fields,
            input,
        })
    }
}

/// Parse field definitions from token stream, extracting doc comments
fn parse_field_defs_with_docs(
    tokens: &TokenStream2,
) -> std::result::Result<Vec<ParsedFieldDef>, String> {
    let tokens: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut fields = Vec::new();
    let mut i = 0;
    let mut current_doc: Option<String> = None;

    while i < tokens.len() {
        // Skip commas
        if let TokenTree::Punct(p) = &tokens[i]
            && p.as_char() == ','
        {
            i += 1;
            continue;
        }

        // Check for doc comment: #[doc = "..."]
        if let TokenTree::Punct(p) = &tokens[i]
            && p.as_char() == '#'
            && i + 1 < tokens.len()
            && let TokenTree::Group(g) = &tokens[i + 1]
            && g.delimiter() == proc_macro2::Delimiter::Bracket
            && let Some(doc) = extract_doc_from_attr(&g.stream())
        {
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

fn extract_doc_from_attr(tokens: &TokenStream2) -> Option<String> {
    let tokens: Vec<TokenTree> = tokens.clone().into_iter().collect();

    // Expected: doc = "..."
    if tokens.len() >= 3
        && let TokenTree::Ident(ident) = &tokens[0]
        && *ident == "doc"
        && let TokenTree::Punct(p) = &tokens[1]
        && p.as_char() == '='
        && let TokenTree::Literal(lit) = &tokens[2]
    {
        let lit_str = lit.to_string();
        // Remove quotes and unescape
        if lit_str.starts_with('"') && lit_str.ends_with('"') {
            let inner = &lit_str[1..lit_str.len() - 1];
            return Some(unescape_string(inner.trim_start()));
        }
    }
    None
}

// ============================================================================
// ENTRY POINT
// ============================================================================

/// Parses struct field definitions and their values in one pass, generating field initialization code with comprehensive error messages.
pub fn build_struct_fields(input: TokenStream2) -> TokenStream2 {
    let mut iter = input.to_token_iter();

    let parsed_input: BuildStructFieldsInput = match iter.parse() {
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

    match build_struct_fields_impl(&input) {
        Ok(tokens) => tokens,
        Err(err) => emit_error(err),
    }
}

fn emit_error(err: SpannedError) -> TokenStream2 {
    let message = err.message;
    let span = err.span;
    let help = err.help;

    // Append help text to the message
    let full_message = if let Some(help_text) = help {
        format!("{message}\n  = help: {help_text}")
    } else {
        message
    };
    quote_spanned! { span => compile_error!(#full_message) }
}

fn build_struct_fields_impl(
    input: &ParsedBuildInput,
) -> std::result::Result<TokenStream2, SpannedError> {
    let krate_path = &input.krate_path;
    let enum_name = &input.enum_name;
    let variant_name = &input.variant_name;
    let struct_name = &input.struct_name;

    // Parse all field assignments from input tokens
    let parsed_fields = parse_input_fields(&input.input, &input.fields)?;

    // Build the struct fields with values
    let field_values: Vec<TokenStream2> = input
        .fields
        .iter()
        .map(|field_def| {
            let field_name = &field_def.name;
            let field_name_str = field_name.to_string();

            // Find if this field was set in input
            let parsed = parsed_fields.iter().find(|p| p.name == field_name_str);

            let value = match (parsed, field_def.kind) {
                (Some(p), FieldKind::String) => match &p.value {
                    FieldValue::String(s) => quote! { #s },
                    _ => quote! { "" }, // Will error elsewhere
                },
                (Some(p), FieldKind::OptString) => match &p.value {
                    FieldValue::String(s) => quote! { Some(#s) },
                    _ => quote! { None },
                },
                (Some(p), FieldKind::Bool) => match &p.value {
                    FieldValue::Bool(b) => quote! { #b },
                    FieldValue::Flag => quote! { true },
                    _ => quote! { false },
                },
                (Some(p), FieldKind::OptBool) => match &p.value {
                    FieldValue::Bool(b) => quote! { Some(#b) },
                    FieldValue::Flag => quote! { Some(true) },
                    _ => quote! { None },
                },
                (Some(p), FieldKind::OptChar) => match &p.value {
                    FieldValue::Char(c) => quote! { Some(#c) },
                    _ => quote! { None },
                },
                (Some(p), FieldKind::I64) => match &p.value {
                    FieldValue::I64(n) => quote! { #n },
                    _ => quote! { 0 }, // Will error elsewhere
                },
                (Some(p), FieldKind::OptI64) => match &p.value {
                    FieldValue::I64(n) => quote! { Some(#n) },
                    _ => quote! { None },
                },
                (Some(p), FieldKind::ListString) => match &p.value {
                    FieldValue::ListString(items) => quote! { &[#(#items),*] },
                    _ => quote! { &[] },
                },
                (Some(p), FieldKind::ListI64) => match &p.value {
                    FieldValue::ListI64(items) => quote! { &[#(#items),*] },
                    _ => quote! { &[] },
                },
                (Some(p), FieldKind::Ident) => match &p.value {
                    FieldValue::Ident(s) => quote! { #s },
                    _ => quote! { "" },
                },
                (None, FieldKind::String) => quote! { "" },
                (None, FieldKind::OptString) => quote! { None },
                (None, FieldKind::Bool) => quote! { false },
                (None, FieldKind::OptBool) => quote! { None },
                (None, FieldKind::OptChar) => quote! { None },
                (None, FieldKind::I64) => quote! { 0 },
                (None, FieldKind::OptI64) => quote! { None },
                (None, FieldKind::ListString) => quote! { &[] },
                (None, FieldKind::ListI64) => quote! { &[] },
                (None, FieldKind::Ident) => quote! { "" },
            };

            quote! { #field_name: #value }
        })
        .collect();

    Ok(quote! {
        #krate_path::#enum_name::#variant_name(#krate_path::#struct_name {
            #(#field_values),*
        })
    })
}

fn parse_input_fields(
    input: &TokenStream2,
    field_defs: &[ParsedFieldDef],
) -> std::result::Result<Vec<ParsedField>, SpannedError> {
    let tokens: Vec<TokenTree> = input.clone().into_iter().collect();
    let mut parsed = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        // Skip commas
        if let TokenTree::Punct(p) = &tokens[i]
            && p.as_char() == ','
        {
            i += 1;
            continue;
        }

        // Expect identifier (field name)
        let field_name = match &tokens[i] {
            TokenTree::Ident(ident) => ident.clone(),
            other => {
                return Err(SpannedError {
                    message: format!("expected field name, found `{other}`"),
                    span: other.span(),
                    help: None,
                });
            }
        };
        let field_name_str = field_name.to_string();
        let field_span = field_name.span();
        i += 1;

        // Check for duplicate field
        if parsed
            .iter()
            .any(|p: &ParsedField| p.name == field_name_str)
        {
            return Err(SpannedError {
                message: format!(
                    "duplicate field `{field_name_str}`; each field can only be specified once"
                ),
                span: field_span,
                help: None,
            });
        }

        // Find field definition
        let field_def = field_defs.iter().find(|f| f.name == field_name_str);
        if field_def.is_none() {
            // Unknown field - generate helpful error
            let known_names: Vec<_> = field_defs.iter().map(|f| f.name.to_string()).collect();
            let suggestion = find_closest(&field_name_str, &known_names);
            let msg = if let Some(s) = suggestion {
                format!(
                    "unknown field `{}`; did you mean `{}`? Known fields: {}",
                    field_name_str,
                    s,
                    known_names.join(", ")
                )
            } else {
                format!(
                    "unknown field `{}`; known fields: {}",
                    field_name_str,
                    known_names.join(", ")
                )
            };
            return Err(SpannedError {
                message: msg,
                span: field_span,
                help: None,
            });
        }
        let field_def = field_def.unwrap();

        // Check what follows: `=` or nothing (flag) or `,` (flag)
        if i >= tokens.len() {
            // End of input - this is a flag
            match field_def.kind {
                FieldKind::Bool | FieldKind::OptBool => {
                    parsed.push(ParsedField {
                        name: field_name_str,
                        name_span: field_span,
                        value: FieldValue::Flag,
                    });
                }
                FieldKind::String | FieldKind::OptString => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires a string value: `{field_name_str} = \"value\"`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::OptChar => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires a char value: `{field_name_str} = 'v'`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::I64 | FieldKind::OptI64 => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires an integer value: `{field_name_str} = 42`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::ListString => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires a list value: `{field_name_str} = [\"a\", \"b\"]`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::ListI64 => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires a list value: `{field_name_str} = [1, 2, 3]`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::Ident => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires an identifier value: `{field_name_str} = some_value`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
            }
            continue;
        }

        // Check for `=`
        if let TokenTree::Punct(p) = &tokens[i] {
            if p.as_char() == '=' {
                i += 1;
                // Parse value
                if i >= tokens.len() {
                    return Err(SpannedError {
                        message: format!("`{field_name_str}` requires a value after `=`"),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }

                let value_token = &tokens[i];
                i += 1;

                match field_def.kind {
                    FieldKind::String | FieldKind::OptString => {
                        // Expect string literal
                        if let TokenTree::Literal(lit) = value_token {
                            let lit_str = lit.to_string();
                            // Remove quotes
                            if lit_str.starts_with('\"') && lit_str.ends_with('\"') {
                                let inner = lit_str[1..lit_str.len() - 1].to_string();
                                parsed.push(ParsedField {
                                    name: field_name_str,
                                    name_span: field_span,
                                    value: FieldValue::String(inner),
                                });
                            } else {
                                return Err(SpannedError {
                                    message: format!(
                                        "`{field_name_str}` expects a string literal: `{field_name_str} = \"value\"`"
                                    ),
                                    span: value_token.span(),
                                    help: field_def.doc.clone(),
                                });
                            }
                        } else if let TokenTree::Ident(ident) = value_token {
                            // Common mistake: using an identifier instead of a string
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects a string literal, not an identifier; \
                                     try `{field_name_str} = \"{ident}\"` (with quotes)"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        } else {
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects a string literal: `{field_name_str} = \"value\"`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        }
                    }
                    FieldKind::Bool | FieldKind::OptBool => {
                        // Expect true/false
                        if let TokenTree::Ident(ident) = value_token {
                            let ident_str = ident.to_string();
                            match ident_str.as_str() {
                                "true" => {
                                    parsed.push(ParsedField {
                                        name: field_name_str,
                                        name_span: field_span,
                                        value: FieldValue::Bool(true),
                                    });
                                }
                                "false" => {
                                    parsed.push(ParsedField {
                                        name: field_name_str,
                                        name_span: field_span,
                                        value: FieldValue::Bool(false),
                                    });
                                }
                                _ => {
                                    return Err(SpannedError {
                                        message: format!(
                                            "`{field_name_str}` expects `true` or `false`: `{field_name_str} = true`"
                                        ),
                                        span: value_token.span(),
                                        help: field_def.doc.clone(),
                                    });
                                }
                            }
                        } else if let TokenTree::Literal(lit) = value_token {
                            // Common mistake: using a string instead of bool
                            let lit_str = lit.to_string();
                            if lit_str.starts_with('"') && lit_str.ends_with('"') {
                                let inner = &lit_str[1..lit_str.len() - 1];
                                // Check if the string content looks like a bool
                                let suggestion = match inner {
                                    "true" | "yes" | "1" | "on" => "true",
                                    "false" | "no" | "0" | "off" => "false",
                                    _ => "true",
                                };
                                return Err(SpannedError {
                                    message: format!(
                                        "`{field_name_str}` expects `true` or `false`, not a string; \
                                         try `{field_name_str} = {suggestion}` (without quotes)"
                                    ),
                                    span: value_token.span(),
                                    help: field_def.doc.clone(),
                                });
                            }
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects `true` or `false`: `{field_name_str} = true`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        } else {
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects `true` or `false`: `{field_name_str} = true`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        }
                    }
                    FieldKind::OptChar => {
                        // Expect char literal: 'v'
                        if let TokenTree::Literal(lit) = value_token {
                            let lit_str = lit.to_string();
                            // Check for char literal format: 'x'
                            if lit_str.starts_with('\'')
                                && lit_str.ends_with('\'')
                                && lit_str.len() >= 3
                            {
                                // Parse the char (handling escape sequences)
                                let inner = &lit_str[1..lit_str.len() - 1];
                                let c = if inner.starts_with('\\') {
                                    // Handle escape sequences
                                    match inner.chars().nth(1) {
                                        Some('n') => '\n',
                                        Some('r') => '\r',
                                        Some('t') => '\t',
                                        Some('\\') => '\\',
                                        Some('\'') => '\'',
                                        Some('0') => '\0',
                                        Some(c) => c,
                                        None => {
                                            return Err(SpannedError {
                                                message: format!(
                                                    "`{field_name_str}` has invalid escape sequence in char literal"
                                                ),
                                                span: value_token.span(),
                                                help: field_def.doc.clone(),
                                            });
                                        }
                                    }
                                } else {
                                    inner.chars().next().unwrap_or(' ')
                                };
                                parsed.push(ParsedField {
                                    name: field_name_str,
                                    name_span: field_span,
                                    value: FieldValue::Char(c),
                                });
                            } else {
                                return Err(SpannedError {
                                    message: format!(
                                        "`{field_name_str}` expects a char literal: `{field_name_str} = 'v'`"
                                    ),
                                    span: value_token.span(),
                                    help: field_def.doc.clone(),
                                });
                            }
                        } else if let TokenTree::Ident(ident) = value_token {
                            // Common mistake: using an identifier instead of char
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects a char literal, not an identifier; \
                                     try `{field_name_str} = '{ident}'` (with single quotes)"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        } else {
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects a char literal: `{field_name_str} = 'v'`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        }
                    }
                    FieldKind::I64 | FieldKind::OptI64 => {
                        // Expect integer literal (possibly negative)
                        let (value, advance) =
                            parse_integer_value(&tokens[i - 1..], value_token, &field_name_str)?;
                        parsed.push(ParsedField {
                            name: field_name_str,
                            name_span: field_span,
                            value: FieldValue::I64(value),
                        });
                        // Advance past any additional tokens consumed (for negative numbers)
                        i += advance;
                    }
                    FieldKind::ListString => {
                        // Expect bracket group with string literals: ["a", "b"]
                        if let TokenTree::Group(g) = value_token {
                            if g.delimiter() == proc_macro2::Delimiter::Bracket {
                                let items = parse_string_list(&g.stream())?;
                                parsed.push(ParsedField {
                                    name: field_name_str,
                                    name_span: field_span,
                                    value: FieldValue::ListString(items),
                                });
                            } else {
                                // Common mistake: wrong bracket type
                                let bracket_name = match g.delimiter() {
                                    proc_macro2::Delimiter::Brace => "curly braces `{}`",
                                    proc_macro2::Delimiter::Parenthesis => "parentheses `()`",
                                    _ => "wrong delimiters",
                                };
                                return Err(SpannedError {
                                    message: format!(
                                        "`{field_name_str}` expects square brackets `[]`, not {bracket_name}; \
                                         try `{field_name_str} = [\"a\", \"b\"]`"
                                    ),
                                    span: value_token.span(),
                                    help: field_def.doc.clone(),
                                });
                            }
                        } else if let TokenTree::Literal(lit) = value_token {
                            // Common mistake: single string instead of list
                            let lit_str = lit.to_string();
                            if lit_str.starts_with('"') && lit_str.ends_with('"') {
                                let inner = &lit_str[1..lit_str.len() - 1];
                                return Err(SpannedError {
                                    message: format!(
                                        "`{field_name_str}` expects a list, not a single string; \
                                         try `{field_name_str} = [\"{inner}\"]`"
                                    ),
                                    span: value_token.span(),
                                    help: field_def.doc.clone(),
                                });
                            }
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects a list: `{field_name_str} = [\"a\", \"b\"]`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        } else {
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects a list: `{field_name_str} = [\"a\", \"b\"]`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        }
                    }
                    FieldKind::ListI64 => {
                        // Expect bracket group with integer literals: [1, 2, 3]
                        if let TokenTree::Group(g) = value_token {
                            if g.delimiter() == proc_macro2::Delimiter::Bracket {
                                let items = parse_i64_list(&g.stream())?;
                                parsed.push(ParsedField {
                                    name: field_name_str,
                                    name_span: field_span,
                                    value: FieldValue::ListI64(items),
                                });
                            } else {
                                // Common mistake: wrong bracket type
                                let bracket_name = match g.delimiter() {
                                    proc_macro2::Delimiter::Brace => "curly braces `{}`",
                                    proc_macro2::Delimiter::Parenthesis => "parentheses `()`",
                                    _ => "wrong delimiters",
                                };
                                return Err(SpannedError {
                                    message: format!(
                                        "`{field_name_str}` expects square brackets `[]`, not {bracket_name}; \
                                         try `{field_name_str} = [1, 2, 3]`"
                                    ),
                                    span: value_token.span(),
                                    help: field_def.doc.clone(),
                                });
                            }
                        } else if let TokenTree::Literal(lit) = value_token {
                            // Common mistake: single number instead of list
                            let lit_str = lit.to_string();
                            if lit_str.chars().all(|c| c.is_ascii_digit() || c == '-') {
                                return Err(SpannedError {
                                    message: format!(
                                        "`{field_name_str}` expects a list, not a single value; \
                                         try `{field_name_str} = [{lit_str}]`"
                                    ),
                                    span: value_token.span(),
                                    help: field_def.doc.clone(),
                                });
                            }
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects a list: `{field_name_str} = [1, 2, 3]`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        } else {
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects a list: `{field_name_str} = [1, 2, 3]`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        }
                    }
                    FieldKind::Ident => {
                        // Expect bare identifier: method = post
                        if let TokenTree::Ident(ident) = value_token {
                            parsed.push(ParsedField {
                                name: field_name_str,
                                name_span: field_span,
                                value: FieldValue::Ident(ident.to_string()),
                            });
                        } else if let TokenTree::Literal(lit) = value_token {
                            // Common mistake: using a string instead of identifier
                            let lit_str = lit.to_string();
                            if lit_str.starts_with('"') && lit_str.ends_with('"') {
                                let inner = &lit_str[1..lit_str.len() - 1];
                                return Err(SpannedError {
                                    message: format!(
                                        "`{field_name_str}` expects a bare identifier, not a string; \
                                         try `{field_name_str} = {inner}` (without quotes)"
                                    ),
                                    span: value_token.span(),
                                    help: field_def.doc.clone(),
                                });
                            }
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects an identifier: `{field_name_str} = some_value`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        } else {
                            return Err(SpannedError {
                                message: format!(
                                    "`{field_name_str}` expects an identifier: `{field_name_str} = some_value`"
                                ),
                                span: value_token.span(),
                                help: field_def.doc.clone(),
                            });
                        }
                    }
                }
            } else if p.as_char() == ',' {
                // Flag followed by comma
                match field_def.kind {
                    FieldKind::Bool | FieldKind::OptBool => {
                        parsed.push(ParsedField {
                            name: field_name_str,
                            name_span: field_span,
                            value: FieldValue::Flag,
                        });
                    }
                    FieldKind::String | FieldKind::OptString => {
                        return Err(SpannedError {
                            message: format!(
                                "`{field_name_str}` requires a string value: `{field_name_str} = \"value\"`"
                            ),
                            span: field_span,
                            help: field_def.doc.clone(),
                        });
                    }
                    FieldKind::OptChar => {
                        return Err(SpannedError {
                            message: format!(
                                "`{field_name_str}` requires a char value: `{field_name_str} = 'v'`"
                            ),
                            span: field_span,
                            help: field_def.doc.clone(),
                        });
                    }
                    FieldKind::I64 | FieldKind::OptI64 => {
                        return Err(SpannedError {
                            message: format!(
                                "`{field_name_str}` requires an integer value: `{field_name_str} = 42`"
                            ),
                            span: field_span,
                            help: field_def.doc.clone(),
                        });
                    }
                    FieldKind::ListString => {
                        return Err(SpannedError {
                            message: format!(
                                "`{field_name_str}` requires a list value: `{field_name_str} = [\"a\", \"b\"]`"
                            ),
                            span: field_span,
                            help: field_def.doc.clone(),
                        });
                    }
                    FieldKind::ListI64 => {
                        return Err(SpannedError {
                            message: format!(
                                "`{field_name_str}` requires a list value: `{field_name_str} = [1, 2, 3]`"
                            ),
                            span: field_span,
                            help: field_def.doc.clone(),
                        });
                    }
                    FieldKind::Ident => {
                        return Err(SpannedError {
                            message: format!(
                                "`{field_name_str}` requires an identifier value: `{field_name_str} = some_value`"
                            ),
                            span: field_span,
                            help: field_def.doc.clone(),
                        });
                    }
                }
                i += 1;
            } else {
                return Err(SpannedError {
                    message: format!("expected `=` or `,` after field name `{field_name_str}`"),
                    span: p.span(),
                    help: None,
                });
            }
        } else {
            // No `=` and not end - check if it's another identifier (next field)
            // This means current field is a flag
            match field_def.kind {
                FieldKind::Bool | FieldKind::OptBool => {
                    parsed.push(ParsedField {
                        name: field_name_str,
                        name_span: field_span,
                        value: FieldValue::Flag,
                    });
                }
                FieldKind::String | FieldKind::OptString => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires a string value: `{field_name_str} = \"value\"`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::OptChar => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires a char value: `{field_name_str} = 'v'`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::I64 | FieldKind::OptI64 => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires an integer value: `{field_name_str} = 42`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::ListString => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires a list value: `{field_name_str} = [\"a\", \"b\"]`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::ListI64 => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires a list value: `{field_name_str} = [1, 2, 3]`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
                FieldKind::Ident => {
                    return Err(SpannedError {
                        message: format!(
                            "`{field_name_str}` requires an identifier value: `{field_name_str} = some_value`"
                        ),
                        span: field_span,
                        help: field_def.doc.clone(),
                    });
                }
            }
        }
    }

    Ok(parsed)
}

/// Parse an integer value, handling optional negative sign
fn parse_integer_value(
    tokens: &[TokenTree],
    value_token: &TokenTree,
    field_name: &str,
) -> std::result::Result<(i64, usize), SpannedError> {
    // Check if the value token is a negative sign
    if let TokenTree::Punct(p) = value_token
        && p.as_char() == '-'
    {
        // Next token should be the number
        if tokens.len() > 1
            && let TokenTree::Literal(lit) = &tokens[1]
        {
            let lit_str = lit.to_string();
            if let Ok(n) = lit_str.parse::<i64>() {
                return Ok((-n, 1)); // Consumed one extra token
            }
        }
        return Err(SpannedError {
            message: "expected integer literal after `-`".to_string(),
            span: p.span(),
            help: None,
        });
    }

    // Regular positive integer
    if let TokenTree::Literal(lit) = value_token {
        let lit_str = lit.to_string();
        // Try to parse as integer
        if let Ok(n) = lit_str.parse::<i64>() {
            return Ok((n, 0));
        }
        // Also try parsing with suffix (like 0i64)
        let cleaned = lit_str.trim_end_matches(|c: char| c.is_alphabetic() || c == '_');
        if let Ok(n) = cleaned.parse::<i64>() {
            return Ok((n, 0));
        }
        // Check if it looks like a number that overflowed
        if cleaned.chars().all(|c| c.is_ascii_digit()) {
            return Err(SpannedError {
                message: format!(
                    "`{}` value `{}` is too large; this field accepts i64 (range {} to {})",
                    field_name,
                    cleaned,
                    i64::MIN,
                    i64::MAX
                ),
                span: lit.span(),
                help: None,
            });
        }
        return Err(SpannedError {
            message: format!("`{field_name}` expected integer literal, got `{lit_str}`"),
            span: lit.span(),
            help: None,
        });
    }

    Err(SpannedError {
        message: format!("`{field_name}` expected integer literal, got `{value_token}`"),
        span: value_token.span(),
        help: None,
    })
}

/// Parse a list of string literals from bracket contents: "a", "b", "c"
fn parse_string_list(stream: &TokenStream2) -> std::result::Result<Vec<String>, SpannedError> {
    let tokens: Vec<TokenTree> = stream.clone().into_iter().collect();
    let mut items = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        // Skip commas
        if let TokenTree::Punct(p) = &tokens[i]
            && p.as_char() == ','
        {
            i += 1;
            continue;
        }

        // Expect string literal
        if let TokenTree::Literal(lit) = &tokens[i] {
            let lit_str = lit.to_string();
            if lit_str.starts_with('\"') && lit_str.ends_with('\"') {
                let inner = lit_str[1..lit_str.len() - 1].to_string();
                items.push(inner);
                i += 1;
            } else {
                return Err(SpannedError {
                    message: format!("expected string literal in list, got `{lit_str}`"),
                    span: lit.span(),
                    help: None,
                });
            }
        } else {
            return Err(SpannedError {
                message: format!("expected string literal in list, got `{}`", tokens[i]),
                span: tokens[i].span(),
                help: None,
            });
        }
    }

    Ok(items)
}

/// Parse a list of integer literals from bracket contents: 1, 2, 3
fn parse_i64_list(stream: &TokenStream2) -> std::result::Result<Vec<i64>, SpannedError> {
    let tokens: Vec<TokenTree> = stream.clone().into_iter().collect();
    let mut items = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        // Skip commas
        if let TokenTree::Punct(p) = &tokens[i] {
            if p.as_char() == ',' {
                i += 1;
                continue;
            }
            // Handle negative numbers
            if p.as_char() == '-' {
                if i + 1 < tokens.len()
                    && let TokenTree::Literal(lit) = &tokens[i + 1]
                {
                    let lit_str = lit.to_string();
                    if let Ok(n) = lit_str.parse::<i64>() {
                        items.push(-n);
                        i += 2;
                        continue;
                    }
                }
                return Err(SpannedError {
                    message: "expected integer after `-`".to_string(),
                    span: p.span(),
                    help: None,
                });
            }
        }

        // Expect integer literal
        if let TokenTree::Literal(lit) = &tokens[i] {
            let lit_str = lit.to_string();
            if let Ok(n) = lit_str.parse::<i64>() {
                items.push(n);
                i += 1;
            } else {
                // Try stripping suffix
                let cleaned = lit_str.trim_end_matches(|c: char| c.is_alphabetic() || c == '_');
                if let Ok(n) = cleaned.parse::<i64>() {
                    items.push(n);
                    i += 1;
                } else {
                    return Err(SpannedError {
                        message: format!("expected integer literal in list, got `{lit_str}`"),
                        span: lit.span(),
                        help: None,
                    });
                }
            }
        } else {
            return Err(SpannedError {
                message: format!("expected integer literal in list, got `{}`", tokens[i]),
                span: tokens[i].span(),
                help: None,
            });
        }
    }

    Ok(items)
}

#[cfg(feature = "helpful-derive")]
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

#[cfg(not(feature = "helpful-derive"))]
fn find_closest<'a>(_target: &str, _candidates: &'a [String]) -> Option<&'a str> {
    None
}
