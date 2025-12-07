//! Conversion from grammar types to parsed type representations
//!
//! This module converts the unsynn-parsed grammar types (Struct, Enum, etc.)
//! into the portable P-type representations (PStruct, PEnum, etc.) from
//! `facet-macro-types`.

use crate::grammar::*;
use facet_macro_types::*;
use proc_macro2::TokenStream;
use unsynn::{Comma, IParse, ToTokenIter, ToTokens};

// ============================================================================
// PUBLIC PARSING API
// ============================================================================

/// Parse a struct from a token stream
pub fn parse_struct(tokens: TokenStream) -> Result<PStruct, String> {
    let mut iter = tokens.to_token_iter();
    let parsed: Struct = iter
        .parse()
        .map_err(|e| format!("failed to parse struct: {e:?}"))?;
    Ok(struct_from_grammar(&parsed))
}

/// Parse an enum from a token stream
pub fn parse_enum(tokens: TokenStream) -> Result<PEnum, String> {
    let mut iter = tokens.to_token_iter();
    let parsed: Enum = iter
        .parse()
        .map_err(|e| format!("failed to parse enum: {e:?}"))?;
    Ok(enum_from_grammar(&parsed))
}

/// Parse either a struct or enum from a token stream
pub fn parse_type(tokens: TokenStream) -> Result<PType, String> {
    let mut iter = tokens.to_token_iter();

    // Try to parse as AdtDecl (struct or enum)
    let parsed: AdtDecl = iter
        .parse()
        .map_err(|e| format!("failed to parse type: {e:?}"))?;

    match parsed {
        AdtDecl::Struct(s) => Ok(PType::Struct(struct_from_grammar(&s))),
        AdtDecl::Enum(e) => Ok(PType::Enum(enum_from_grammar(&e))),
    }
}

// ============================================================================
// CONVERSION FUNCTIONS
// ============================================================================

/// Convert from grammar Struct to PStruct
fn struct_from_grammar(s: &Struct) -> PStruct {
    let mut display_name = s.name.to_string();

    // Parse attributes
    let attrs = attrs_from_grammar(&s.attributes, &mut display_name);

    // Extract rename_all rule
    let rename_all_rule = attrs.rename_all;

    // Parse fields
    let kind = struct_kind_from_grammar(&s.kind, rename_all_rule);

    // Parse generics
    let generics = s.generics.as_ref().map(generics_from_grammar);

    PStruct {
        name: s.name.clone(),
        attrs,
        kind,
        generics,
    }
}

/// Convert from grammar Enum to PEnum
fn enum_from_grammar(e: &Enum) -> PEnum {
    let mut display_name = e.name.to_string();

    // Parse attributes (including repr)
    let attrs = attrs_from_grammar(&e.attributes, &mut display_name);

    // Get container-level rename_all rule
    let container_rename_all = attrs.rename_all;

    // Parse variants
    let variants: Vec<PVariant> = e
        .body
        .content
        .iter()
        .map(|d| variant_from_grammar(&d.value, container_rename_all))
        .collect();

    // Parse generics
    let generics = e.generics.as_ref().map(generics_from_grammar);

    PEnum {
        name: e.name.clone(),
        attrs,
        variants,
        generics,
    }
}

/// Convert from grammar StructKind to PStructKind
fn struct_kind_from_grammar(kind: &StructKind, rename_all: Option<RenameRule>) -> PStructKind {
    match kind {
        StructKind::Struct { fields, .. } => {
            let parsed_fields: Vec<PStructField> = fields
                .content
                .iter()
                .map(|d| field_from_struct_field(&d.value, rename_all))
                .collect();
            PStructKind::Struct {
                fields: parsed_fields,
            }
        }
        StructKind::TupleStruct { fields, .. } => {
            let parsed_fields: Vec<PStructField> = fields
                .content
                .iter()
                .enumerate()
                .map(|(idx, d)| {
                    field_from_tuple_field(&d.value.attributes, idx, &d.value.typ, rename_all)
                })
                .collect();
            PStructKind::Tuple {
                fields: parsed_fields,
            }
        }
        StructKind::UnitStruct { .. } => PStructKind::Unit,
    }
}

/// Convert from grammar EnumVariantLike to PVariant
fn variant_from_grammar(
    var: &EnumVariantLike,
    container_rename_all: Option<RenameRule>,
) -> PVariant {
    let (raw_name, attributes) = match &var.variant {
        EnumVariantData::Unit(v) => (&v.name, &v.attributes),
        EnumVariantData::Tuple(v) => (&v.name, &v.attributes),
        EnumVariantData::Struct(v) => (&v.name, &v.attributes),
    };

    let initial_name = raw_name.to_string();
    let mut display_name = initial_name.clone();

    // Parse attributes
    let attrs = attrs_from_grammar(attributes, &mut display_name);

    // Determine effective name
    let name = if display_name != initial_name {
        // Field-level rename takes precedence
        PName {
            raw: IdentOrLiteral::Ident(raw_name.clone()),
            effective: display_name,
        }
    } else {
        // Use container rename_all rule
        PName::new(
            container_rename_all,
            IdentOrLiteral::Ident(raw_name.clone()),
        )
    };

    // Get variant's own rename_all for its fields
    let variant_rename_all = attrs.rename_all;

    // Parse variant kind
    let kind = match &var.variant {
        EnumVariantData::Unit(_) => PVariantKind::Unit,
        EnumVariantData::Tuple(v) => {
            let fields: Vec<PStructField> = v
                .fields
                .content
                .iter()
                .enumerate()
                .map(|(idx, d)| {
                    field_from_tuple_field(
                        &d.value.attributes,
                        idx,
                        &d.value.typ,
                        variant_rename_all,
                    )
                })
                .collect();
            PVariantKind::Tuple { fields }
        }
        EnumVariantData::Struct(v) => {
            let fields: Vec<PStructField> = v
                .fields
                .content
                .iter()
                .map(|d| field_from_struct_field(&d.value, variant_rename_all))
                .collect();
            PVariantKind::Struct { fields }
        }
    };

    // Parse discriminant
    let discriminant = var
        .discriminant
        .as_ref()
        .map(|d| d.second.to_token_stream());

    PVariant {
        name,
        attrs,
        kind,
        discriminant,
    }
}

/// Convert from grammar StructField (named field)
fn field_from_struct_field(f: &StructField, rename_all: Option<RenameRule>) -> PStructField {
    let initial_name = f.name.to_string();
    let mut display_name = initial_name.clone();

    let attrs = attrs_from_grammar(&f.attributes, &mut display_name);

    let name = if display_name != initial_name {
        PName {
            raw: IdentOrLiteral::Ident(f.name.clone()),
            effective: display_name,
        }
    } else {
        PName::new(rename_all, IdentOrLiteral::Ident(f.name.clone()))
    };

    PStructField {
        name,
        ty: f.typ.to_token_stream(),
        attrs,
    }
}

/// Convert from grammar TupleField (unnamed/indexed field)
fn field_from_tuple_field(
    attributes: &[Attribute],
    idx: usize,
    typ: &VerbatimUntil<Comma>,
    rename_all: Option<RenameRule>,
) -> PStructField {
    let initial_name = idx.to_string();
    let mut display_name = initial_name.clone();

    let attrs = attrs_from_grammar(attributes, &mut display_name);

    let name = if display_name != initial_name {
        PName {
            raw: IdentOrLiteral::Literal(idx),
            effective: display_name,
        }
    } else {
        PName::new(rename_all, IdentOrLiteral::Literal(idx))
    };

    PStructField {
        name,
        ty: typ.to_token_stream(),
        attrs,
    }
}

/// Convert from grammar Attribute list
fn attrs_from_grammar(attrs: &[Attribute], display_name: &mut String) -> PAttrs {
    let mut doc_lines: Vec<String> = Vec::new();
    let mut facet_attrs: Vec<PFacetAttr> = Vec::new();
    let mut repr: Option<PRepr> = None;

    for attr in attrs {
        match &attr.body.content {
            AttributeInner::Doc(doc) => {
                // Unescape the doc string
                let text = unescape_doc(doc.value.as_str());
                doc_lines.push(text);
            }
            AttributeInner::Repr(repr_inner) => {
                if let Some(parsed) = repr_from_grammar(repr_inner) {
                    repr = Some(parsed);
                }
            }
            AttributeInner::Facet(facet) => {
                for inner in facet.inner.content.iter() {
                    if let Some(pattr) = facet_attr_from_grammar(&inner.value) {
                        facet_attrs.push(pattr);
                    }
                }
            }
            AttributeInner::Any(_) => {
                // Ignore other attributes
            }
        }
    }

    // Extract rename_all and rename from facet attrs
    let mut rename_all: Option<RenameRule> = None;

    for attr in &facet_attrs {
        if attr.is_builtin() {
            match attr.key.as_str() {
                "rename" => {
                    let s = attr.args_string();
                    *display_name = s;
                }
                "rename_all" => {
                    let s = attr.args_string();
                    if let Some(rule) = RenameRule::parse(&s) {
                        rename_all = Some(rule);
                    }
                }
                _ => {}
            }
        }
    }

    PAttrs {
        doc: doc_lines,
        facet: facet_attrs,
        repr: repr.unwrap_or(PRepr::Rust(None)),
        rename_all,
    }
}

/// Convert from grammar FacetInner
fn facet_attr_from_grammar(inner: &FacetInner) -> Option<PFacetAttr> {
    match inner {
        FacetInner::Namespaced(ns) => {
            let args = match &ns.args {
                Some(AttrArgs::Parens(p)) => p.content.to_token_stream(),
                Some(AttrArgs::Equals(e)) => e.value.to_token_stream(),
                None => TokenStream::new(),
            };
            Some(PFacetAttr {
                ns: Some(ns.ns.to_string()),
                key: ns.key.to_string(),
                args,
            })
        }
        FacetInner::Simple(simple) => {
            let args = match &simple.args {
                Some(AttrArgs::Parens(p)) => p.content.to_token_stream(),
                Some(AttrArgs::Equals(e)) => e.value.to_token_stream(),
                None => TokenStream::new(),
            };
            Some(PFacetAttr {
                ns: None,
                key: simple.key.to_string(),
                args,
            })
        }
    }
}

/// Convert from grammar ReprInner
fn repr_from_grammar(repr: &ReprInner) -> Option<PRepr> {
    let items = &repr.attr.content;

    let mut is_c = false;
    let mut is_transparent = false;
    let mut primitive: Option<PrimitiveRepr> = None;

    for item in items.iter() {
        let s = item.value.to_string();
        match s.as_str() {
            "C" | "c" => is_c = true,
            "transparent" => is_transparent = true,
            "u8" => primitive = Some(PrimitiveRepr::U8),
            "u16" => primitive = Some(PrimitiveRepr::U16),
            "u32" => primitive = Some(PrimitiveRepr::U32),
            "u64" => primitive = Some(PrimitiveRepr::U64),
            "u128" => primitive = Some(PrimitiveRepr::U128),
            "i8" => primitive = Some(PrimitiveRepr::I8),
            "i16" => primitive = Some(PrimitiveRepr::I16),
            "i32" => primitive = Some(PrimitiveRepr::I32),
            "i64" => primitive = Some(PrimitiveRepr::I64),
            "i128" => primitive = Some(PrimitiveRepr::I128),
            "usize" => primitive = Some(PrimitiveRepr::Usize),
            "isize" => primitive = Some(PrimitiveRepr::Isize),
            _ => {}
        }
    }

    if is_transparent {
        Some(PRepr::Transparent)
    } else if is_c {
        Some(PRepr::C(primitive))
    } else if primitive.is_some() {
        Some(PRepr::Rust(primitive))
    } else {
        None
    }
}

/// Convert from grammar GenericParams
fn generics_from_grammar(params: &GenericParams) -> BoundedGenericParams {
    let mut lifetimes = Vec::new();
    let mut type_params = Vec::new();
    let mut const_params = Vec::new();

    for param in params.params.iter() {
        match &param.value {
            GenericParam::Lifetime { name, .. } => {
                lifetimes.push(name.name.to_string());
            }
            GenericParam::Type { name, bounds, .. } => {
                let bounds_ts = bounds
                    .as_ref()
                    .map(|b| b.second.to_token_stream())
                    .unwrap_or_default();
                type_params.push((name.clone(), bounds_ts));
            }
            GenericParam::Const { name, typ, .. } => {
                const_params.push((name.clone(), typ.to_token_stream()));
            }
        }
    }

    BoundedGenericParams {
        lifetimes,
        type_params,
        const_params,
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Unescape a doc string (remove surrounding quotes, handle escape sequences)
fn unescape_doc(s: &str) -> String {
    let s = s.trim();
    let s = s.trim_start_matches('"').trim_end_matches('"');

    // Handle basic escape sequences
    s.replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_parse_simple_struct() {
        let tokens = quote! {
            struct Foo {
                bar: u32,
                baz: String,
            }
        };

        let parsed = parse_struct(tokens).unwrap();
        assert_eq!(parsed.name.to_string(), "Foo");
        assert!(parsed.kind.is_named());

        let fields = parsed.kind.fields();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].effective_name(), "bar");
        assert_eq!(fields[1].effective_name(), "baz");
    }

    #[test]
    fn test_parse_tuple_struct() {
        let tokens = quote! {
            struct Point(i32, i32);
        };

        let parsed = parse_struct(tokens).unwrap();
        assert_eq!(parsed.name.to_string(), "Point");
        assert!(parsed.kind.is_tuple());

        let fields = parsed.kind.fields();
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn test_parse_unit_struct() {
        let tokens = quote! {
            struct Unit;
        };

        let parsed = parse_struct(tokens).unwrap();
        assert_eq!(parsed.name.to_string(), "Unit");
        assert!(parsed.kind.is_unit());
    }

    #[test]
    fn test_parse_simple_enum() {
        let tokens = quote! {
            enum Color {
                Red,
                Green,
                Blue,
            }
        };

        let parsed = parse_enum(tokens).unwrap();
        assert_eq!(parsed.name.to_string(), "Color");
        assert_eq!(parsed.variants.len(), 3);
        assert_eq!(parsed.variants[0].effective_name(), "Red");
        assert_eq!(parsed.variants[1].effective_name(), "Green");
        assert_eq!(parsed.variants[2].effective_name(), "Blue");
    }

    #[test]
    fn test_parse_enum_with_fields() {
        let tokens = quote! {
            enum Message {
                Quit,
                Move { x: i32, y: i32 },
                Write(String),
            }
        };

        let parsed = parse_enum(tokens).unwrap();
        assert_eq!(parsed.name.to_string(), "Message");
        assert_eq!(parsed.variants.len(), 3);

        assert!(parsed.variants[0].kind.is_unit());
        assert!(parsed.variants[1].kind.is_struct());
        assert!(parsed.variants[2].kind.is_tuple());
    }

    #[test]
    fn test_parse_struct_with_rename() {
        let tokens = quote! {
            #[facet(rename_all = "camelCase")]
            struct Config {
                user_name: String,
                max_count: u32,
            }
        };

        let parsed = parse_struct(tokens).unwrap();
        assert_eq!(parsed.attrs.rename_all, Some(RenameRule::CamelCase));

        let fields = parsed.kind.fields();
        assert_eq!(fields[0].effective_name(), "userName");
        assert_eq!(fields[1].effective_name(), "maxCount");
    }

    #[test]
    fn test_parse_struct_with_doc() {
        let tokens = quote! {
            #[doc = " This is a doc comment"]
            struct Documented {
                #[doc = " Field doc"]
                field: u32,
            }
        };

        let parsed = parse_struct(tokens).unwrap();
        assert!(!parsed.attrs.doc.is_empty());

        let fields = parsed.kind.fields();
        assert!(!fields[0].attrs.doc.is_empty());
    }

    #[test]
    fn test_parse_repr_c() {
        let tokens = quote! {
            #[repr(C)]
            struct CStruct {
                a: u8,
                b: u16,
            }
        };

        let parsed = parse_struct(tokens).unwrap();
        assert!(matches!(parsed.attrs.repr, PRepr::C(None)));
    }

    #[test]
    fn test_parse_repr_u8() {
        let tokens = quote! {
            #[repr(u8)]
            enum Small {
                A,
                B,
            }
        };

        let parsed = parse_enum(tokens).unwrap();
        assert!(matches!(
            parsed.attrs.repr,
            PRepr::Rust(Some(PrimitiveRepr::U8))
        ));
    }
}
