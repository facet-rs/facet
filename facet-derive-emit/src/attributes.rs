use crate::{BoundedGenericParams, RenameRule};
use facet_derive_parse::{Ident, TokenStream};
use quote::{format_ident, quote};

/// All the supported facet attributes, e.g. `#[facet(sensitive)]` `#[facet(rename_all)]`, etc.
///
/// Stands for `parsed facet attr`
#[derive(Clone)]
pub enum PFacetAttr {
    /// Valid in field
    /// `#[facet(sensitive)]` — must be censored in debug outputs
    Sensitive,

    /// Valid in container
    /// `#[facet(opaque)]` — the inner field does not have to implement
    /// `Facet`
    Opaque,

    /// Valid in container
    /// `#[facet(transparent)]` — applied on things like `NonZero<T>`, `Utf8PathBuf`,
    /// etc. — when you're doing the newtype pattern. `de/ser` is forwarded.
    Transparent,

    /// Valid in container
    /// `#[facet(invariants = "invariants_func")]` — returns a bool, is called
    /// when doing `Wip::build`
    Invariants { fn_name: String },

    /// Valid in container
    /// `#[facet(deny_unknown_fields)]`
    DenyUnknownFields,

    /// Valid in field
    /// `#[facet(default = "fn_name")]` — when deserializing and missing, use `fn_name` to provide a default value
    DefaultEquals { fn_name: String },

    /// Valid in field
    /// `#[facet(default)]` — when deserializing and missing, use the field's value from
    /// the container's `Default::default()`
    Default,

    /// Valid in field, enum variant, container
    /// An arbitrary/unknown string, like,
    /// `#[facet(bleh)]`
    Arbitrary { content: String },

    /// Valid in field
    /// `#[facet(rename = "new_name")]` — rename this field
    Rename { name: String },

    /// Valid in container
    /// `#[facet(rename_all = "rule")]` — rename all fields following a rule
    RenameAll { rule: RenameRule },
}

impl PFacetAttr {
    /// Parse a `FacetAttr` attribute into a `PFacetAttr`.
    /// Returns None if the input is not supported.
    pub fn parse(facet_attr: &facet_derive_parse::FacetAttr) -> Self {
        use facet_derive_parse::FacetInner;

        match &facet_attr.inner.content {
            FacetInner::Sensitive(_) => PFacetAttr::Sensitive,
            FacetInner::Opaque(_) => PFacetAttr::Opaque,
            FacetInner::Transparent(_) => PFacetAttr::Transparent,
            FacetInner::Invariants(invariant) => {
                let fn_name = invariant.value.value().to_string();
                PFacetAttr::Invariants { fn_name }
            }
            FacetInner::DenyUnknownFields(_) => PFacetAttr::DenyUnknownFields,
            FacetInner::DefaultEquals(default_equals) => {
                let fn_name = default_equals.value.value().to_string();
                PFacetAttr::DefaultEquals { fn_name }
            }
            FacetInner::Default(_) => PFacetAttr::Default,
            FacetInner::RenameAll(rename_all) => {
                let rule_str = rename_all.value.as_str();
                if let Some(rule) = RenameRule::from_str(rule_str) {
                    PFacetAttr::RenameAll { rule }
                } else {
                    panic!("Unknown #[facet(rename_all = ...)] rule: {}", rule_str);
                }
            }
            FacetInner::Other(tokens) => {
                // tokens is Vec<TokenTree> -- reconstruct as string for Arbitrary or try to parse rename
                if tokens.len() >= 3 {
                    // handle #[facet(rename = "...")]
                    if let (
                        Some(facet_derive_parse::TokenTree::Ident(ident)),
                        Some(facet_derive_parse::TokenTree::Punct(punct)),
                        Some(facet_derive_parse::TokenTree::Literal(lit)),
                    ) = (tokens.first(), tokens.get(1), tokens.get(2))
                    {
                        if *ident == "rename" && punct.as_char() == '=' {
                            // Remove quotes from Literal
                            let lit_str = lit.to_string();
                            let name = lit_str.trim_matches('"').to_string();
                            return PFacetAttr::Rename { name };
                        }
                    }
                }
                // fallback to Arbitrary, stringify tokens
                let content = tokens
                    .iter()
                    .map(|tt| tt.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                PFacetAttr::Arbitrary { content }
            }
        }
    }
}

/// Parsed attr
pub enum PAttr {
    /// A single line of doc comments
    /// `#[doc = "Some doc"], or `/// Some doc`, same thing
    Doc { line: String },

    /// A representation attribute
    Repr { repr: PRepr },

    /// A facet attribute
    Facet { name: String },
}

/// A parsed name, which includes the raw name and the
/// effective name.
///
/// Examples:
///
///   raw = "foo_bar", no rename rule, effective = "foo_bar"
///   raw = "foo_bar", #[facet(rename = "kiki")], effective = "kiki"
///   raw = "foo_bar", #[facet(rename_all = camelCase)], effective = "fooBar"
///   raw = "r#type", no rename rule, effective = "type"
///
#[derive(Clone)]
pub struct PName {
    /// The raw identifier, as we found it in the source code. It might
    /// be _actually_ raw, as in "r#keyword".
    pub raw: Ident,

    /// The name after applying rename rules, which might not be a valid identifier in Rust.
    /// It could be a number. It could be a kebab-case thing.
    pub effective: String,
}

impl PName {
    /// Constructs a new `PName` with the given raw name, an optional container-level rename rule,
    /// an optional field-level rename rule, and a raw identifier.
    ///
    /// Precedence:
    ///   - If field_rename_rule is Some, use it on raw for effective name
    ///   - Else if container_rename_rule is Some, use it on raw for effective name
    ///   - Else, strip raw ("r#" if present) for effective name
    pub fn new(
        container_rename_rule: Option<RenameRule>,
        field_rename_rule: Option<RenameRule>,
        raw: Ident,
    ) -> Self {
        let raw_str = raw.to_string();
        // Remove Rust's raw identifier prefix, e.g. r#type -> type
        let norm_raw_str = raw_str.strip_prefix("r#").unwrap_or(&raw_str).to_string();

        let effective = if let Some(field_rule) = field_rename_rule {
            field_rule.apply(&norm_raw_str)
        } else if let Some(container_rule) = container_rename_rule {
            container_rule.apply(&norm_raw_str)
        } else {
            norm_raw_str // Use the normalized string (without r#)
        };

        Self {
            raw: raw.clone(), // Keep the original raw identifier
            effective,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PRepr {
    Rust,
    Transparent,
    C,
    Primitive(PrimitiveRepr),
}

impl PRepr {
    /// Parse a `&str` (for example a value coming from #[repr(...)] attribute)
    /// into a `PRepr` variant.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        match s {
            "C" | "c" => Some(PRepr::C),
            "Rust" | "rust" => Some(PRepr::Rust),
            "transparent" => Some(PRepr::Transparent),
            "u8" => Some(PRepr::Primitive(PrimitiveRepr::U8)),
            "u16" => Some(PRepr::Primitive(PrimitiveRepr::U16)),
            "u32" => Some(PRepr::Primitive(PrimitiveRepr::U32)),
            "u64" => Some(PRepr::Primitive(PrimitiveRepr::U64)),
            "u128" => Some(PRepr::Primitive(PrimitiveRepr::U128)),
            "i8" => Some(PRepr::Primitive(PrimitiveRepr::I8)),
            "i16" => Some(PRepr::Primitive(PrimitiveRepr::I16)),
            "i32" => Some(PRepr::Primitive(PrimitiveRepr::I32)),
            "i64" => Some(PRepr::Primitive(PrimitiveRepr::I64)),
            "i128" => Some(PRepr::Primitive(PrimitiveRepr::I128)),
            "usize" => Some(PRepr::Primitive(PrimitiveRepr::Usize)),
            "isize" => Some(PRepr::Primitive(PrimitiveRepr::Isize)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveRepr {
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    Usize,
}

impl PrimitiveRepr {
    pub fn type_name(&self) -> TokenStream {
        match self {
            PrimitiveRepr::U8 => quote! { u8 },
            PrimitiveRepr::U16 => quote! { u16 },
            PrimitiveRepr::U32 => quote! { u32 },
            PrimitiveRepr::U64 => quote! { u64 },
            PrimitiveRepr::U128 => quote! { u128 },
            PrimitiveRepr::I8 => quote! { i8 },
            PrimitiveRepr::I16 => quote! { i16 },
            PrimitiveRepr::I32 => quote! { i32 },
            PrimitiveRepr::I64 => quote! { i64 },
            PrimitiveRepr::I128 => quote! { i128 },
            PrimitiveRepr::Isize => quote! { isize },
            PrimitiveRepr::Usize => quote! { usize },
        }
    }
}

/// Parsed attributes
#[derive(Clone)]
pub struct PAttrs {
    /// An array of doc lines
    pub doc: Vec<String>,

    /// Facet attributes specifically
    pub facet: Vec<PFacetAttr>,

    /// Representation of the facet
    pub repr: PRepr,

    /// rename_all rule (if any)
    pub rename_all: Option<RenameRule>,

    /// rename (if any)
    pub rename: Option<String>,
}

impl PAttrs {
    fn parse(attrs: &[facet_derive_parse::Attribute]) -> Self {
        let mut doc_lines: Vec<String> = Vec::new();
        let mut facet_attrs: Vec<PFacetAttr> = Vec::new();
        let mut repr: Option<PRepr> = None;
        let mut rename_all: Option<RenameRule> = None;
        let mut rename: Option<String> = None;

        for attr in attrs {
            match &attr.body.content {
                facet_derive_parse::AttributeInner::Doc(doc_attr) => {
                    // Handle doc comments
                    doc_lines.push(doc_attr.value.value().to_string());
                }
                facet_derive_parse::AttributeInner::Repr(repr_attr) => {
                    // Parse repr attribute, e.g. #[repr(C)], #[repr(transparent)], #[repr(u8)]
                    // repr_attr.attr.content is a Vec<Delimited<Ident, Operator<','>>>
                    // which represents something like ["C"], or ["u8"], or ["transparent"]
                    //
                    // We should parse each possible repr kind. But usually there's only one item.
                    //
                    // We'll take the first one and parse it, ignoring the rest.
                    let repr_items = &repr_attr.attr.content;
                    if let Some(first) = repr_items.0.first() {
                        let repr_kind = first.value.to_string();
                        match PRepr::parse(repr_kind.as_str()) {
                            Some(parsed) => repr = Some(parsed),
                            None => {
                                panic!("Unknown #[repr] attribute: {}", repr_kind);
                            }
                        }
                    } else {
                        // No content: default to Rust
                        repr = Some(PRepr::Rust);
                    }
                }
                facet_derive_parse::AttributeInner::Facet(facet_attr) => {
                    let attr = PFacetAttr::parse(facet_attr);
                    facet_attrs.push(attr);
                }
                _ => {
                    // Ignore unknown AttributeInner types
                }
            }
        }

        // Find rename and rename_all rules from facet_attrs list
        for attr in &facet_attrs {
            match attr {
                PFacetAttr::RenameAll { rule } => {
                    rename_all = Some(*rule);
                }
                PFacetAttr::Rename { name } => {
                    rename = Some(name.clone());
                }
                _ => {}
            }
        }

        Self {
            doc: doc_lines,
            facet: facet_attrs,
            repr: repr.unwrap_or(PRepr::Rust),
            rename_all,
            rename,
        }
    }

    pub(crate) fn is_transparent(&self) -> bool {
        self.facet
            .iter()
            .any(|attr| matches!(attr, PFacetAttr::Transparent))
    }
}

/// Parsed container
pub struct PContainer {
    /// Name of the container (could be a struct, an enum variant, etc.)
    pub name: String,

    /// Attributes of the container
    pub attrs: PAttrs,

    /// Generic parameters of the container
    pub bgp: BoundedGenericParams,
}

/// Parse struct
pub struct PStruct {
    /// Container information
    pub container: PContainer,

    /// Kind of struct
    pub kind: PStructKind,
}

/// Parsed enum (given attributes etc.)
pub struct PEnum {
    /// Container information
    pub container: PContainer,
}

/// Parsed field
#[derive(Clone)]
pub struct PStructField {
    /// The field's name (with rename rules applied)
    pub name: PName,

    /// The field's type
    pub ty: TokenStream,

    /// The field's offset (can be an expression, like `offset_of!(self, field)`)
    pub offset: TokenStream,

    /// The field's attributes
    pub attrs: PAttrs,
}

impl PStructField {
    /// Parse a named struct field (usual struct).
    fn from_struct_field(f: &facet_derive_parse::StructField) -> Self {
        use facet_derive_parse::ToTokens;
        Self::parse(
            &f.attributes,
            f.name.clone(),          // Pass Ident directly
            f.typ.to_token_stream(), // Convert to TokenStream
        )
    }

    /// Parse a tuple (unnamed) field for tuple structs or enum tuple variants.
    /// The index is converted to an identifier like `_0`, `_1`, etc.
    fn from_enum_field(
        attrs: &[facet_derive_parse::Attribute],
        idx: usize,
        typ: &facet_derive_parse::VerbatimUntil<facet_derive_parse::Comma>,
    ) -> Self {
        use facet_derive_parse::ToTokens;
        // Create an Ident from the index, using `_` prefix convention for tuple fields
        let name = format_ident!("_{}", idx);
        let ty = typ.to_token_stream(); // Convert to TokenStream
        Self::parse(attrs, name, ty)
    }

    /// Central parse function used by both `from_struct_field` and `from_enum_field`.
    fn parse(attrs: &[facet_derive_parse::Attribute], name: Ident, ty: TokenStream) -> Self {
        // Parse attributes for the field
        let attrs = PAttrs::parse(attrs);

        // Find container-level rename_all rule and field-level rename rule, if any
        let container_rename_rule = attrs.rename_all;
        let field_rename = attrs.rename.clone(); // Specific #[facet(rename = "...")] on the field

        // Name resolution:
        // Precedence:
        //   1. Field-level #[facet(rename = "...")]
        //   2. Container-level #[facet(rename_all = "...")]
        //   3. Raw field name (after stripping "r#")
        let raw = name.clone();

        let p_name = if let Some(explicit_name) = field_rename {
            // If #[facet(rename = "...")] is present, use it directly as the effective name.
            // Preserve the span of the original identifier.
            PName {
                raw: raw.clone(),
                effective: explicit_name,
            }
        } else {
            // Otherwise, use the PName::new logic which applies rename_all rules or normalization.
            // field_rename_rule is None because #[facet(rename_all=...)] cannot be applied to fields.
            let field_rename_rule = None;
            PName::new(container_rename_rule, field_rename_rule, raw)
        };

        // Field type as TokenStream (already provided as argument)
        let ty = ty.clone();

        // Offset string -- we don't know the offset here in generic parsing, so just default to empty
        let offset = quote! {};

        PStructField {
            name: p_name,
            ty,
            offset,
            attrs,
        }
    }
}
/// Parsed struct kind, modeled after `StructKind`.
pub enum PStructKind {
    /// A regular struct with named fields.
    Struct { fields: Vec<PStructField> },
    /// A tuple struct.
    TupleStruct { fields: Vec<PStructField> },
    /// A unit struct.
    UnitStruct,
}

impl PStructKind {
    /// Parse a `facet_derive_parse::StructKind` into a `PStructKind`.
    pub fn parse(kind: &facet_derive_parse::StructKind) -> Self {
        match kind {
            facet_derive_parse::StructKind::Struct { clauses: _, fields } => {
                let parsed_fields = fields
                    .content
                    .0
                    .iter()
                    .map(|delim| PStructField::from_struct_field(&delim.value))
                    .collect();
                PStructKind::Struct {
                    fields: parsed_fields,
                }
            }
            facet_derive_parse::StructKind::TupleStruct {
                fields,
                clauses: _,
                semi: _,
            } => {
                let parsed_fields = fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, delim)| {
                        PStructField::from_enum_field(
                            &delim.value.attributes,
                            idx,
                            &delim.value.typ,
                        )
                    })
                    .collect();
                PStructKind::TupleStruct {
                    fields: parsed_fields,
                }
            }
            facet_derive_parse::StructKind::UnitStruct {
                clauses: _,
                semi: _,
            } => PStructKind::UnitStruct,
        }
    }
}

impl PStruct {
    pub fn parse(s: &facet_derive_parse::Struct) -> Self {
        // Parse top-level (container) attributes for the struct.
        let pattrs = PAttrs::parse(&s.attributes);

        // Build PContainer from struct's name and attributes.
        let container = PContainer {
            name: s.name.to_string(),
            attrs: pattrs,
            bgp: BoundedGenericParams::parse(s.generics.as_ref()),
        };

        // Delegate struct kind parsing to PStructKind::parse
        let kind = PStructKind::parse(&s.kind);

        PStruct { container, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenameRule;
    use quote::format_ident;

    #[test]
    fn pname_new_no_rename_rule() {
        let raw_ident = format_ident!("foo_bar");
        let p = PName::new(None, None, raw_ident);
        assert_eq!(p.raw.to_string(), "foo_bar");
        assert_eq!(p.effective.to_string(), "foo_bar");
    }

    #[test]
    fn pname_new_strips_raw_prefix() {
        let raw_ident = format_ident!("r#type");
        let p = PName::new(None, None, raw_ident);
        assert_eq!(p.raw.to_string(), "r#type");
        assert_eq!(p.effective.to_string(), "type");
    }

    #[test]
    fn pname_new_applies_field_rename_rule() {
        let rule = RenameRule::ScreamingSnakeCase;
        let raw_ident = format_ident!("r#case_test");
        let p = PName::new(Some(RenameRule::CamelCase), Some(rule), raw_ident);
        assert_eq!(p.raw.to_string(), "r#case_test");
        assert_eq!(p.effective.to_string(), "CASE_TEST");
    }

    #[test]
    fn pname_new_applies_container_rename_rule() {
        let rule = RenameRule::KebabCase;
        let raw_ident = format_ident!("r#abc_def");
        let p = PName::new(Some(rule), None, raw_ident);
        assert_eq!(p.raw.to_string(), "r#abc_def");
        assert_eq!(p.effective.to_string(), "abc-def");
    }

    #[test]
    fn pname_new_field_rule_precedence() {
        let container_rule = RenameRule::PascalCase;
        let field_rule = RenameRule::CamelCase;
        let raw_ident = format_ident!("foo_bar");
        let p = PName::new(Some(container_rule), Some(field_rule), raw_ident);
        assert_eq!(p.raw.to_string(), "foo_bar");
        // CamelCase applied, not PascalCase
        assert_eq!(p.effective.to_string(), "fooBar");
    }

    #[test]
    fn pname_new_empty_string() {
        // An empty string cannot be a valid identifier.
        // Test with a valid identifier instead.
        let raw_ident = format_ident!("a");
        let p = PName::new(None, None, raw_ident);
        assert_eq!(p.raw.to_string(), "a");
        assert_eq!(p.effective.to_string(), "a");
    }
}
