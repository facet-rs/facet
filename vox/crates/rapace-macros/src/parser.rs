use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, quote_spanned};
use unsynn::operator::names::{And, Assign, Comma, Gt, Lt, Pound, RArrow, Semicolon};
use unsynn::{
    BraceGroupContaining, BracketGroupContaining, Colon, CommaDelimitedVec, Cons, Either,
    EndOfStream, Except, Ident, LiteralString, Many, ParenthesisGroupContaining, Parse, TokenIter,
    TokenStream, TokenTree,
};
use unsynn::{IParse, ToTokenIter, ToTokens, keyword, unsynn};

keyword! {
    pub KAsync = "async";
    pub KFn = "fn";
    pub KTrait = "trait";
    pub KSelfKw = "self";
    pub KMut = "mut";
    pub KDoc = "doc";
    pub KPub = "pub";
}

type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

unsynn! {
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
        pub _amp: And,
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
    pub vis_tokens: TokenStream2,
    pub ident: Ident,
    pub doc_lines: Vec<String>,
    pub methods: Vec<ParsedMethod>,
}

pub struct ParsedMethod {
    pub name: Ident,
    pub doc_lines: Vec<String>,
    pub args: Vec<MethodArg>,
    pub return_type: TokenStream2,
}

#[derive(Clone)]
pub struct MethodArg {
    pub name: Ident,
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
    let doc_lines = collect_doc_lines(&attributes);

    let vis_tokens = match Visibility::parse(&mut iter) {
        Ok(vis) => vis.to_token_stream(),
        Err(_) => TokenStream::new(),
    };

    KTrait::parse(&mut iter).map_err(Error::from)?;
    let ident = Ident::parse(&mut iter).map_err(Error::from)?;

    // Require the trait body to start immediately after the name.
    let body = BraceGroupContaining::<TokenStream>::parse(&mut iter).map_err(|err| {
        let next_span = iter.clone().next().map_or(ident.span(), |tt| tt.span());
        let message = if matches!(err.kind, unsynn::ErrorKind::UnexpectedToken) {
            "rapace::service traits cannot declare generics or supertraits yet"
        } else {
            "failed to parse service trait body"
        };
        Error::new(next_span, message)
    })?;

    EndOfStream::parse(&mut iter)
        .map_err(|_| Error::new(ident.span(), "unexpected tokens after trait body"))?;

    let methods = parse_methods(body.content)?;

    Ok(ParsedTrait {
        vis_tokens,
        ident,
        doc_lines,
        methods,
    })
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

fn collect_doc_lines(attrs: &[RawAttribute]) -> Vec<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        let mut body_iter = attr.body.content.clone().to_token_iter();
        if let Ok(doc_attr) = DocAttribute::parse(&mut body_iter) {
            let line = doc_attr.value.as_str().replace("\\\"", "\"");
            docs.push(line);
        }
    }
    docs
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
            .map_err(|_| Error::new(async_span, "rapace::service methods must be async"))?;

        KFn::parse(&mut iter).map_err(Error::from)?;
        let name = Ident::parse(&mut iter).map_err(Error::from)?;
        let name_span = name.span();

        if let Some(TokenTree::Punct(p)) = iter.clone().next()
            && p.as_char() == '<'
        {
            return Err(Error::new(
                name_span,
                "rapace::service methods cannot be generic yet",
            ));
        }

        let params_group =
            ParenthesisGroupContaining::<TokenStream>::parse(&mut iter).map_err(Error::from)?;
        let args = parse_method_params(params_group.content, name_span)?;

        let return_type = parse_return_type(&mut iter)?;

        Semicolon::parse(&mut iter).map_err(Error::from)?;

        let doc_lines = collect_doc_lines(&attrs);

        methods.push(ParsedMethod {
            name,
            doc_lines,
            args,
            return_type,
        });
    }

    Ok(methods)
}

fn parse_method_params(tokens: TokenStream, error_span: Span) -> Result<Vec<MethodArg>> {
    let mut iter = tokens.to_token_iter();

    RefSelf::parse(&mut iter)
        .map_err(|_| Error::new(error_span, "rapace::service methods must take &self"))?;

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
            .map(|entry| MethodArg {
                name: entry.value.name,
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
        return Ok(quote! { () });
    }

    RArrow::parse(iter).map_err(Error::from)?;
    let ty_tokens = VerbatimUntil::<Semicolon>::parse(iter).map_err(Error::from)?;
    Ok(ty_tokens.to_token_stream())
}

pub fn join_doc_lines(lines: &[String]) -> String {
    lines.join("\n")
}
