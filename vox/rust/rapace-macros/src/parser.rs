//! Trait parser using unsynn.
//!
//! Inspired by `rust-legacy/rapace-macros/src/parser.rs`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote_spanned;
use unsynn::operator::names::{Assign, Colon, Comma, Gt, Lt, PathSep, Pound, RArrow, Semicolon};
use unsynn::{
    keyword, unsynn, Any, BraceGroupContaining, BracketGroupContaining, CommaDelimitedVec, Cons,
    Either, EndOfStream, Except, Ident, LiteralString, Many, Optional,
    ParenthesisGroupContaining, Parse, ToTokenIter, ToTokens, TokenStream,
};

keyword! {
    pub KAsync = "async";
    pub KFn = "fn";
    pub KTrait = "trait";
    pub KSelfKw = "self";
    pub KMut = "mut";
    pub KDoc = "doc";
    pub KPub = "pub";
    pub KWhere = "where";
}

/// Parses tokens and groups until `C` is found, handling `<...>` correctly.
type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping.
    #[derive(Clone)]
    pub struct AngleTokenTree(
        pub Either<Cons<Lt, Vec<Cons<Except<Gt>, AngleTokenTree>>, Gt>, unsynn::TokenTree>,
    );

    pub struct RawAttribute {
        pub _pound: Pound,
        pub body: BracketGroupContaining<TokenStream>,
    }

    pub struct DocAttribute {
        pub _doc: KDoc,
        pub _assign: Assign,
        pub value: LiteralString,
    }

    pub enum Visibility {
        Pub(KPub),
        PubRestricted(Cons<KPub, ParenthesisGroupContaining<TokenStream>>),
    }

    pub struct RefSelf {
        pub _amp: unsynn::operator::names::And,
        pub mutability: Option<KMut>,
        pub name: KSelfKw,
    }

    pub struct MethodParam {
        pub name: Ident,
        pub _colon: Colon,
        pub ty: VerbatimUntil<Comma>,
    }

    pub struct GenericParams {
        pub _lt: Lt,
        pub params: VerbatimUntil<Gt>,
        pub _gt: Gt,
    }

    pub struct TypePath {
        pub leading: Option<PathSep>,
        pub first: Ident,
        pub rest: Any<Cons<PathSep, Ident>>,
    }

    pub struct ReturnType {
        pub _arrow: RArrow,
        pub ty: VerbatimUntil<Either<Semicolon, KWhere>>,
    }

    pub struct WhereClause {
        pub _where: KWhere,
        pub bounds: VerbatimUntil<Semicolon>,
    }

    pub struct MethodParams {
        pub receiver: RefSelf,
        pub rest: Optional<Cons<Comma, CommaDelimitedVec<MethodParam>>>,
    }

    pub struct ServiceMethod {
        pub attributes: Any<RawAttribute>,
        pub _async: KAsync,
        pub _fn: KFn,
        pub name: Ident,
        pub generics: Optional<GenericParams>,
        pub params: ParenthesisGroupContaining<MethodParams>,
        pub return_type: Optional<ReturnType>,
        pub where_clause: Optional<WhereClause>,
        pub _semi: Semicolon,
    }

    pub struct ServiceTrait {
        pub attributes: Any<RawAttribute>,
        pub vis: Optional<Visibility>,
        pub _trait: KTrait,
        pub name: Ident,
        pub generics: Optional<GenericParams>,
        pub body: BraceGroupContaining<Any<ServiceMethod>>,
        pub _eos: EndOfStream,
    }
}

pub struct ParsedTrait {
    pub name: String,
    pub doc: Option<String>,
    pub methods: Vec<ParsedMethod>,
}

pub struct ParsedMethod {
    pub name: String,
    pub doc: Option<String>,
    pub args: Vec<ParsedArg>,
    pub return_type: TokenStream2,
}

pub struct ParsedArg {
    pub name: String,
    pub ty: TokenStream2,
}

#[derive(Debug, Clone)]
pub struct Error {
    pub span: Span,
    pub message: String,
}

impl Error {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    pub fn to_compile_error(&self) -> TokenStream2 {
        let msg = &self.message;
        let span = self.span;
        quote_spanned! {span=> compile_error!(#msg); }
    }
}

impl From<unsynn::Error> for Error {
    fn from(err: unsynn::Error) -> Self {
        Self::new(Span::call_site(), err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn parse_trait(tokens: &TokenStream2) -> Result<ParsedTrait> {
    let mut iter = tokens.clone().to_token_iter();
    let parsed = ServiceTrait::parse(&mut iter).map_err(Error::from)?;

    if !parsed.generics.is_empty() {
        return Err(Error::new(
            parsed.name.span(),
            "service traits cannot declare generics yet",
        ));
    }

    let doc = collect_doc_string(parsed.attributes);

    let methods = parsed
        .body
        .content
        .into_iter()
        .map(|entry| lower_method(entry.value))
        .collect::<Result<Vec<_>>>()?;

    Ok(ParsedTrait {
        name: parsed.name.to_string(),
        doc,
        methods,
    })
}

fn lower_method(method: ServiceMethod) -> Result<ParsedMethod> {
    if !method.generics.is_empty() {
        return Err(Error::new(
            method.name.span(),
            "service methods cannot be generic yet",
        ));
    }

    if method.params.content.receiver.mutability.is_some() {
        return Err(Error::new(
            method.name.span(),
            "service methods must take &self, not &mut self",
        ));
    }

    let mut args = Vec::new();
    if let Some(rest) = method.params.content.rest.into_iter().next() {
        for entry in rest.value.second {
            let name = entry.value.name.to_string();
            let ty = entry.value.ty.to_token_stream();
            args.push(ParsedArg { name, ty });
        }
    }

    let return_type = method
        .return_type
        .into_iter()
        .next()
        .map(|r| r.value.ty.to_token_stream())
        .unwrap_or_else(|| quote::quote! { () });

    Ok(ParsedMethod {
        name: method.name.to_string(),
        doc: collect_doc_string(method.attributes),
        args,
        return_type,
    })
}

fn collect_doc_string(attrs: Any<RawAttribute>) -> Option<String> {
    let mut docs = Vec::new();

    for attr in attrs {
        let mut body_iter = attr.value.body.content.clone().to_token_iter();
        if let Ok(doc_attr) = DocAttribute::parse(&mut body_iter) {
            let line = doc_attr.value.as_str().replace("\\\"", "\"");
            docs.push(line);
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_trait() {
        let src = r#"
            pub trait Echo {
                async fn echo(&self, message: String) -> String;
            }
        "#;
        let ts: TokenStream2 = src.parse().expect("tokenize");
        let parsed = parse_trait(&ts).expect("parse_trait");

        assert_eq!(parsed.name, "Echo");
        assert_eq!(parsed.methods.len(), 1);

        let method = &parsed.methods[0];
        assert_eq!(method.name, "echo");
        assert_eq!(method.args.len(), 1);
        assert_eq!(method.args[0].name, "message");
        assert_eq!(method.args[0].ty.to_string(), "String");
        assert_eq!(method.return_type.to_string(), "String");
    }

    #[test]
    fn parse_no_return_type() {
        let src = r#"
            trait Ping {
                async fn ping(&self);
            }
        "#;
        let ts: TokenStream2 = src.parse().expect("tokenize");
        let parsed = parse_trait(&ts).expect("parse_trait");
        assert_eq!(parsed.methods[0].return_type.to_string(), "()");
    }

    #[test]
    fn parse_trait_with_doc() {
        let src = r#"
            #[doc = " A simple echo service"]
            pub trait Echo {
                #[doc = " Echoes the message back"]
                async fn echo(&self, message: String) -> String;
            }
        "#;
        let ts: TokenStream2 = src.parse().expect("tokenize");
        let parsed = parse_trait(&ts).expect("parse_trait");
        assert_eq!(parsed.doc.as_deref(), Some(" A simple echo service"));
        assert_eq!(
            parsed.methods[0].doc.as_deref(),
            Some(" Echoes the message back")
        );
    }

    #[test]
    fn parse_generic_arg_type_tokens() {
        let src = r#"
            trait Lists {
                async fn f(&self, a: Vec<Option<String>>) -> Vec<u8>;
            }
        "#;
        let ts: TokenStream2 = src.parse().expect("tokenize");
        let parsed = parse_trait(&ts).expect("parse_trait");
        assert_eq!(
            parsed.methods[0].args[0].ty.to_string().replace(' ', ""),
            "Vec<Option<String>>"
        );
    }
}
