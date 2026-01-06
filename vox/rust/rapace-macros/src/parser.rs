//! Trait parser using unsynn.
//!
//! Inspired by `rust-legacy/rapace-macros/src/parser.rs`.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote_spanned;
use unsynn::operator::names::{Assign, Comma, Gt, Lt, Pound, RArrow, Semicolon};
use unsynn::{
    keyword, unsynn, BraceGroupContaining, BracketGroupContaining, Colon, CommaDelimitedVec,
    Cons, Either, EndOfStream, Except, IParse, Ident, LiteralString, Many,
    ParenthesisGroupContaining, Parse, ToTokenIter, ToTokens, TokenIter, TokenStream, TokenTree,
};

keyword! {
    pub KAsync = "async";
    pub KFn = "fn";
    pub KTrait = "trait";
    pub KSelfKw = "self";
    pub KMut = "mut";
    pub KDoc = "doc";
    pub KPub = "pub";
}

/// Parses tokens and groups until `C` is found, handling `<...>` correctly.
type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping.
    #[derive(Clone)]
    pub struct AngleTokenTree(
        pub Either<Cons<Lt, Vec<Cons<Except<Gt>, AngleTokenTree>>, Gt>, TokenTree>,
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

    let attributes = parse_attributes(&mut iter)?;
    let doc = collect_doc_string(&attributes);

    // Skip visibility
    let _ = Visibility::parse(&mut iter);

    KTrait::parse(&mut iter).map_err(Error::from)?;
    let ident = Ident::parse(&mut iter).map_err(Error::from)?;
    let name = ident.to_string();

    let body = BraceGroupContaining::<TokenStream>::parse(&mut iter).map_err(|err| {
        let next_span = iter.clone().next().map_or(ident.span(), |tt| tt.span());
        let message = if matches!(err.kind, unsynn::ErrorKind::UnexpectedToken) {
            "service traits cannot declare generics or supertraits yet"
        } else {
            "failed to parse service trait body"
        };
        Error::new(next_span, message)
    })?;

    EndOfStream::parse(&mut iter)
        .map_err(|_| Error::new(ident.span(), "unexpected tokens after trait body"))?;

    let methods = parse_methods(body.content)?;

    Ok(ParsedTrait { name, doc, methods })
}

fn parse_attributes(iter: &mut TokenIter) -> Result<Vec<RawAttribute>> {
    let mut attrs = Vec::new();
    loop {
        let mut lookahead = iter.clone();
        if lookahead.parse::<Pound>().is_err() {
            break;
        }
        let attr = RawAttribute::parse(iter).map_err(Error::from)?;
        attrs.push(attr);
    }
    Ok(attrs)
}

fn collect_doc_string(attrs: &[RawAttribute]) -> Option<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        let mut body_iter = attr.body.content.clone().to_token_iter();
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

fn parse_methods(body: TokenStream2) -> Result<Vec<ParsedMethod>> {
    let mut iter = body.to_token_iter();
    let mut methods = Vec::new();

    loop {
        let mut lookahead = iter.clone();
        if lookahead.next().is_none() {
            break;
        }

        let attrs = parse_attributes(&mut iter)?;

        let async_span = iter
            .clone()
            .next()
            .map_or(Span::call_site(), |tt| tt.span());
        KAsync::parse(&mut iter)
            .map_err(|_| Error::new(async_span, "service methods must be async"))?;

        KFn::parse(&mut iter).map_err(Error::from)?;
        let ident = Ident::parse(&mut iter).map_err(Error::from)?;
        let name = ident.to_string();
        let name_span = ident.span();

        if let Some(TokenTree::Punct(p)) = iter.clone().next() {
            if p.as_char() == '<' {
                return Err(Error::new(
                    name_span,
                    "service methods cannot be generic yet",
                ));
            }
        }

        let params_group =
            ParenthesisGroupContaining::<TokenStream>::parse(&mut iter).map_err(Error::from)?;
        let args = parse_method_params(params_group.content, name_span)?;

        let return_type = parse_return_type(&mut iter)?;

        // Skip optional where clause
        if let Some(TokenTree::Ident(ident)) = iter.clone().next() {
            if ident == "where" {
                while let Some(peek) = iter.clone().next() {
                    if matches!(&peek, TokenTree::Punct(p) if p.as_char() == ';') {
                        break;
                    }
                    iter.next();
                }
            }
        }

        Semicolon::parse(&mut iter).map_err(Error::from)?;

        let doc = collect_doc_string(&attrs);

        methods.push(ParsedMethod {
            name,
            doc,
            args,
            return_type,
        });
    }

    Ok(methods)
}

fn parse_method_params(tokens: TokenStream, error_span: Span) -> Result<Vec<ParsedArg>> {
    let mut iter = tokens.to_token_iter();

    let ref_self = RefSelf::parse(&mut iter)
        .map_err(|_| Error::new(error_span, "service methods must take &self"))?;

    if ref_self.mutability.is_some() {
        return Err(Error::new(
            error_span,
            "service methods must take &self, not &mut self",
        ));
    }

    // Optional comma after &self
    let mut lookahead = iter.clone();
    if lookahead.parse::<Comma>().is_ok() {
        iter.parse::<Comma>().map_err(Error::from)?;
    }

    let args = if iter.clone().next().is_none() {
        Vec::new()
    } else {
        let parsed = iter
            .parse::<CommaDelimitedVec<MethodParam>>()
            .map_err(Error::from)?;
        parsed
            .into_iter()
            .map(|entry| ParsedArg {
                name: entry.value.name.to_string(),
                ty: entry.value.ty.to_token_stream(),
            })
            .collect()
    };

    EndOfStream::parse(&mut iter)
        .map_err(|_| Error::new(error_span, "failed to parse method parameters"))?;

    Ok(args)
}

fn parse_return_type(iter: &mut TokenIter) -> Result<TokenStream2> {
    let mut lookahead = iter.clone();
    if lookahead.parse::<RArrow>().is_err() {
        return Ok(quote::quote! { () });
    }

    RArrow::parse(iter).map_err(Error::from)?;

    let mut ty_tokens = TokenStream2::new();
    loop {
        let next = iter.clone().next();
        match next {
            Some(TokenTree::Punct(p)) if p.as_char() == ';' => break,
            Some(TokenTree::Ident(ident)) if ident == "where" => break,
            Some(_) => {
                let tt = iter.next().expect("we just saw a next token");
                ty_tokens.extend(std::iter::once(tt));
            }
            None => break,
        }
    }

    Ok(ty_tokens)
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
        assert!(method.args[0].ty.to_string().contains("String"));
        assert!(method.return_type.to_string().contains("String"));
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

        assert_eq!(parsed.doc, Some(" A simple echo service".to_string()));
        assert_eq!(
            parsed.methods[0].doc,
            Some(" Echoes the message back".to_string())
        );
    }

    #[test]
    fn parse_no_return_type() {
        let src = r#"
            pub trait T {
                async fn foo(&self);
            }
        "#;
        let ts: TokenStream2 = src.parse().expect("tokenize");
        let parsed = parse_trait(&ts).expect("parse_trait");
        assert_eq!(parsed.methods[0].return_type.to_string(), "()");
    }

    #[test]
    fn parse_generic_arg_type_tokens() {
        let src = r#"
            pub trait T {
                async fn foo(&self, data: Vec<u8>) -> Vec<u8>;
            }
        "#;
        let ts: TokenStream2 = src.parse().expect("tokenize");
        let parsed = parse_trait(&ts).expect("parse_trait");

        let method = &parsed.methods[0];
        assert_eq!(method.args.len(), 1);
        assert_eq!(
            method.args[0].ty.to_string().replace(' ', ""),
            "Vec<u8>"
        );
        assert_eq!(method.return_type.to_string().replace(' ', ""), "Vec<u8>");
    }
}
