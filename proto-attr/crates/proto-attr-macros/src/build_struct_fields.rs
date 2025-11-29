//! Implementation of `__build_struct_fields!` proc-macro.
//!
//! This proc-macro handles all struct field parsing in one shot,
//! avoiding the need for recursive macro_rules calls.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::quote;
#[cfg(not(feature = "nightly"))]
use quote::quote_spanned;
use unsynn::*;

/// Error with span information for better diagnostics
struct SpannedError {
    message: String,
    span: Span,
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
    struct FieldsSection {
        _at: At,
        _kw: KFields,
        content: BraceGroupContaining<CommaDelimitedVec<FieldDef>>,
    }

    /// @input { ... }
    struct InputSection {
        _at: At,
        _kw: KInput,
        content: BraceGroup,
    }

    /// A field definition: `name: opt_string`
    struct FieldDef {
        name: Ident,
        _colon: Col,
        kind: Ident,
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
}

#[derive(Clone, Copy, PartialEq)]
enum FieldKind {
    Bool,
    String,
    OptString,
    OptBool,
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
    /// Flag (no value): `primary_key`
    Flag,
}

impl BuildStructFieldsInput {
    fn to_parsed(&self) -> std::result::Result<ParsedBuildInput, String> {
        let krate_path = self.krate_section.content.0.stream();
        let enum_name = self.enum_name_section.content.content.clone();
        let variant_name = self.variant_name_section.content.content.clone();
        let struct_name = self.struct_name_section.content.content.clone();

        let fields: std::result::Result<Vec<_>, _> = self
            .fields_section
            .content
            .content
            .iter()
            .map(|d| {
                let name = d.value.name.clone();
                let kind_str = d.value.kind.to_string();
                let kind = match kind_str.as_str() {
                    "bool" => FieldKind::Bool,
                    "string" => FieldKind::String,
                    "opt_string" => FieldKind::OptString,
                    "opt_bool" => FieldKind::OptBool,
                    _ => {
                        return Err(format!(
                            "expected `bool`, `string`, `opt_string`, or `opt_bool`, got `{}`",
                            kind_str
                        ));
                    }
                };
                Ok(ParsedFieldDef { name, kind })
            })
            .collect();

        let input = self.input_section.content.0.stream();

        Ok(ParsedBuildInput {
            krate_path,
            enum_name,
            variant_name,
            struct_name,
            fields: fields?,
            input,
        })
    }
}

// ============================================================================
// ENTRY POINT
// ============================================================================

pub fn build_struct_fields(input: TokenStream) -> TokenStream {
    let input2 = TokenStream2::from(input);
    let mut iter = input2.to_token_iter();

    let parsed_input: BuildStructFieldsInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote! { compile_error!(#msg); }.into();
        }
    };

    let input = match parsed_input.to_parsed() {
        Ok(i) => i,
        Err(e) => {
            return quote! { compile_error!(#e); }.into();
        }
    };

    match build_struct_fields_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => emit_error(err, &input),
    }
}

fn emit_error(err: SpannedError, input: &ParsedBuildInput) -> TokenStream {
    let message = err.message;
    let span = err.span;

    #[cfg(feature = "nightly")]
    {
        use proc_macro::{Diagnostic, Level};
        let diag = Diagnostic::spanned(span.unwrap(), Level::Error, &message);
        diag.emit();

        // Return a valid dummy expression with default field values
        // The error is emitted, compilation will fail, but this prevents cascading errors
        let krate_path = &input.krate_path;
        let enum_name = &input.enum_name;
        let variant_name = &input.variant_name;
        let struct_name = &input.struct_name;

        let field_defaults: Vec<TokenStream2> = input
            .fields
            .iter()
            .map(|f| {
                let name = &f.name;
                let default = match f.kind {
                    FieldKind::Bool => quote! { false },
                    FieldKind::String => quote! { "" },
                    FieldKind::OptString => quote! { None },
                    FieldKind::OptBool => quote! { None },
                };
                quote! { #name: #default }
            })
            .collect();

        quote! {
            #krate_path::#enum_name::#variant_name(#krate_path::#struct_name {
                #(#field_defaults),*
            })
        }
        .into()
    }

    #[cfg(not(feature = "nightly"))]
    {
        let _ = input; // unused on stable
        quote_spanned! { span => compile_error!(#message) }.into()
    }
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
                (None, FieldKind::String) => quote! { "" },
                (None, FieldKind::OptString) => quote! { None },
                (None, FieldKind::Bool) => quote! { false },
                (None, FieldKind::OptBool) => quote! { None },
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
        if let TokenTree::Punct(p) = &tokens[i] {
            if p.as_char() == ',' {
                i += 1;
                continue;
            }
        }

        // Expect identifier (field name)
        let field_name = match &tokens[i] {
            TokenTree::Ident(ident) => ident.clone(),
            other => {
                return Err(SpannedError {
                    message: format!("expected field name, found `{}`", other),
                    span: other.span(),
                });
            }
        };
        let field_name_str = field_name.to_string();
        let field_span = field_name.span();
        i += 1;

        // Find field definition
        let field_def = field_defs
            .iter()
            .find(|f| f.name.to_string() == field_name_str);
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
                            "`{}` requires a string value: `{} = \"value\"`",
                            field_name_str, field_name_str
                        ),
                        span: field_span,
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
                        message: format!("`{}` requires a value after `=`", field_name_str),
                        span: field_span,
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
                                        "`{}` expects a string literal: `{} = \"value\"`",
                                        field_name_str, field_name_str
                                    ),
                                    span: value_token.span(),
                                });
                            }
                        } else {
                            return Err(SpannedError {
                                message: format!(
                                    "`{}` expects a string literal: `{} = \"value\"`",
                                    field_name_str, field_name_str
                                ),
                                span: value_token.span(),
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
                                            "`{}` expects `true` or `false`: `{} = true`",
                                            field_name_str, field_name_str
                                        ),
                                        span: value_token.span(),
                                    });
                                }
                            }
                        } else {
                            return Err(SpannedError {
                                message: format!(
                                    "`{}` expects `true` or `false`: `{} = true`",
                                    field_name_str, field_name_str
                                ),
                                span: value_token.span(),
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
                                "`{}` requires a string value: `{} = \"value\"`",
                                field_name_str, field_name_str
                            ),
                            span: field_span,
                        });
                    }
                }
                i += 1;
            } else {
                return Err(SpannedError {
                    message: format!("expected `=` or `,` after field name `{}`", field_name_str),
                    span: p.span(),
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
                            "`{}` requires a string value: `{} = \"value\"`",
                            field_name_str, field_name_str
                        ),
                        span: field_span,
                    });
                }
            }
        }
    }

    Ok(parsed)
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
