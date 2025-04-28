use std::borrow::Cow;

use facet_derive_parse::*;

use crate::{BoundedGenericParams, RenameRule, process_enum, process_struct};

/// Removes the `r#` prefix from a raw identifier string, if present.
pub(crate) fn normalize_ident_str(ident_str: &str) -> &str {
    ident_str.strip_prefix("r#").unwrap_or(ident_str)
}

pub fn facet_derive(input: TokenStream) -> TokenStream {
    let mut i = input.to_token_iter();

    // Parse as TypeDecl
    match i.parse::<Cons<AdtDecl, EndOfStream>>() {
        Ok(it) => match it.first {
            AdtDecl::Struct(parsed) => process_struct::process_struct(parsed),
            AdtDecl::Enum(parsed) => process_enum::process_enum(parsed),
        },
        Err(err) => {
            panic!(
                "Could not parse type declaration: {}\nError: {}",
                input, err
            );
        }
    }
}

pub(crate) struct ContainerAttributes {
    pub code: String,
    pub rename_rule: Option<RenameRule>,
}

/// Generate a static declaration that exports the crate
pub(crate) fn generate_static_decl(type_name: &str) -> String {
    format!(
        "#[used]\nstatic {}_SHAPE: &'static ::facet::Shape = <{} as ::facet::Facet>::SHAPE;",
        RenameRule::ScreamingSnakeCase.apply(type_name),
        type_name
    )
}

pub(crate) fn build_maybe_doc(attrs: &[Attribute]) -> String {
    let doc_lines: Vec<_> = attrs
        .iter()
        .filter_map(|attr| match &attr.body.content {
            AttributeInner::Doc(doc_inner) => Some(doc_inner.value.value()),
            _ => None,
        })
        .collect();

    if doc_lines.is_empty() {
        String::new()
    } else {
        format!(r#".doc(&[{}])"#, doc_lines.join(","))
    }
}

pub(crate) struct FieldInfo<'a> {
    /// something like `r#type`
    pub(crate) raw_field_name: &'a str,

    /// something like `type`
    pub(crate) normalized_field_name: &'a str,

    /// something like `String`
    pub(crate) field_type: &'a str,

    /// something like `Person`
    pub(crate) struct_name: &'a str,

    /// the bounded generic params for the container/struct
    pub(crate) bgp: &'a BoundedGenericParams,

    /// the attributes for that field
    pub(crate) attrs: &'a [Attribute],

    /// the base field offset — for structs it's always zero, for
    /// enums it depends on the variant/discriminant
    pub(crate) base_field_offset: Option<&'a str>,

    /// the rename rule to use for the container
    pub(crate) rename_rule: Option<RenameRule>,
}

/// Generates field definitions for a struct
///
/// `base_field_offset` applies a shift to the field offset, which is useful for
/// generating fields for a struct that is part of a #[repr(C)] enum.
pub(crate) fn gen_struct_field<'a>(fi: FieldInfo<'a>) -> String {
    // Determine field flags
    let mut flags = "::facet::FieldFlags::EMPTY";
    let mut attribute_list: Vec<String> = vec![];
    let mut doc_lines: Vec<&str> = vec![];
    let mut shape_of = "shape_of";
    // Start with the normalized name, potentially overridden by `rename`
    let mut name_for_metadata: Cow<'a, str> = Cow::Borrowed(fi.normalized_field_name);
    let mut has_explicit_rename = false;
    for attr in fi.attrs {
        match &attr.body.content {
            AttributeInner::Facet(facet_attr) => match &facet_attr.inner.content {
                FacetInner::Sensitive(_ksensitive) => {
                    flags = "::facet::FieldFlags::SENSITIVE";
                    attribute_list.push("::facet::FieldAttribute::Sensitive".to_string());
                }
                FacetInner::Default(_) => {
                    attribute_list.push("::facet::FieldAttribute::Default(None)".to_string());
                }
                FacetInner::DefaultEquals(inner) => {
                    attribute_list.push(format!(
                        r#"::facet::FieldAttribute::Default(Some(|ptr| {{
                            unsafe {{ ptr.put({}()) }}
                        }}))"#,
                        inner
                            .value
                            .value()
                            .trim_start_matches('"')
                            .trim_end_matches('"') // FIXME: that's pretty bad
                    ));
                }
                FacetInner::Transparent(_) => {
                    // Not applicable on fields; ignore.
                }
                FacetInner::Invariants(_invariant_inner) => {
                    panic!("fields cannot have invariants")
                }
                FacetInner::Opaque(_kopaque) => {
                    shape_of = "shape_of_opaque";
                }
                FacetInner::DenyUnknownFields(_) => {
                    // not applicable on fields
                }
                FacetInner::RenameAll(_) => {
                    // not applicable on fields
                }
                FacetInner::Other(tt) => {
                    let attr_str = tt.tokens_to_string();

                    // Split the attributes by commas to handle multiple attributes
                    let attrs = attr_str.split(',').map(|s| s.trim()).collect::<Vec<_>>();

                    for attr in attrs {
                        if let Some(equal_pos) = attr.find('=') {
                            let key = attr[..equal_pos].trim();
                            if key == "rename" {
                                has_explicit_rename = true;
                                let value = attr[equal_pos + 1..].trim().trim_matches('"');
                                // Use the renamed value for metadata name
                                name_for_metadata = Cow::Owned(value.to_string());
                                // Keep the Rename attribute for reflection
                                attribute_list.push(format!(
                                    r#"::facet::FieldAttribute::Rename({:?})"#,
                                    value
                                ));
                            } else if key == "skip_serializing_if" {
                                let value = attr[equal_pos + 1..].trim();
                                attribute_list.push(format!(
                                    r#"::facet::FieldAttribute::SkipSerializingIf(unsafe {{ ::std::mem::transmute({value} as fn(&{field_type}) -> bool) }})"#, field_type = fi.field_type
                                ));
                            } else {
                                attribute_list.push(format!(
                                    r#"::facet::FieldAttribute::Arbitrary({:?})"#,
                                    attr
                                ));
                            }
                        } else if attr == "skip_serializing" {
                            attribute_list
                                .push(r#"::facet::FieldAttribute::SkipSerializing"#.to_string());
                        } else if attr == "sensitive" {
                            flags = "::facet::FieldFlags::SENSITIVE";
                            attribute_list.push("::facet::FieldAttribute::Sensitive".to_string());
                        } else {
                            attribute_list
                                .push(format!(r#"::facet::FieldAttribute::Arbitrary({:?})"#, attr));
                        }
                    }
                }
            },
            AttributeInner::Doc(doc_inner) => doc_lines.push(doc_inner.value.value()),
            AttributeInner::Repr(_) => {
                // muffin
            }
            AttributeInner::Any(_) => {
                // muffin two
            }
        }
    }

    // Apply rename_all rule if there's no explicit rename attribute
    if !has_explicit_rename && fi.rename_rule.is_some() {
        // Only apply to named fields (not tuple indices)
        if !fi.normalized_field_name.chars().all(|c| c.is_ascii_digit()) {
            let renamed = fi.rename_rule.unwrap().apply(fi.normalized_field_name);
            attribute_list.push(format!(r#"::facet::FieldAttribute::Rename({:?})"#, renamed));
            name_for_metadata = Cow::Owned(renamed);
        }
    }

    let attributes = attribute_list.join(",");

    let maybe_field_doc = if doc_lines.is_empty() {
        String::new()
    } else {
        format!(r#".doc(&[{}])"#, doc_lines.join(","))
    };

    let maybe_base_field_offset = fi
        .base_field_offset
        .map(|offset| format!(" + {offset}"))
        .unwrap_or_default();

    // Generate each field definition
    format!(
        "::facet::Field::builder()
            .name(\"{name_for_metadata}\")
            .shape(|| ::facet::{shape_of}(&|s: &{struct_name}{bgp}| &s.{raw_field_name}))
            .offset(::core::mem::offset_of!({struct_name}{bgp}, {raw_field_name}){maybe_base_field_offset})
            .flags({flags})
            .attributes(&const {{ [{attributes}] }})
            {maybe_field_doc}
            .build()",
        struct_name = fi.struct_name,
        raw_field_name = fi.raw_field_name,
        bgp = fi.bgp.display_without_bounds()
    )
}

pub(crate) fn build_where_clauses(
    where_clauses: Option<&WhereClauses>,
    generics: Option<&GenericParams>,
) -> String {
    let mut where_clauses_s: Vec<String> = vec![];
    if let Some(wc) = where_clauses {
        for c in &wc.clauses.0 {
            where_clauses_s.push(c.value.to_string())
        }
    }

    if let Some(generics) = generics {
        for p in &generics.params.0 {
            match &p.value {
                GenericParam::Lifetime { name, .. } => {
                    where_clauses_s.push(format!("{name}: '__facet"));
                    where_clauses_s.push(format!("'__facet: {name}"));
                }
                GenericParam::Const { .. } => {
                    // ignore for now
                }
                GenericParam::Type { name, .. } => {
                    where_clauses_s.push(format!("{name}: ::facet::Facet<'__facet>"));
                }
            }
        }
    }

    if where_clauses_s.is_empty() {
        "".to_string()
    } else {
        format!("where {}", where_clauses_s.join(", "))
    }
}

pub(crate) fn build_type_params(generics: Option<&GenericParams>) -> String {
    let mut type_params_s: Vec<String> = vec![];
    if let Some(generics) = generics {
        for p in &generics.params.0 {
            match &p.value {
                GenericParam::Lifetime { .. } => {
                    // ignore for now
                }
                GenericParam::Const { .. } => {
                    // ignore for now
                }
                GenericParam::Type { name, .. } => {
                    type_params_s.push(format!(
                        "::facet::TypeParam {{ name: {:?}, shape: || <{name} as ::facet::Facet>::SHAPE }}",
                        // debug fmt because we want it to be quoted & escaped, but to_string because we don't want the `Ident { .. }`
                        name.to_string()
                    ));
                }
            }
        }
    }

    if type_params_s.is_empty() {
        "".to_string()
    } else {
        format!(".type_params(&[{}])", type_params_s.join(", "))
    }
}

pub(crate) fn build_container_attributes(attributes: &[Attribute]) -> ContainerAttributes {
    let mut items: Vec<Cow<str>> = vec![];
    let mut rename_all_rule: Option<RenameRule> = None;

    for attr in attributes {
        match &attr.body.content {
            AttributeInner::Facet(facet_attr) => match &facet_attr.inner.content {
                FacetInner::DenyUnknownFields(_) => {
                    items.push("::facet::ShapeAttribute::DenyUnknownFields".into());
                }
                FacetInner::DefaultEquals(_) | FacetInner::Default(_) => {
                    items.push("::facet::ShapeAttribute::Default".into());
                }
                FacetInner::Transparent(_) => {
                    items.push("::facet::ShapeAttribute::Transparent".into());
                }
                FacetInner::RenameAll(rename_all_inner) => {
                    let rule_str = rename_all_inner.value.value().trim_matches('"');
                    if let Some(rule) = RenameRule::from_str(rule_str) {
                        rename_all_rule = Some(rule);
                        items.push(
                            format!(r#"::facet::ShapeAttribute::RenameAll({:?})"#, rule_str).into(),
                        );
                    } else {
                        panic!("Invalid rename_all value: {:?}", rule_str);
                    }
                }
                FacetInner::Sensitive(_) => {
                    // TODO
                }
                FacetInner::Invariants(_) => {
                    // dealt with elsewhere
                }
                FacetInner::Opaque(_) => {
                    // TODO
                }
                FacetInner::Other(other) => {
                    let attr_str = other.tokens_to_string();
                    if let Some(equal_pos) = attr_str.find('=') {
                        let key = attr_str[..equal_pos].trim();
                        if key == "rename_all" {
                            let value = attr_str[equal_pos + 1..].trim().trim_matches('"');
                            if let Some(rule) = RenameRule::from_str(value) {
                                rename_all_rule = Some(rule);
                                items.push(
                                    format!(r#"::facet::ShapeAttribute::RenameAll({:?})"#, value)
                                        .into(),
                                );
                            } else {
                                panic!("Invalid rename_all value: {:?}", value);
                            }
                        } else {
                            items.push(
                                format!(
                                    r#"::facet::ShapeAttribute::Arbitrary({:?})"#,
                                    other.tokens_to_string()
                                )
                                .into(),
                            );
                        }
                    } else {
                        items.push(
                            format!(
                                r#"::facet::ShapeAttribute::Arbitrary({:?})"#,
                                other.tokens_to_string()
                            )
                            .into(),
                        );
                    }
                }
            },
            _ => {
                // do nothing.
            }
        }
    }

    let attributes_string = if items.is_empty() {
        "".to_string()
    } else {
        format!(".attributes(&[{}])", items.join(", "))
    };

    ContainerAttributes {
        code: attributes_string,
        rename_rule: rename_all_rule,
    }
}

pub(crate) fn get_discriminant_value(lit: &Literal) -> i64 {
    let s = lit.to_string();
    get_discriminant_value_from_str(&s)
}

fn strip_underscores(s: &str) -> Cow<str> {
    if s.contains('_') {
        Cow::Owned(s.chars().filter(|&c| c != '_').collect())
    } else {
        Cow::Borrowed(s)
    }
}

fn get_discriminant_value_from_str(s: &str) -> i64 {
    let s = s.trim();

    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        let hex = strip_underscores(hex);
        i64::from_str_radix(&hex, 16).expect("Invalid hex literal for discriminant")
    } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        let bin = strip_underscores(bin);
        i64::from_str_radix(&bin, 2).expect("Invalid binary literal for discriminant")
    } else if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        let oct = strip_underscores(oct);
        i64::from_str_radix(&oct, 8).expect("Invalid octal literal for discriminant")
    } else {
        // Plain decimal. Support optional _ separators (Rust literals)
        let parsed = strip_underscores(s);
        parsed
            .parse::<i64>()
            .expect("Invalid decimal literal for discriminant")
    }
}

#[cfg(test)]
mod tests {
    use super::get_discriminant_value_from_str;

    #[test]
    fn test_decimal_discriminants() {
        assert_eq!(get_discriminant_value_from_str("7"), 7);
        assert_eq!(get_discriminant_value_from_str("10"), 10);
        assert_eq!(get_discriminant_value_from_str("123_456"), 123456);
        assert_eq!(get_discriminant_value_from_str(" 42 "), 42);
    }

    #[test]
    fn test_hex_discriminants() {
        assert_eq!(get_discriminant_value_from_str("0x01"), 1);
        assert_eq!(get_discriminant_value_from_str("0x7F"), 127);
        assert_eq!(get_discriminant_value_from_str("0x80"), 128);
        assert_eq!(get_discriminant_value_from_str("0x10"), 16);
        assert_eq!(get_discriminant_value_from_str("0xfeed"), 0xfeed);
        assert_eq!(get_discriminant_value_from_str("0xBEEF"), 0xBEEF);
        assert_eq!(get_discriminant_value_from_str("0xBE_EF"), 0xBEEF);
        assert_eq!(get_discriminant_value_from_str("0X1A"), 26);
    }

    #[test]
    fn test_binary_discriminants() {
        assert_eq!(get_discriminant_value_from_str("0b0000_0000"), 0);
        assert_eq!(get_discriminant_value_from_str("0b0000_0001"), 1);
        assert_eq!(get_discriminant_value_from_str("0b0000_0010"), 2);
        assert_eq!(get_discriminant_value_from_str("0b0000_0100"), 4);
        assert_eq!(get_discriminant_value_from_str("0b0000_0111"), 7);
        assert_eq!(get_discriminant_value_from_str("0B1011"), 11);
    }

    #[test]
    fn test_octal_discriminants() {
        assert_eq!(get_discriminant_value_from_str("0o77"), 63);
        assert_eq!(get_discriminant_value_from_str("0o077"), 63);
        assert_eq!(get_discriminant_value_from_str("0o123"), 83);
        assert_eq!(get_discriminant_value_from_str("0o1_234"), 668);
        assert_eq!(get_discriminant_value_from_str("0O345"), 229);
    }

    #[test]
    fn test_mixed_notations() {
        assert_eq!(get_discriminant_value_from_str("1"), 1);
        assert_eq!(get_discriminant_value_from_str("0xA"), 10);
        assert_eq!(get_discriminant_value_from_str("0b1111"), 15);
        assert_eq!(get_discriminant_value_from_str("0o77"), 63);
    }
}
