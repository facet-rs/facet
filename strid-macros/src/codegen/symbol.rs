use std::fmt::{self, Display};

use syn::{Ident, Path};

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

impl PartialEq<Symbol> for Path {
    fn eq(&self, word: &Symbol) -> bool {
        self.is_ident(word.0)
    }
}

impl PartialEq<Symbol> for &Path {
    fn eq(&self, word: &Symbol) -> bool {
        self.is_ident(word.0)
    }
}

impl Display for Symbol {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

fn get_lit_str(attr_name: Symbol, lit: &syn::Lit) -> Result<&syn::LitStr, syn::Error> {
    if let syn::Lit::Str(lit) = lit {
        Ok(lit)
    } else {
        Err(syn::Error::new_spanned(
            lit,
            format!(
                "expected attribute `{}` to have a string value (`{} = \"value\"`)",
                attr_name, attr_name
            ),
        ))
    }
}

// fn parse_lit_into_path(attr_name: Symbol, lit: &syn::Lit) -> Result<syn::Path, ()> {
//     let string = get_lit_str( attr_name, lit)?;
//     parse_lit_str(string).map_err(|_| {
//         syn::Error::new_spanned(lit, format!("failed to parse path: {:?}", string.value()))
//     })
// }

pub(super) fn parse_expr_as_lit(expr: &syn::Expr) -> Result<&syn::Lit, syn::Error> {
    if let syn::Expr::Lit(l) = expr {
        Ok(&l.lit)
    } else {
        Err(syn::Error::new_spanned(
            expr,
            "expected a literal in this position",
        ))
    }
}

pub(super) fn parse_lit_into_type(
    attr_name: Symbol,
    lit: &syn::Lit,
) -> Result<syn::Type, syn::Error> {
    let string = get_lit_str(attr_name, lit)?;
    parse_lit_str(string).map_err(|_| {
        syn::Error::new_spanned(lit, format!("failed to parse type: {:?}", string.value()))
    })
}

pub(super) fn parse_lit_into_string(
    attr_name: Symbol,
    lit: &syn::Lit,
) -> Result<String, syn::Error> {
    let string = get_lit_str(attr_name, lit)?;
    Ok(string.value())
}

fn parse_lit_str<T>(s: &syn::LitStr) -> syn::parse::Result<T>
where
    T: syn::parse::Parse,
{
    let tokens = spanned_tokens(s)?;
    syn::parse2(tokens)
}

fn spanned_tokens(s: &syn::LitStr) -> syn::parse::Result<proc_macro2::TokenStream> {
    let stream = syn::parse_str(&s.value())?;
    Ok(respan_token_stream(stream, s.span()))
}

fn respan_token_stream(
    stream: proc_macro2::TokenStream,
    span: proc_macro2::Span,
) -> proc_macro2::TokenStream {
    stream
        .into_iter()
        .map(|token| respan_token_tree(token, span))
        .collect()
}

fn respan_token_tree(
    mut token: proc_macro2::TokenTree,
    span: proc_macro2::Span,
) -> proc_macro2::TokenTree {
    if let proc_macro2::TokenTree::Group(g) = &mut token {
        *g = proc_macro2::Group::new(g.delimiter(), respan_token_stream(g.stream(), span));
    }
    token.set_span(span);
    token
}
