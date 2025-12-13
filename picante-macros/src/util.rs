use heck::{ToSnakeCase, ToUpperCamelCase};
use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote};
use unsynn::*;

/// Parses tokens until `C` is found at the current token tree level.
///
/// This is like a "verbatim until delimiter" parser, but it treats `<...>` as a nested token tree
/// (since proc-macros don't expose angle brackets as groups).
pub type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

pub(crate) fn compile_error(message: &str) -> TokenStream {
    quote! { compile_error!(#message); }.into()
}

/// Convert a CamelCase identifier to snake_case (good enough for generated API names).
pub(crate) fn snake_ident(name: &Ident) -> Ident {
    Ident::new(&name.to_string().to_snake_case(), name.span())
}

pub(crate) fn pascal_ident(name: &Ident) -> Ident {
    Ident::new(&name.to_string().to_upper_camel_case(), name.span())
}

pub(crate) fn fn_kind_id_ident(name: &Ident) -> Ident {
    format_ident!("{}_KIND", name.to_string().to_uppercase())
}

pub(crate) struct Param {
    pub name: Ident,
    pub param_type: TokenStream2,
}

pub(crate) struct FnItem {
    pub vis: TokenStream2,
    pub is_async: bool,
    pub name: Ident,
    pub generics: TokenStream2,
    pub params: Vec<Param>,
    pub return_type: TokenStream2,
    pub body: TokenStream2,
    pub doc_attrs: Vec<TokenStream2>,
    pub other_attrs: Vec<TokenStream2>,
}

impl FnItem {
    pub fn parse(input: TokenStream2) -> std::result::Result<Self, String> {
        let mut it = input.to_token_iter();
        let sig: FnSignature = it
            .parse()
            .map_err(|e| format!("picante: failed to parse function: {e:?}"))?;

        let vis = sig
            .vis
            .as_ref()
            .map(|v| v.to_token_stream())
            .unwrap_or_default();
        let is_async = sig.async_kw.is_some();

        let generics = sig
            .generics
            .as_ref()
            .map(|g| g.to_token_stream())
            .unwrap_or_default();

        let params_ts = extract_parenthesis_content(sig.params.to_token_stream());
        let params = parse_fn_parameters(params_ts);

        let return_type = sig
            .return_type
            .as_ref()
            .map(|rt| rt.return_type.to_token_stream())
            .unwrap_or_else(|| quote! { () });

        let body = sig.body.to_token_stream();

        let mut doc_attrs = Vec::new();
        let mut other_attrs = Vec::new();
        if let Some(many) = &sig.attributes {
            for attr in many.as_slice().iter() {
                let tokens = attr.to_token_stream();
                if attr_is_doc(&attr.value) {
                    doc_attrs.push(tokens);
                } else {
                    other_attrs.push(tokens);
                }
            }
        }

        Ok(Self {
            vis,
            is_async,
            name: sig.name,
            generics,
            params,
            return_type,
            body,
            doc_attrs,
            other_attrs,
        })
    }
}

// ============================================================================
// Unsynn grammar (minimal)
// ============================================================================

keyword! {
    pub KPub = "pub";
    pub KAsync = "async";
    pub KFn = "fn";
}

unsynn! {
    pub struct FnSignature {
        pub attributes: Option<Many<Attribute>>,
        pub vis: Option<KPub>,
        pub async_kw: Option<KAsync>,
        pub _fn_kw: KFn,
        pub name: Ident,
        pub generics: Option<GenericParams>,
        pub params: ParenthesisGroup,
        pub return_type: Option<ReturnType>,
        pub body: BraceGroup,
    }

    pub struct GenericParams {
        pub _lt: Lt,
        pub params: VerbatimUntil<Gt>,
        pub _gt: Gt,
    }

    pub struct ReturnType {
        pub _arrow: RArrow,
        pub return_type: VerbatimUntil<BraceGroup>,
    }

    /// Parses either a `TokenTree` or `<...>` grouping (which is not a [`Group`] as far as
    /// proc-macros are concerned).
    #[derive(Clone)]
    pub struct AngleTokenTree(
        #[allow(clippy::type_complexity)]
        pub Either<Cons<Lt, Vec<Cons<Except<Gt>, AngleTokenTree>>, Gt>, TokenTree>,
    );

    pub struct Attribute {
        pub _pound: Pound,
        pub body: BracketGroupContaining<AttributeInner>,
    }

    pub enum AttributeInner {
        Any(Vec<TokenTree>),
    }

    pub struct Parameter {
        pub name: Ident,
        pub _colon: Colon,
        pub param_type: VerbatimUntil<Comma>,
    }
}

fn attr_is_doc(attr: &Attribute) -> bool {
    let AttributeInner::Any(items) = &attr.body.content;
    matches!(items.first(), Some(TokenTree::Ident(_)))
        && matches!(items.first(), Some(TokenTree::Ident(id)) if id == "doc")
}

fn extract_parenthesis_content(ts: TokenStream2) -> TokenStream2 {
    let mut it = ts.to_token_iter();
    match it.parse::<TokenTree>() {
        Ok(TokenTree::Group(group)) => group.stream(),
        _ => TokenStream2::new(),
    }
}

fn parse_fn_parameters(params_ts: TokenStream2) -> Vec<Param> {
    let mut it = params_ts.to_token_iter();
    match it.parse::<CommaDelimitedVec<Parameter>>() {
        Ok(params) => params
            .into_iter()
            .map(|d| Param {
                name: d.value.name,
                param_type: d.value.param_type.to_token_stream(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn parse_db_generic(
    generics: &TokenStream2,
) -> std::result::Result<(Ident, TokenStream2), String> {
    if generics.is_empty() {
        return Err(
            "picante: #[picante::tracked] requires a generic DB param, e.g. `fn foo<DB: ...>(db: &DB, ...)`"
                .to_string(),
        );
    }

    let trees: Vec<TokenTree> = generics.clone().into_iter().collect();
    let Some(TokenTree::Punct(lt)) = trees.first() else {
        return Err("picante: failed to parse generics".to_string());
    };
    if lt.as_char() != '<' {
        return Err("picante: failed to parse generics".to_string());
    }
    let Some(TokenTree::Punct(gt)) = trees.last() else {
        return Err("picante: failed to parse generics".to_string());
    };
    if gt.as_char() != '>' {
        return Err("picante: failed to parse generics".to_string());
    }

    // Slice without the surrounding <...>.
    let inner = &trees[1..trees.len().saturating_sub(1)];

    // Reject multiple generic params for now.
    let mut depth_angles = 0u32;
    for tt in inner.iter() {
        match tt {
            TokenTree::Punct(p) if p.as_char() == '<' => depth_angles += 1,
            TokenTree::Punct(p) if p.as_char() == '>' => {
                depth_angles = depth_angles.saturating_sub(1)
            }
            TokenTree::Punct(p) if p.as_char() == ',' && depth_angles == 0 => {
                return Err("picante: #[picante::tracked] currently supports exactly one type parameter (DB)".to_string());
            }
            _ => {}
        }
    }

    let mut it = inner.iter().cloned().peekable();

    let Some(first) = it.next() else {
        return Err("picante: expected `<DB: ...>`".to_string());
    };
    if matches!(&first, TokenTree::Punct(p) if p.as_char() == '\'') {
        return Err(
            "picante: lifetime generic params are not supported (use a type param `DB`)"
                .to_string(),
        );
    }
    let TokenTree::Ident(db_ident) = first else {
        return Err("picante: expected `<DB: ...>`".to_string());
    };

    // Optional bounds after `:`.
    let mut bounds = TokenStream2::new();
    let mut saw_colon = false;
    let mut depth_angles = 0u32;
    for tt in it {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == '<' => depth_angles += 1,
            TokenTree::Punct(p) if p.as_char() == '>' => {
                depth_angles = depth_angles.saturating_sub(1)
            }
            TokenTree::Punct(p) if p.as_char() == ':' && depth_angles == 0 && !saw_colon => {
                saw_colon = true;
                continue;
            }
            _ => {}
        }

        if !saw_colon {
            return Err("picante: expected `<DB: ...>`".to_string());
        }
        bounds.extend(::core::iter::once(tt));
    }

    Ok((db_ident, bounds))
}

pub(crate) fn strip_ref_inner_ident(ty: &TokenStream2) -> Option<Ident> {
    let mut it = ty.clone().into_iter().peekable();

    match it.next()? {
        TokenTree::Punct(p) if p.as_char() == '&' => {}
        _ => return None,
    }

    // Optional lifetime: &'db T
    if let Some(TokenTree::Punct(p)) = it.peek()
        && p.as_char() == '\''
    {
        it.next();
        let _lifetime_name = it.next()?;
    }

    // Reject &mut.
    if let Some(TokenTree::Ident(id)) = it.peek()
        && id == "mut"
    {
        return None;
    }

    let Some(TokenTree::Ident(inner)) = it.next() else {
        return None;
    };

    if it.next().is_some() {
        return None;
    }

    Some(inner)
}

/// Return (ok_type, returns_picante_result).
pub(crate) fn take_return_ok_type(
    ret: &TokenStream2,
) -> std::result::Result<(TokenStream2, bool), String> {
    let (head, generic_args) = split_outer_type(ret.clone());
    let Some(generic_args) = generic_args else {
        return Ok((ret.clone(), false));
    };

    let Some(last_ident) = last_ident(&head) else {
        return Ok((ret.clone(), false));
    };

    if last_ident != "PicanteResult" {
        return Ok((ret.clone(), false));
    }

    let ok = first_generic_arg(&generic_args)?;
    Ok((ok, true))
}

fn split_outer_type(ret: TokenStream2) -> (TokenStream2, Option<Vec<TokenTree>>) {
    let mut head = TokenStream2::new();
    let mut args = Vec::new();
    let mut depth = 0u32;
    let mut in_args = false;

    for tt in ret {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == '<' => {
                depth += 1;
                if depth == 1 {
                    in_args = true;
                    continue;
                }
            }
            TokenTree::Punct(p) if p.as_char() == '>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && in_args {
                    in_args = false;
                    continue;
                }
            }
            _ => {}
        }

        if in_args {
            args.push(tt);
        } else {
            head.extend(::core::iter::once(tt));
        }
    }

    if args.is_empty() {
        (head, None)
    } else {
        (head, Some(args))
    }
}

fn last_ident(ts: &TokenStream2) -> Option<String> {
    ts.clone()
        .into_iter()
        .filter_map(|tt| match tt {
            TokenTree::Ident(id) => Some(id.to_string()),
            _ => None,
        })
        .last()
}

fn first_generic_arg(tokens: &[TokenTree]) -> std::result::Result<TokenStream2, String> {
    let mut out = TokenStream2::new();
    let mut depth_angles = 0u32;

    for tt in tokens {
        match tt {
            TokenTree::Punct(p) if p.as_char() == '<' => depth_angles += 1,
            TokenTree::Punct(p) if p.as_char() == '>' => {
                depth_angles = depth_angles.saturating_sub(1)
            }
            TokenTree::Punct(p) if p.as_char() == ',' && depth_angles == 0 => break,
            _ => {}
        }
        out.extend(::core::iter::once(tt.clone()));
    }

    if out.is_empty() {
        return Err("picante: expected generic argument".to_string());
    }
    Ok(out)
}
