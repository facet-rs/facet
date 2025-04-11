#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod process_enum;
mod process_struct;

use unsynn::*;

keyword! {
    KPub = "pub";
    KStruct = "struct";
    KEnum = "enum";
    KDoc = "doc";
    KRepr = "repr";
    KCrate = "crate";
    KIn = "in";
    KConst = "const";
    KMut = "mut";
    KFacet = "facet";
    KSensitive = "sensitive";
}

operator! {
    Eq = "=";
    Semi = ";";
    Apostrophe = "'";
    DoubleSemicolon = "::";
}

/// Parses tokens and groups until `C` is found on the current token tree level.
type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;
type ModPath = Cons<Option<PathSep>, PathSepDelimited<Ident>>;

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping (which is not a [`Group`] as far as proc-macros
    /// are concerned).
    struct AngleTokenTree(Either<Cons<Lt, Vec<Cons<Except<Gt>, AngleTokenTree>>, Gt>, TokenTree>);

    enum TypeDecl {
        Struct(Struct),
        Enum(Enum),
    }

    enum Vis {
        Pub(KPub),
        /// `pub(in? crate::foo::bar)`/`pub(in? ::foo::bar)`
        PubIn(Cons<KPub, ParenthesisGroupContaining<Cons<Option<KIn>, ModPath>>>),
    }

    struct Attribute {
        _pound: Pound,
        body: BracketGroupContaining<AttributeInner>,
    }

    enum AttributeInner {
        Facet(FacetAttr),
        Doc(DocInner),
        Repr(ReprInner),
        Any(Vec<TokenTree>)
    }

    struct FacetAttr {
        _facet: KFacet,
        inner: ParenthesisGroupContaining<FacetInner>,
    }

    enum FacetInner {
        Sensitive(KSensitive),
        Other(Vec<TokenTree>)
    }

    struct DocInner {
        _kw_doc: KDoc,
        _eq: Eq,
        value: LiteralString,
    }

    struct ReprInner {
        _kw_repr: KRepr,
        attr: ParenthesisGroupContaining<Ident>,
    }

    struct Struct {
        attributes: Vec<Attribute>,
        _vis: Option<Vis>,
        _kw_struct: KStruct,
        name: Ident,
        kind: StructKind,
    }

    enum StructKind {
        Struct(BraceGroupContaining<CommaDelimitedVec<StructField>>),
        TupleStruct(Cons<ParenthesisGroupContaining<CommaDelimitedVec<TupleField>>, Semi>),
        UnitStruct(Semi)
    }

    struct Lifetime {
        _apostrophe: Apostrophe,
        name: Ident,
    }

    enum Expr {
        Integer(LiteralInteger),
    }

    enum ConstOrMut {
        Const(KConst),
        Mut(KMut),
    }

    struct StructField {
        attributes: Vec<Attribute>,
        _vis: Option<Vis>,
        name: Ident,
        _colon: Colon,
        typ: VerbatimUntil<Comma>,
    }

    struct TupleField {
        attributes: Vec<Attribute>,
        vis: Option<Vis>,
        typ: VerbatimUntil<Comma>,
    }

    struct Enum {
        attributes: Vec<Attribute>,
        _pub: Option<KPub>,
        _kw_enum: KEnum,
        name: Ident,
        body: BraceGroupContaining<CommaDelimitedVec<EnumVariantLike>>,
    }

    enum EnumVariantLike {
        Tuple(TupleVariant),
        Struct(StructVariant),
        Unit(UnitVariant),
    }

    struct UnitVariant {
        attributes: Vec<Attribute>,
        name: Ident,
    }

    struct TupleVariant {
        attributes: Vec<Attribute>,
        name: Ident,
        fields: ParenthesisGroupContaining<CommaDelimitedVec<TupleField>>,
    }

    struct StructVariant {
        attributes: Vec<Attribute>,
        name: Ident,
        fields: BraceGroupContaining<CommaDelimitedVec<StructField>>,
    }
}

/// Derive the Facet trait for structs, tuple structs, and enums.
///
/// This uses unsynn, so it's light, but it _will_ choke on some Rust syntax because...
/// there's a lot of Rust syntax.
#[proc_macro_derive(Facet, attributes(facet))]
pub fn facet_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = TokenStream::from(input);
    let mut i = input.to_token_iter();

    // Parse as TypeDecl
    match i.parse::<Cons<TypeDecl, EndOfStream>>() {
        Ok(it) => match it.first {
            TypeDecl::Struct(parsed) => process_struct::process_struct(parsed),
            TypeDecl::Enum(parsed) => process_enum::process_enum(parsed),
        },
        Err(err) => {
            panic!(
                "Could not parse type declaration: {}\nError: {}",
                input, err
            );
        }
    }
}

impl core::fmt::Display for AngleTokenTree {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.0 {
            Either::First(it) => {
                write!(f, "<")?;
                for it in it.second.iter() {
                    write!(f, "{}", it.second)?;
                }
                write!(f, ">")?;
            }
            Either::Second(it) => write!(f, "{}", it)?,
            Either::Third(Invalid) => unreachable!(),
            Either::Fourth(Invalid) => unreachable!(),
        };
        Ok(())
    }
}

struct VerbatimDisplay<'a, C>(&'a VerbatimUntil<C>);
impl<C> core::fmt::Display for VerbatimDisplay<'_, C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for tt in self.0.0.iter() {
            write!(f, "{}", tt.value.second)?;
        }
        Ok(())
    }
}

impl core::fmt::Display for ConstOrMut {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConstOrMut::Const(_) => write!(f, "const"),
            ConstOrMut::Mut(_) => write!(f, "mut"),
        }
    }
}

impl core::fmt::Display for Lifetime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "'{}", self.name)
    }
}

impl core::fmt::Display for Expr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Expr::Integer(int) => write!(f, "{}", int.value()),
        }
    }
}

/// Converts PascalCase to UPPER_SNAKE_CASE
pub(crate) fn to_upper_snake_case(input: &str) -> String {
    input
        .chars()
        .enumerate()
        .fold(String::new(), |mut acc, (i, c)| {
            if c.is_uppercase() {
                if i > 0 {
                    acc.push('_');
                }
                acc.push(c.to_ascii_uppercase());
            } else {
                acc.push(c.to_ascii_uppercase());
            }
            acc
        })
}

/// Generate a static declaration that exports the crate
pub(crate) fn generate_static_decl(type_name: &str) -> String {
    format!(
        "#[used]\nstatic {}_SHAPE: &'static facet::Shape = <{} as facet::Facet>::SHAPE;",
        to_upper_snake_case(type_name),
        type_name
    )
}

pub(crate) fn build_maybe_doc(attrs: &[Attribute]) -> String {
    let doc_lines: Vec<_> = attrs
        .iter()
        .filter_map(|attr| match &attr.body.content {
            AttributeInner::Doc(doc_inner) => Some(doc_inner.value.value()),
            _ => None,
        })
        .collect();

    if doc_lines.is_empty() {
        String::new()
    } else {
        format!(r#".doc(&[{}])"#, doc_lines.join(","))
    }
}

pub(crate) fn gen_struct_field(field_name: &str, struct_name: &str, attrs: &[Attribute]) -> String {
    // Determine field flags
    let mut flags = "facet::FieldFlags::EMPTY";
    let mut attribute_list: Vec<String> = vec![];
    let mut doc_lines: Vec<&str> = vec![];
    for attr in attrs {
        match &attr.body.content {
            AttributeInner::Facet(facet_attr) => match &facet_attr.inner.content {
                FacetInner::Sensitive(_ksensitive) => {
                    flags = "facet::FieldFlags::SENSITIVE";
                    attribute_list.push("facet::FieldAttribute::Sensitive".to_string());
                }
                FacetInner::Other(tt) => {
                    attribute_list.push(format!(
                        r#"facet::FieldAttribute::Arbitrary({:?})"#,
                        tt.tokens_to_string()
                    ));
                }
            },
            AttributeInner::Doc(doc_inner) => doc_lines.push(doc_inner.value.value()),
            AttributeInner::Repr(_) => {
                // muffin
            }
            AttributeInner::Any(_) => {
                // muffin two
            }
        }
    }
    let attributes = attribute_list.join(",");

    let maybe_field_doc = if doc_lines.is_empty() {
        String::new()
    } else {
        format!(r#".doc(&[{}])"#, doc_lines.join(","))
    };

    // Generate each field definition
    format!(
        "facet::Field::builder()
    .name(\"{field_name}\")
    .shape(facet::shape_of(&|s: {struct_name}| s.{field_name}))
    .offset(::core::mem::offset_of!({struct_name}, {field_name}))
    .flags({flags})
    .attributes(&[{attributes}])
    {maybe_field_doc}
    .build()"
    )
}
