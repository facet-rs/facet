use crate::RenameRule;

/// All the supported facet attributes, e.g. `#[facet(sensitive)]` `#[facet(rename_all)]`, etc.
///
/// Stands for `parsed facet attr`
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

/// Parsed attr
pub enum PAttr {
    /// A single line of doc comments
    /// `#[doc = "Some doc"], or `/// Some doc`, same thing
    Doc { line: String },

    /// A representation attribute
    Repr { repr: PRepr },
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

/// Parsed enum (given attributes etc.)
pub struct PEnum {
    pub name: String,
    pub repr: PRepr,
}

pub enum PRepr {
    Rust,
    Transparent,
    C,
    Primitive(PrimitiveRepr),
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

pub fn parse_attributes(attrs: &[facet_derive_parse::Attribute]) -> Vec<PFacetAttr> {
    let mut result = Vec::new();

    for attr in attrs {
        if let facet_derive_parse::AttributeInner::Facet(facet_attr) = &attr.body.content {
            match &facet_attr.inner.content {
                facet_derive_parse::FacetInner::Sensitive(_) => {
                    result.push(PFacetAttr::Sensitive);
                }
                facet_derive_parse::FacetInner::Opaque(_) => {
                    result.push(PFacetAttr::Opaque);
                }
                facet_derive_parse::FacetInner::Transparent(_) => {
                    result.push(PFacetAttr::Transparent);
                }
                facet_derive_parse::FacetInner::Invariants(invariants_inner) => {
                    // Get the function name as string from the literal
                    let fn_name = invariants_inner.value.value().trim_matches('"').to_string();
                    result.push(PFacetAttr::Invariants { fn_name });
                }
                facet_derive_parse::FacetInner::DenyUnknownFields(_) => {
                    result.push(PFacetAttr::DenyUnknownFields);
                }
                facet_derive_parse::FacetInner::DefaultEquals(default_eq_inner) => {
                    let fn_name = default_eq_inner.value.value().trim_matches('"').to_string();
                    result.push(PFacetAttr::DefaultEquals { fn_name });
                }
                facet_derive_parse::FacetInner::Default(_) => {
                    result.push(PFacetAttr::Default);
                }
                facet_derive_parse::FacetInner::RenameAll(rename_all_inner) => {
                    let rule_str = rename_all_inner.value.value().trim_matches('"');
                    if let Some(rule) = RenameRule::from_str(rule_str) {
                        result.push(PFacetAttr::RenameAll { rule });
                    } else {
                        panic!(
                            "Invalid value for rename_all: unrecognized rename rule (got: {rule_str:?})"
                        );
                    }
                }
                facet_derive_parse::FacetInner::Other(tts) => {
                    // FIXME: that's bad — we should parse that in `facet-derive-parse`

                    // flatten to string and parse for arbitrary attributes
                    let attr_str = tts.iter().map(|tt| tt.to_string()).collect::<String>();
                    let attrs = attr_str.split(',').map(|s| s.trim()).collect::<Vec<_>>();
                    for attr in attrs {
                        if let Some(equal_pos) = attr.find('=') {
                            let key = attr[..equal_pos].trim();
                            let value = attr[equal_pos + 1..].trim().trim_matches('"');
                            if key == "rename" {
                                result.push(PFacetAttr::Rename {
                                    name: value.to_string(),
                                });
                            } else if key == "rename_all" {
                                if let Some(rule) = RenameRule::from_str(value) {
                                    result.push(PFacetAttr::RenameAll { rule });
                                } else {
                                    panic!(
                                        "Invalid value for rename_all: unrecognized rename rule (got: {value:?})"
                                    );
                                }
                            } else {
                                result.push(PFacetAttr::Arbitrary {
                                    content: attr.to_string(),
                                });
                            }
                        } else if !attr.is_empty() {
                            result.push(PFacetAttr::Arbitrary {
                                content: attr.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    result
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
