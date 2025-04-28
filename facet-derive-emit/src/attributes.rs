use crate::RenameRule;

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
pub struct PName {
    pub raw: String,
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
        raw: String,
    ) -> Self {
        let mut norm_raw = raw.clone();
        // Remove Rust's raw identifier prefix, e.g. r#type -> type
        if let Some(stripped) = norm_raw.strip_prefix("r#") {
            norm_raw = stripped.to_string();
        }

        let effective = if let Some(field_rule) = field_rename_rule {
            field_rule.apply(&norm_raw)
        } else if let Some(container_rule) = container_rename_rule {
            container_rule.apply(&norm_raw)
        } else {
            norm_raw.clone()
        };

        Self { raw, effective }
    }
}

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
    pub fn type_name(&self) -> &'static str {
        match self {
            PrimitiveRepr::U8 => "u8",
            PrimitiveRepr::U16 => "u16",
            PrimitiveRepr::U32 => "u32",
            PrimitiveRepr::U64 => "u64",
            PrimitiveRepr::U128 => "u128",
            PrimitiveRepr::I8 => "i8",
            PrimitiveRepr::I16 => "i16",
            PrimitiveRepr::I32 => "i32",
            PrimitiveRepr::I64 => "i64",
            PrimitiveRepr::I128 => "i128",
            PrimitiveRepr::Isize => "isize",
            PrimitiveRepr::Usize => "usize",
        }
    }
}

/// Parsed attributes
pub struct PAttrs {
    /// An array of doc lines
    pub doc: Vec<String>,

    /// Facet attributes specifically
    pub facet_atts: Vec<PFacetAttr>,

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
            facet_atts: facet_attrs,
            repr: repr.unwrap_or(PRepr::Rust),
            rename_all,
            rename,
        }
    }
}

/// Parsed container
pub struct PContainer {
    /// Name of the container (could be a struct, an enum variant, etc.)
    pub name: String,

    /// Attributes of the container
    pub attrs: PAttrs,
}

/// Parse struct
pub struct PStruct {
    /// Container information
    pub container: PContainer,
}

/// Parsed enum (given attributes etc.)
pub struct PEnum {
    /// Container information
    pub container: PContainer,
}

/// Parsed field
pub struct PField {
    /// The field's name (with rename rules applied)
    pub name: PName,

    /// The field's type
    pub ty: String,

    /// The field's offset (can be an expression, like `offset_of!(self, field)`)
    pub offset: String,

    /// The field's attributes
    pub attrs: PAttrs,
}

impl PStruct {
    pub fn parse(s: &facet_derive_parse::Struct) -> Self {
        // Parse top-level (container) attributes for the struct.
        let pattrs = PAttrs::parse(&s.attributes);

        // Build PContainer from struct's name and attributes.
        let container = PContainer {
            name: s.name.to_string(),
            attrs: pattrs,
        };

        PStruct { container }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenameRule;

    #[test]
    fn pname_new_no_rename_rule() {
        let p = PName::new(None, None, "foo_bar".to_string());
        assert_eq!(p.raw, "foo_bar");
        assert_eq!(p.effective, "foo_bar");
    }

    #[test]
    fn pname_new_strips_raw_prefix() {
        let p = PName::new(None, None, "r#type".to_string());
        assert_eq!(p.raw, "r#type");
        assert_eq!(p.effective, "type");
    }

    #[test]
    fn pname_new_applies_field_rename_rule() {
        let rule = RenameRule::ScreamingSnakeCase;
        let p = PName::new(
            Some(RenameRule::CamelCase),
            Some(rule),
            "r#case_test".to_string(),
        );
        assert_eq!(p.effective, "CASE_TEST");
    }

    #[test]
    fn pname_new_applies_container_rename_rule() {
        let rule = RenameRule::KebabCase;
        let p = PName::new(Some(rule), None, "r#abc_def".to_string());
        assert_eq!(p.effective, "abc-def");
    }

    #[test]
    fn pname_new_field_rule_precedence() {
        let container_rule = RenameRule::PascalCase;
        let field_rule = RenameRule::CamelCase;
        let p = PName::new(
            Some(container_rule),
            Some(field_rule),
            "foo_bar".to_string(),
        );
        // CamelCase applied, not PascalCase
        assert_eq!(p.effective, "fooBar");
    }

    #[test]
    fn pname_new_empty_string() {
        let p = PName::new(None, None, "".to_string());
        assert_eq!(p.raw, "");
        assert_eq!(p.effective, "");
    }
}
