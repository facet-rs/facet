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

    #[derive(Clone)]
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

/// Structured AST for RPC signature types
#[derive(Debug, Clone)]
pub enum Type {
    /// Simple path: String, u32, Error, std::string::String
    Path(TypePath),

    /// Generic with 1 param: Vec<T>, Option<T>, Stream<T>
    Generic1 {
        path: TypePath,
        arg: Box<Type>,
    },

    /// Generic with 2 params: Result<T, E>, Map<K, V>
    Generic2 {
        path: TypePath,
        arg1: Box<Type>,
        arg2: Box<Type>,
    },

    /// Tuple: (), (T,), (T1, T2, ...)
    Tuple(Vec<Type>),

    /// Reference: &T, &'a T, &mut T, &'a mut T
    Reference {
        lifetime: Option<String>,
        mutable: bool,
        inner: Box<Type>,
    },
}

impl Type {
    /// Extract Ok and Err types if this is Result<T, E>
    pub fn as_result(&self) -> Option<(&Type, &Type)> {
        match self {
            Type::Generic2 { path, arg1, arg2 }
                if path.last_segment().as_str() == "Result" => Some((arg1, arg2)),
            _ => None,
        }
    }

    /// Check if type contains a lifetime anywhere in the tree
    pub fn has_lifetime(&self) -> bool {
        match self {
            Type::Reference { lifetime: Some(_), .. } => true,
            Type::Reference { inner, .. } => inner.has_lifetime(),
            Type::Generic1 { arg, .. } => arg.has_lifetime(),
            Type::Generic2 { arg1, arg2, .. } => {
                arg1.has_lifetime() || arg2.has_lifetime()
            }
            Type::Tuple(elems) => elems.iter().any(|t| t.has_lifetime()),
            Type::Path(_) => false,
        }
    }

    /// Check if type contains Stream anywhere in the tree
    /// rs[impl streaming.error-no-streams] - detect Stream in type
    pub fn contains_stream(&self) -> bool {
        match self {
            Type::Path(path) => path.last_segment().as_str() == "Stream",
            Type::Generic1 { path, arg } => {
                path.last_segment().as_str() == "Stream" || arg.contains_stream()
            }
            Type::Generic2 { path, arg1, arg2 } => {
                path.last_segment().as_str() == "Stream"
                    || arg1.contains_stream()
                    || arg2.contains_stream()
            }
            Type::Tuple(elems) => elems.iter().any(|t| t.contains_stream()),
            Type::Reference { inner, .. } => inner.contains_stream(),
        }
    }

    /// Get a human-readable display of the type for error messages
    pub fn to_string(&self) -> String {
        self.to_tokens().to_string()
    }

    /// Convert Type back to TokenStream2 for codegen
    pub fn to_tokens(&self) -> TokenStream2 {
        match self {
            Type::Path(path) => path.to_token_stream(),
            Type::Generic1 { path, arg } => {
                let path_tokens = path.to_token_stream();
                let arg_tokens = arg.to_tokens();
                quote::quote! { #path_tokens<#arg_tokens> }
            }
            Type::Generic2 { path, arg1, arg2 } => {
                let path_tokens = path.to_token_stream();
                let arg1_tokens = arg1.to_tokens();
                let arg2_tokens = arg2.to_tokens();
                quote::quote! { #path_tokens<#arg1_tokens, #arg2_tokens> }
            }
            Type::Tuple(elems) => {
                let elem_tokens: Vec<_> = elems.iter().map(|t| t.to_tokens()).collect();
                quote::quote! { (#(#elem_tokens),*) }
            }
            Type::Reference { lifetime, mutable, inner } => {
                let inner_tokens = inner.to_tokens();
                let mut tokens = TokenStream2::new();
                tokens.extend(std::iter::once(proc_macro2::TokenTree::Punct(
                    proc_macro2::Punct::new('&', proc_macro2::Spacing::Alone)
                )));
                if let Some(l) = lifetime {
                    tokens.extend(std::iter::once(proc_macro2::TokenTree::Punct(
                        proc_macro2::Punct::new('\'', proc_macro2::Spacing::Joint)
                    )));
                    tokens.extend(std::iter::once(proc_macro2::TokenTree::Ident(
                        proc_macro2::Ident::new(l, proc_macro2::Span::call_site())
                    )));
                }
                if *mutable {
                    tokens.extend(quote::quote! { mut });
                }
                tokens.extend(inner_tokens);
                tokens
            }
        }
    }
}

impl TypePath {
    /// Get the last segment (e.g., "Result" from "std::result::Result")
    pub fn last_segment(&self) -> String {
        self.rest
            .iter()
            .last()
            .map(|seg| seg.value.second.to_string())
            .unwrap_or_else(|| self.first.to_string())
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
    pub return_type: Type,
}

pub struct ParsedArg {
    pub name: String,
    pub ty: Type,
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

/// Parse a type from tokens (from VerbatimUntil)
fn parse_type(tokens: &TokenStream2) -> Result<Type> {
    use proc_macro2::TokenTree;

    let mut iter = tokens.clone().into_iter().peekable();
    parse_type_from_iter(&mut iter)
}

fn parse_type_from_iter(iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>) -> Result<Type> {
    use proc_macro2::{TokenTree, Delimiter};

    // Check for reference: &, &mut, &'a, &'a mut
    if let Some(TokenTree::Punct(p)) = iter.peek() {
        if p.as_char() == '&' {
            iter.next(); // consume &

            // Check for lifetime: 'a
            let lifetime = if let Some(TokenTree::Punct(p)) = iter.peek() {
                if p.as_char() == '\'' {
                    iter.next(); // consume '
                    if let Some(TokenTree::Ident(ident)) = iter.next() {
                        Some(ident.to_string())
                    } else {
                        return Err(Error::new(Span::call_site(), "expected lifetime name after '"));
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Check for mut
            let mutable = if let Some(TokenTree::Ident(ident)) = iter.peek() {
                if ident.to_string() == "mut" {
                    iter.next(); // consume mut
                    true
                } else {
                    false
                }
            } else {
                false
            };

            // Parse inner type
            let inner = Box::new(parse_type_from_iter(iter)?);

            return Ok(Type::Reference {
                lifetime,
                mutable,
                inner,
            });
        }
    }

    // Check for tuple: (...)
    if let Some(TokenTree::Group(group)) = iter.peek() {
        if group.delimiter() == Delimiter::Parenthesis {
            let group = match iter.next() {
                Some(TokenTree::Group(g)) => g,
                _ => unreachable!(),
            };

            let inner_tokens = group.stream();
            if inner_tokens.is_empty() {
                // Unit type: ()
                return Ok(Type::Tuple(vec![]));
            }

            // Parse comma-separated types
            let mut elements = Vec::new();
            let mut current_tokens = TokenStream2::new();

            for tt in inner_tokens {
                if matches!(tt, TokenTree::Punct(ref p) if p.as_char() == ',') {
                    if !current_tokens.is_empty() {
                        elements.push(parse_type(&current_tokens)?);
                        current_tokens = TokenStream2::new();
                    }
                } else {
                    current_tokens.extend(std::iter::once(tt));
                }
            }

            if !current_tokens.is_empty() {
                elements.push(parse_type(&current_tokens)?);
            }

            return Ok(Type::Tuple(elements));
        }
    }

    // Parse path (potentially with generics): Foo, Foo::Bar, Foo<T>, Foo<T, E>
    let mut path_tokens = Vec::new();
    let mut generic_args: Option<Vec<Type>> = None;

    while let Some(tt) = iter.peek() {
        match tt {
            TokenTree::Ident(_) => {
                path_tokens.push(iter.next().unwrap());
            }
            TokenTree::Punct(p) if p.as_char() == ':' => {
                path_tokens.push(iter.next().unwrap());
                // Expect another : for ::
                if let Some(TokenTree::Punct(p)) = iter.peek() {
                    if p.as_char() == ':' {
                        path_tokens.push(iter.next().unwrap());
                    }
                }
            }
            TokenTree::Punct(p) if p.as_char() == '<' => {
                iter.next(); // consume <

                // Parse generic arguments
                let mut args = Vec::new();
                let mut current_tokens = TokenStream2::new();
                let mut angle_depth = 1;

                while let Some(tt) = iter.next() {
                    match &tt {
                        TokenTree::Punct(p) if p.as_char() == '<' => {
                            angle_depth += 1;
                            current_tokens.extend(std::iter::once(tt));
                        }
                        TokenTree::Punct(p) if p.as_char() == '>' => {
                            angle_depth -= 1;
                            if angle_depth == 0 {
                                if !current_tokens.is_empty() {
                                    args.push(parse_type(&current_tokens)?);
                                }
                                break;
                            } else {
                                current_tokens.extend(std::iter::once(tt));
                            }
                        }
                        TokenTree::Punct(p) if p.as_char() == ',' && angle_depth == 1 => {
                            if !current_tokens.is_empty() {
                                args.push(parse_type(&current_tokens)?);
                                current_tokens = TokenStream2::new();
                            }
                        }
                        _ => {
                            current_tokens.extend(std::iter::once(tt));
                        }
                    }
                }

                generic_args = Some(args);
                break;
            }
            _ => break,
        }
    }

    // Build TypePath from path_tokens
    let path_stream: TokenStream2 = path_tokens.into_iter().collect();
    let mut path_iter = path_stream.to_token_iter();
    let type_path = TypePath::parse(&mut path_iter)
        .map_err(|e| Error::new(Span::call_site(), format!("failed to parse type path: {}", e)))?;

    // Build Type based on generic args
    match generic_args {
        None => Ok(Type::Path(type_path)),
        Some(args) if args.len() == 1 => Ok(Type::Generic1 {
            path: type_path,
            arg: Box::new(args.into_iter().next().unwrap()),
        }),
        Some(args) if args.len() == 2 => {
            let mut iter = args.into_iter();
            Ok(Type::Generic2 {
                path: type_path,
                arg1: Box::new(iter.next().unwrap()),
                arg2: Box::new(iter.next().unwrap()),
            })
        }
        Some(args) => Err(Error::new(
            Span::call_site(),
            format!("generics with {} params not supported", args.len()),
        )),
    }
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
            let ty_tokens = entry.value.ty.to_token_stream();
            let ty = parse_type(&ty_tokens)?;
            args.push(ParsedArg { name, ty });
        }
    }

    let return_type = method
        .return_type
        .into_iter()
        .next()
        .map(|r| {
            let ty_tokens = r.value.ty.to_token_stream();
            parse_type(&ty_tokens)
        })
        .transpose()?
        .unwrap_or(Type::Tuple(vec![]));

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
