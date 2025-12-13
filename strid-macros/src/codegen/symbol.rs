use std::fmt::{self, Display};

use unsynn::{IParse, Ident, Literal, ToTokenIter};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Symbol(&'static str);

// pub const NO_AUTO_REF: Symbol = Symbol("no_auto_ref");
// pub const OWNED: Symbol = Symbol("owned");
pub const CLONE: Symbol = Symbol("clone");
pub const DEBUG: Symbol = Symbol("debug");
pub const DISPLAY: Symbol = Symbol("display");
pub const ORD: Symbol = Symbol("ord");
pub const SERDE: Symbol = Symbol("serde");
pub const REF: Symbol = Symbol("ref_name");
pub const REF_DOC: Symbol = Symbol("ref_doc");
pub const REF_ATTR: Symbol = Symbol("ref_attr");
pub const OWNED_ATTR: Symbol = Symbol("owned_attr");
pub const NO_STD: Symbol = Symbol("no_std");
pub const NO_EXPOSE: Symbol = Symbol("no_expose");
pub const VALIDATOR: Symbol = Symbol(super::check_mode::VALIDATOR);
pub const NORMALIZER: Symbol = Symbol(super::check_mode::NORMALIZER);

impl PartialEq<Symbol> for Ident {
    fn eq(&self, word: &Symbol) -> bool {
        self == word.0
    }
}

impl PartialEq<Symbol> for &Ident {
    fn eq(&self, word: &Symbol) -> bool {
        *self == word.0
    }
}

impl Display for Symbol {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

fn get_lit_str(attr_name: Symbol, lit: &Literal) -> Result<String, String> {
    // proc_macro2::Literal doesn't have variants, so we parse its string representation
    let lit_str = lit.to_string();

    // Check if it's a string literal (starts and ends with quotes)
    if lit_str.starts_with('"') && lit_str.ends_with('"') && lit_str.len() >= 2 {
        // Remove the surrounding quotes and unescape
        Ok(lit_str[1..lit_str.len() - 1].to_string())
    } else {
        Err(format!(
            "expected attribute `{}` to have a string value (`{} = \"value\"`)",
            attr_name, attr_name
        ))
    }
}

// fn parse_lit_into_path(attr_name: Symbol, lit: &syn::Lit) -> Result<syn::Path, ()> {
//     let string = get_lit_str( attr_name, lit)?;
//     parse_lit_str(string).map_err(|_| {
//         syn::Error::new_spanned(lit, format!("failed to parse path: {:?}", string.value()))
//     })
// }

/// Parse a literal into a string.
pub(super) fn parse_lit_into_string(attr_name: Symbol, lit: &Literal) -> Result<String, String> {
    get_lit_str(attr_name, lit)
}

/// Parse a string literal into a type by parsing its contents.
pub(super) fn parse_lit_into_type(
    _attr_name: Symbol,
    string: &str,
) -> Result<crate::grammar::Type, String> {
    let tokens: proc_macro2::TokenStream = string
        .parse()
        .map_err(|e| format!("failed to parse type from string: {}", e))?;

    let mut iter = tokens.to_token_iter();
    iter.parse::<crate::grammar::Type>()
        .map_err(|e| format!("failed to parse type: {}", e))
}
