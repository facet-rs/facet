use proc_macro2::{Ident, TokenStream as TokenStream2, TokenTree};
use unsynn::*;

/// Parsed struct definition (named fields only).
pub(crate) struct StructItem {
    pub vis: TokenStream2,
    pub name: Ident,
    pub generics: TokenStream2,
    pub fields: Vec<StructField>,
    pub doc_attrs: Vec<TokenStream2>,
}

pub(crate) struct StructField {
    pub name: Ident,
    pub vis: TokenStream2,
    pub ty: TokenStream2,
    pub attrs: Vec<TokenStream2>,
    pub is_key: bool,
}

impl StructItem {
    pub(crate) fn parse(input: TokenStream2) -> std::result::Result<Self, String> {
        let mut it = input.to_token_iter();
        let decl: StructDecl = it
            .parse()
            .map_err(|e| format!("picante: failed to parse struct: {e:?}"))?;

        let vis = decl
            .vis
            .as_ref()
            .map(|v| v.to_token_stream())
            .unwrap_or_default();
        let generics = decl
            .generics
            .as_ref()
            .map(|g| g.to_token_stream())
            .unwrap_or_default();

        let mut doc_attrs = Vec::new();
        if let Some(many) = &decl.attributes {
            for attr in many.as_slice().iter() {
                let tokens = attr.to_token_stream();
                if attr_is_doc(&attr.value) {
                    doc_attrs.push(tokens);
                } else {
                    // Non-doc attributes are currently ignored.
                }
            }
        }

        let mut fields = Vec::new();
        for delim in decl.fields.content.into_iter() {
            let f = delim.value;

            let field_vis = f
                .vis
                .as_ref()
                .map(|v| v.to_token_stream())
                .unwrap_or_default();

            let mut attrs = Vec::new();
            let mut is_key = false;
            if let Some(many) = &f.attributes {
                for attr in many.as_slice().iter() {
                    if attr_is_key(&attr.value) {
                        is_key = true;
                        continue;
                    }
                    attrs.push(attr.to_token_stream());
                }
            }

            fields.push(StructField {
                name: f.name,
                vis: field_vis,
                ty: f.typ.to_token_stream(),
                attrs,
                is_key,
            });
        }

        Ok(Self {
            vis,
            name: decl.name,
            generics,
            fields,
            doc_attrs,
        })
    }
}

// ============================================================================
// Unsynn grammar
// ============================================================================

keyword! {
    pub KPub = "pub";
    pub KStruct = "struct";
}

/// Parses tokens until `C` is found at the current token tree level.
pub type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping (which is not a [`Group`] as far as proc-macros
    /// are concerned).
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

    pub struct StructDecl {
        pub attributes: Option<Many<Attribute>>,
        pub vis: Option<KPub>,
        pub _struct_kw: KStruct,
        pub name: Ident,
        pub generics: Option<GenericParams>,
        pub fields: BraceGroupContaining<CommaDelimitedVec<FieldDecl>>,
    }

    pub struct GenericParams {
        pub _lt: Lt,
        pub params: VerbatimUntil<Gt>,
        pub _gt: Gt,
    }

    pub struct FieldDecl {
        pub attributes: Option<Many<Attribute>>,
        pub vis: Option<KPub>,
        pub name: Ident,
        pub _colon: Colon,
        pub typ: VerbatimUntil<Comma>,
    }
}

fn attr_is_doc(attr: &Attribute) -> bool {
    let AttributeInner::Any(items) = &attr.body.content;
    matches!(items.first(), Some(TokenTree::Ident(id)) if id == "doc")
}

fn attr_is_key(attr: &Attribute) -> bool {
    let AttributeInner::Any(items) = &attr.body.content;
    matches!(items.as_slice(), [TokenTree::Ident(id)] if id == "key")
}
