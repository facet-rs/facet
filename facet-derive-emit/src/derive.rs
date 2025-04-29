use facet_derive_parse::{ToTokens, *};
use quote::quote;
use std::borrow::Cow;

use crate::{LifetimeName, RenameRule, process_enum, process_struct};

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
