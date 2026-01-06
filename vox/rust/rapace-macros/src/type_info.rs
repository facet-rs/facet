use proc_macro2::TokenStream as TokenStream2;
use unsynn::operator::names::{Comma, Gt, Lt, PathSep};
use unsynn::{Cons, EndOfStream, Except, Ident, Many, Parse, ToTokenIter, ToTokens, unsynn};

use crate::parser::AngleTokenTree;

/// Parses tokens and groups until `C` is found, handling `<...>` correctly.
type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

unsynn! {
    pub struct TypePath {
        pub leading: Option<PathSep>,
        pub first: Ident,
        pub rest: Many<Cons<PathSep, Ident>>,
    }

    pub struct ResultType {
        pub path: TypePath,
        pub _lt: Lt,
        pub ok: VerbatimUntil<Comma>,
        pub _comma: Comma,
        pub err: VerbatimUntil<Gt>,
        pub _gt: Gt,
        pub _eos: EndOfStream,
    }
}

pub fn split_result_types(ty: &TokenStream2) -> Option<(TokenStream2, TokenStream2)> {
    let mut iter = ty.clone().to_token_iter();
    let parsed = ResultType::parse(&mut iter).ok()?;

    let last = parsed
        .path
        .rest
        .iter()
        .last()
        .map(|seg| seg.value.second.to_string())
        .unwrap_or_else(|| parsed.path.first.to_string());

    if last != "Result" {
        return None;
    }

    Some((parsed.ok.to_token_stream(), parsed.err.to_token_stream()))
}
