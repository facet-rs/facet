use facet_derive_parse::{ToTokens, *};
use quote::quote;
use std::borrow::Cow;

use crate::{BoundedGenericParams, LifetimeName, RenameRule, process_enum, process_struct};

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
    pub tokens: TokenStream,
    pub rename_rule: Option<RenameRule>,
}

impl quote::ToTokens for ContainerAttributes {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(self.tokens.clone());
    }
}

/// Generate a static declaration that exports the crate
pub(crate) fn generate_static_decl(type_name: &Ident) -> TokenStream {
    let type_name_str = type_name.to_string();
    let screaming_snake_name = RenameRule::ScreamingSnakeCase.apply(&type_name_str);

    let static_name_ident = quote::format_ident!("{}_SHAPE", screaming_snake_name);

    quote! {
        #[used]
        static #static_name_ident: &'static ::facet::Shape = <#type_name as ::facet::Facet>::SHAPE;
    }
}

pub(crate) fn build_maybe_doc(attrs: &[Attribute]) -> TokenStream {
    let doc_lines: Vec<_> = attrs
        .iter()
        .filter_map(|attr| match &attr.body.content {
            AttributeInner::Doc(doc_inner) => Some(doc_inner.value.to_token_stream()),
            _ => None,
        })
        .collect();

    if doc_lines.is_empty() {
        quote! {}
    } else {
        quote! {
            .doc(&[#(#doc_lines),*])
        }
    }
}

pub(crate) struct FieldInfo<'a> {
    /// something like `r#type`
    pub(crate) raw_field_name: &'a str,

    /// something like `type`
    pub(crate) normalized_field_name: &'a str,

    /// something like `String`
    pub(crate) field_type: TokenStream,

    /// something like `Person`
    pub(crate) struct_name: &'a str,

    /// the bounded generic params for the container/struct
    pub(crate) bgp: &'a BoundedGenericParams,

    /// the attributes for that field
    pub(crate) attrs: &'a [Attribute],

    /// the base field offset — for structs it's always zero, for
    /// enums it depends on the variant/discriminant
    pub(crate) base_field_offset: Option<TokenStream>,

    /// the rename rule to use for the container
    pub(crate) rename_rule: Option<RenameRule>,
}

/// Generates field definitions for a struct
///
/// `base_field_offset` applies a shift to the field offset, which is useful for
/// generating fields for a struct that is part of a #[repr(C)] enum.
pub(crate) fn gen_struct_field<'a>(fi: FieldInfo<'a>) -> TokenStream {
    let mut flags = quote! {};
    let mut flags_empty = true;

    let mut attribute_list: Vec<TokenStream> = vec![];
    let mut doc_lines: Vec<TokenStream> = vec![];
    let mut shape_of = quote! { shape_of };
    let mut display_name: Cow<'a, str> = Cow::Borrowed(fi.normalized_field_name);
    let mut has_explicit_rename = false;

    for attr in fi.attrs {
        match &attr.body.content {
            AttributeInner::Facet(facet_attr) => {
                // Iterate over the comma-delimited items inside #[facet(...)]
                for delimited_facet_inner in &facet_attr.inner.content.0 {
                    let facet_inner = &delimited_facet_inner.value; // Get the FacetInner
                    match facet_inner {
                        FacetInner::Sensitive(_ksensitive) => {
                            if flags_empty {
                                flags_empty = false;
                                flags = quote! { ::facet::FieldFlags::SENSITIVE };
                            } else {
                                flags = quote! { #flags.union(::facet::FieldFlags::SENSITIVE) };
                            }
                        }
                        FacetInner::Default(_) => {
                            attribute_list.push(quote! { ::facet::FieldAttribute::Default(None) });
                        }
                        FacetInner::DefaultEquals(inner) => {
                            let field_ty = &fi.field_type;
                            let default_expr = inner.expr.to_token_stream();
                            attribute_list.push(quote! {
                                ::facet::FieldAttribute::Default(Some(|ptr| {
                                    unsafe { ptr.put::<#field_ty>(#default_expr) }
                                }))
                            });
                        }
                        FacetInner::Transparent(_) => {
                            // Not applicable on fields; ignore.
                        }
                        FacetInner::Invariants(_invariant_inner) => {
                            panic!("fields cannot have invariants")
                        }
                        FacetInner::Opaque(_kopaque) => {
                            shape_of = quote! { shape_of_opaque };
                        }
                        FacetInner::DenyUnknownFields(_) => {
                            // not applicable on fields
                        }
                        FacetInner::Rename(rename_inner) => {
                            let name_str = rename_inner.value.as_str().to_string();
                            has_explicit_rename = true;
                            display_name = Cow::Owned(name_str);
                        }
                        FacetInner::RenameAll(_) => {
                            // not applicable on fields
                        }
                        FacetInner::SkipSerializing(_skip_serializing_inner) => {
                            if flags_empty {
                                flags_empty = false;
                                flags = quote! { ::facet::FieldFlags::SKIP_SERIALIZING };
                            } else {
                                flags =
                                    quote! { #flags.union(::facet::FieldFlags::SKIP_SERIALIZING) };
                            }
                        }
                        FacetInner::SkipSerializingIf(skip_serializing_if_inner) => {
                            let predicate = skip_serializing_if_inner.expr.to_token_stream();
                            let field_ty = &fi.field_type;
                            attribute_list.push(quote! {
                                ::facet::FieldAttribute::SkipSerializingIf(unsafe { ::std::mem::transmute((#predicate) as fn(&#field_ty) -> bool) })
                            });
                        }
                        FacetInner::Arbitrary(tt) => {
                            let attr = tt.tokens_to_string();
                            attribute_list.push(quote! {
                                ::facet::FieldAttribute::Arbitrary(#attr)
                            });
                        }
                    }
                }
            }
            AttributeInner::Doc(doc_inner) => doc_lines.push(doc_inner.value.to_token_stream()),
            AttributeInner::Repr(_) => {
                // muffin
            }
            AttributeInner::Any(_) => {
                // muffin two
            }
        }
    }

    // Apply container-level rename_all rule if there's no explicit rename attribute
    if !has_explicit_rename && fi.rename_rule.is_some() {
        // Only apply to named fields (not tuple indices)
        if !fi.normalized_field_name.chars().all(|c| c.is_ascii_digit()) {
            let renamed = fi.rename_rule.unwrap().apply(fi.normalized_field_name);
            // Don't add Rename attribute again if it was added via Arbitrary(rename=...)
            // The `has_explicit_rename` flag covers this.
            attribute_list.push(quote! { ::facet::FieldAttribute::Rename(#renamed) });
            display_name = Cow::Owned(renamed);
        }
    }

    let maybe_attributes = if attribute_list.is_empty() {
        quote! {}
    } else {
        quote! {
            .attributes(&const { [#(#attribute_list),*] })
        }
    };

    let maybe_field_doc = if doc_lines.is_empty() {
        quote! {}
    } else {
        quote! {
            .doc(&[#(#doc_lines),*])
        }
    };

    let maybe_base_field_offset = if let Some(offset) = fi.base_field_offset {
        quote! { + #offset }
    } else {
        quote! {}
    };

    let maybe_flags = if flags_empty {
        quote! {}
    } else {
        quote! { .flags(#flags) }
    };

    let struct_name = quote::format_ident!("{}", fi.struct_name);

    let raw_field_name = if fi.raw_field_name.chars().all(|c| c.is_numeric()) {
        let field_index = fi.raw_field_name.parse::<usize>().unwrap();
        let literal = TokenTree::Literal(Literal::usize_unsuffixed(field_index));
        quote! { #literal }
    } else {
        let ident = quote::format_ident!("{}", fi.raw_field_name);
        quote! { #ident }
    };

    let bgp = fi.bgp.display_without_bounds();

    // Generate each field definition
    quote! {
        ::facet::Field::builder()
            .name(#display_name)
            .shape(|| ::facet::#shape_of(&|s: &#struct_name #bgp| &s.#raw_field_name))
            .offset(::core::mem::offset_of!(#struct_name #bgp,#raw_field_name) #maybe_base_field_offset)
            #maybe_flags
            #maybe_attributes
            #maybe_field_doc
            .build()
    }
}

pub(crate) fn build_where_clauses(
    where_clauses: Option<&WhereClauses>,
    generics: Option<&GenericParams>,
) -> TokenStream {
    let mut where_clause_tokens = TokenStream::new();
    let mut has_clauses = false;

    if let Some(wc) = where_clauses {
        for c in &wc.clauses.0 {
            if has_clauses {
                where_clause_tokens.extend(quote! { , });
            }
            where_clause_tokens.extend(c.value.to_token_stream());
            has_clauses = true;
        }
    }

    if let Some(generics) = generics {
        for p in &generics.params.0 {
            match &p.value {
                GenericParam::Lifetime { name, .. } => {
                    let facet_lifetime = LifetimeName(quote::format_ident!("{}", "__facet"));
                    let lifetime = LifetimeName(name.name.clone());
                    if has_clauses {
                        where_clause_tokens.extend(quote! { , });
                    }
                    where_clause_tokens
                        .extend(quote! { #lifetime: #facet_lifetime, #facet_lifetime: #lifetime });

                    has_clauses = true;
                }
                GenericParam::Const { .. } => {
                    // ignore for now
                }
                GenericParam::Type { name, .. } => {
                    if has_clauses {
                        where_clause_tokens.extend(quote! { , });
                    }
                    where_clause_tokens.extend(quote! { #name: ::facet::Facet<'__facet> });
                    has_clauses = true;
                }
            }
        }
    }

    if !has_clauses {
        quote! {}
    } else {
        quote! { where #where_clause_tokens }
    }
}

pub(crate) fn build_type_params(generics: Option<&GenericParams>) -> TokenStream {
    let mut type_params = Vec::new();
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
                    let name_str = name.to_string();
                    type_params.push(quote! {
                        ::facet::TypeParam {
                            name: #name_str,
                            shape: || <#name as ::facet::Facet>::SHAPE
                        }
                    });
                }
            }
        }
    }

    if type_params.is_empty() {
        quote! {}
    } else {
        quote! { .type_params(&[#(#type_params),*]) }
    }
}

pub(crate) fn build_container_attributes(attributes: &[Attribute]) -> ContainerAttributes {
    let mut items = Vec::new();
    let mut rename_all_rule: Option<RenameRule> = None;

    for attr in attributes {
        match &attr.body.content {
            AttributeInner::Facet(facet_attr) => {
                // Iterate over the comma-delimited items inside #[facet(...)]
                for delimited_facet_inner in &facet_attr.inner.content.0 {
                    let facet_inner = &delimited_facet_inner.value; // Get the FacetInner
                    match facet_inner {
                        FacetInner::DenyUnknownFields(_) => {
                            items.push(quote! { ::facet::ShapeAttribute::DenyUnknownFields });
                        }
                        FacetInner::DefaultEquals(_) | FacetInner::Default(_) => {
                            items.push(quote! { ::facet::ShapeAttribute::Default });
                        }
                        FacetInner::Transparent(_) => {
                            items.push(quote! { ::facet::ShapeAttribute::Transparent });
                        }
                        FacetInner::Rename(_) => {
                            panic!("Rename not supported at container level")
                        }
                        FacetInner::RenameAll(rename_all_inner) => {
                            let rule_str = rename_all_inner.value.as_str();
                            if let Some(rule) = RenameRule::from_str(rule_str) {
                                rename_all_rule = Some(rule);
                                items
                                    .push(quote! { ::facet::ShapeAttribute::RenameAll(#rule_str) });
                            } else {
                                panic!("Invalid rename_all value: {:?}", rule_str);
                            }
                        }
                        FacetInner::Sensitive(_) => {
                            // Not typically applied at container level, maybe log a warning or ignore?
                            // TODO
                        }
                        FacetInner::Invariants(_) => {
                            // This is handled separately in process_struct/process_enum
                        }
                        FacetInner::Opaque(_) => {
                            // Not typically applied at container level, maybe log a warning or ignore?
                            // TODO
                        }
                        FacetInner::Arbitrary(tt) => {
                            let attr_content = tt.tokens_to_string();
                            items
                                .push(quote! { ::facet::ShapeAttribute::Arbitrary(#attr_content) });
                        }
                        // Added missing arms based on FacetInner definition
                        FacetInner::SkipSerializing(_) => {
                            // Not applicable at container level
                        }
                        FacetInner::SkipSerializingIf(_) => {
                            // Not applicable at container level
                        }
                    }
                }
            }
            _ => {
                // Ignore non-#[facet(...)] attributes like #[repr], #[doc], etc.
            }
        }
    }

    let tokens = if items.is_empty() {
        quote! {}
    } else {
        quote! { .attributes(&[#(#items),*]) }
    };

    ContainerAttributes {
        tokens,
        rename_rule: rename_all_rule, // Return the found rename_all rule (if any)
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
