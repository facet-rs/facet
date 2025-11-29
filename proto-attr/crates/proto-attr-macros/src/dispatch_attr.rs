//! Implementation of `__dispatch_attr!` proc-macro.
//!
//! A unified dispatcher that handles all attribute parsing without
//! calling other generated macro_rules macros. This avoids the Rust
//! limitation on macro-expanded macro_export macros.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{quote, quote_spanned};
use unsynn::*;

// ============================================================================
// UNSYNN TYPE DEFINITIONS
// ============================================================================

keyword! {
    KNamespace = "namespace";
    KEnumName = "enum_name";
    KVariants = "variants";
    KName = "name";
    KRest = "rest";
    KUnit = "unit";
    KNewtype = "newtype";
    KRec = "rec";
}

operator! {
    At = "@";
    Col = ":";
}

unsynn! {
    /// The complete input to __dispatch_attr
    struct DispatchAttrInput {
        namespace_section: NamespaceSection,
        enum_name_section: EnumNameSection,
        variants_section: VariantsSection,
        name_section: NameSection,
        rest_section: RestSection,
    }

    /// @namespace { ... }
    struct NamespaceSection {
        _at: At,
        _kw: KNamespace,
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
        /// struct variant with fields
        Struct(StructVariantDef),
    }

    /// rec Column { name: opt_string, primary_key: bool }
    struct StructVariantDef {
        _rec: KRec,
        struct_name: Ident,
        fields: BraceGroupContaining<CommaDelimitedVec<FieldDef>>,
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

struct ParsedDispatchInput {
    namespace: TokenStream2,
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
    Struct {
        struct_name: Ident,
        fields: Vec<(Ident, FieldKind)>,
    },
}

#[derive(Clone, Copy)]
enum FieldKind {
    Bool,
    String,
    OptString,
    OptBool,
}

impl DispatchAttrInput {
    fn to_parsed(&self) -> std::result::Result<ParsedDispatchInput, String> {
        let namespace = self.namespace_section.content.0.stream();

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
            namespace,
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
            VariantKindDef::Struct(s) => {
                let fields: std::result::Result<Vec<_>, _> = s
                    .fields
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
                            _ => return Err(format!("unknown field kind: {}", kind_str)),
                        };
                        Ok((name, kind))
                    })
                    .collect();
                ParsedVariantKind::Struct {
                    struct_name: s.struct_name.clone(),
                    fields: fields?,
                }
            }
        };

        Ok(ParsedVariant {
            name: self.name.clone(),
            kind,
        })
    }
}

// ============================================================================
// ENTRY POINT
// ============================================================================

pub fn dispatch_attr(input: TokenStream) -> TokenStream {
    let input2 = TokenStream2::from(input);
    let mut iter = input2.to_token_iter();

    let parsed_input: DispatchAttrInput = match iter.parse() {
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

    let namespace = &input.namespace;
    let enum_name = &input.enum_name;
    let attr_name = &input.attr_name;
    let attr_name_str = attr_name.to_string();
    let attr_span = attr_name.span();
    let rest = &input.rest;

    // Find matching variant
    for variant in &input.variants {
        if attr_name_str == variant.name.to_string() {
            let variant_name = &variant.name;
            // Convert to PascalCase for enum variant
            let variant_pascal = to_pascal_case(&variant_name.to_string());
            let variant_ident = proc_macro2::Ident::new(&variant_pascal, variant_name.span());

            return match &variant.kind {
                ParsedVariantKind::Unit => generate_unit(
                    namespace,
                    enum_name,
                    &variant_ident,
                    attr_name,
                    rest,
                    attr_span,
                ),
                ParsedVariantKind::Newtype => generate_newtype(
                    namespace,
                    enum_name,
                    &variant_ident,
                    attr_name,
                    rest,
                    attr_span,
                ),
                ParsedVariantKind::Struct {
                    struct_name,
                    fields,
                } => generate_struct(
                    namespace,
                    enum_name,
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
        format!(
            "unknown attribute `{}`; did you mean `{}`?",
            attr_name_str, s
        )
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

    expanded.into()
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

fn generate_unit(
    namespace: &TokenStream2,
    enum_name: &Ident,
    variant_ident: &proc_macro2::Ident,
    attr_name: &Ident,
    rest: &TokenStream2,
    span: Span,
) -> TokenStream {
    let rest_tokens: Vec<TokenTree> = rest.clone().into_iter().collect();

    // Valid: no rest or empty parens
    if rest_tokens.is_empty() {
        return quote_spanned! { span =>
            #namespace::#enum_name::#variant_ident
        }
        .into();
    }

    // Check for empty parens ()
    if let Some(TokenTree::Group(g)) = rest_tokens.first() {
        if g.delimiter() == proc_macro2::Delimiter::Parenthesis && g.stream().is_empty() {
            return quote_spanned! { span =>
                #namespace::#enum_name::#variant_ident
            }
            .into();
        }
    }

    // Error: unit variant doesn't take arguments
    let msg = format!(
        "`{}` does not take arguments; use just `{}`",
        attr_name, attr_name
    );
    quote_spanned! { span =>
        compile_error!(#msg)
    }
    .into()
}

fn generate_newtype(
    namespace: &TokenStream2,
    enum_name: &Ident,
    variant_ident: &proc_macro2::Ident,
    attr_name: &Ident,
    rest: &TokenStream2,
    span: Span,
) -> TokenStream {
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
                            #namespace::#enum_name::#variant_ident(#lit)
                        }
                        .into();
                    }
                }
            }
            // Error: non-literal in parens
            let msg = format!(
                "`{}` expects a string literal: `{}(\"name\")`",
                attr_str, attr_str
            );
            return quote_spanned! { span =>
                compile_error!(#msg)
            }
            .into();
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
                            #namespace::#enum_name::#variant_ident(#lit)
                        }
                        .into();
                    }
                }
                // Error: non-literal after =
                let msg = format!(
                    "`{}` expects a string literal: `{} = \"name\"`",
                    attr_str, attr_str
                );
                return quote_spanned! { span =>
                    compile_error!(#msg)
                }
                .into();
            }
        }
    }

    // Error: no value provided
    let msg = format!(
        "`{}` requires a string value: `{}(\"name\")` or `{} = \"name\"`",
        attr_str, attr_str, attr_str
    );
    quote_spanned! { span =>
        compile_error!(#msg)
    }
    .into()
}

fn generate_struct(
    namespace: &TokenStream2,
    enum_name: &Ident,
    variant_ident: &proc_macro2::Ident,
    struct_name: &Ident,
    fields: &[(Ident, FieldKind)],
    attr_name: &Ident,
    rest: &TokenStream2,
    span: Span,
) -> TokenStream {
    let rest_tokens: Vec<TokenTree> = rest.clone().into_iter().collect();
    let attr_str = attr_name.to_string();

    // Generate field metadata for __build_struct_fields
    let fields_meta: Vec<TokenStream2> = fields
        .iter()
        .map(|(name, kind)| {
            let kind_str = match kind {
                FieldKind::Bool => quote! { bool },
                FieldKind::String => quote! { string },
                FieldKind::OptString => quote! { opt_string },
                FieldKind::OptBool => quote! { opt_bool },
            };
            quote! { #name: #kind_str }
        })
        .collect();

    // Generate default field values
    let default_fields: Vec<TokenStream2> = fields
        .iter()
        .map(|(name, kind)| {
            let default = match kind {
                FieldKind::Bool => quote! { false },
                FieldKind::String => quote! { "" },
                FieldKind::OptString => quote! { None },
                FieldKind::OptBool => quote! { None },
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
                    #namespace::#enum_name::#variant_ident(#namespace::#struct_name {
                        #(#default_fields),*
                    })
                }
                .into();
            }

            // Non-empty - delegate to __build_struct_fields proc-macro
            return quote_spanned! { span =>
                #namespace::__build_struct_fields!{
                    @krate { #namespace }
                    @enum_name { #enum_name }
                    @variant_name { #variant_ident }
                    @struct_name { #struct_name }
                    @fields { #(#fields_meta),* }
                    @input { #inner }
                }
            }
            .into();
        }
    }

    // No parens - use defaults
    if rest_tokens.is_empty() {
        return quote_spanned! { span =>
            #namespace::#enum_name::#variant_ident(#namespace::#struct_name {
                #(#default_fields),*
            })
        }
        .into();
    }

    // Error: invalid syntax
    let msg = format!(
        "`{}` expects parentheses: `{}(...)` or just `{}`",
        attr_str, attr_str, attr_str
    );
    quote_spanned! { span =>
        compile_error!(#msg)
    }
    .into()
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
