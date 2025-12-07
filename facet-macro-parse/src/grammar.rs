//! Unsynn grammar definitions for parsing Rust structs and enums
//!
//! This module contains all the grammar rules for parsing struct and enum definitions
//! from token streams.

use proc_macro2::TokenTree;
use unsynn::*;

// ============================================================================
// KEYWORDS AND OPERATORS
// ============================================================================

keyword! {
    /// The "pub" keyword.
    pub KPub = "pub";
    /// The "struct" keyword.
    pub KStruct = "struct";
    /// The "enum" keyword.
    pub KEnum = "enum";
    /// The "doc" keyword.
    pub KDoc = "doc";
    /// The "repr" keyword.
    pub KRepr = "repr";
    /// The "crate" keyword.
    pub KCrate = "crate";
    /// The "in" keyword.
    pub KIn = "in";
    /// The "const" keyword.
    pub KConst = "const";
    /// The "where" keyword.
    pub KWhere = "where";
    /// The "mut" keyword.
    pub KMut = "mut";
    /// The "facet" keyword.
    pub KFacet = "facet";
}

operator! {
    /// Represents the '=' operator.
    pub Eq = "=";
    /// Represents the ';' operator.
    pub Semi = ";";
    /// Represents the apostrophe '\'' operator.
    pub Apostrophe = "'";
    /// Represents the double semicolon '::' operator.
    pub DoubleSemicolon = "::";
}

// ============================================================================
// HELPER TYPES
// ============================================================================

/// Parses tokens and groups until `C` is found on the current token tree level.
pub type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

/// Represents a module path, consisting of an optional path separator followed by
/// a path-separator-delimited sequence of identifiers.
pub type ModPath = Cons<Option<PathSep>, PathSepDelimited<Ident>>;

/// Represents type bounds, consisting of a colon followed by tokens until
/// a comma, equals sign, or closing angle bracket is encountered.
pub type Bounds = Cons<Colon, VerbatimUntil<Either<Comma, Eq, Gt>>>;

// ============================================================================
// GRAMMAR DEFINITIONS
// ============================================================================

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping (which is not a [`Group`] as far as proc-macros
    /// are concerned).
    #[derive(Clone)]
    pub struct AngleTokenTree(
        #[allow(clippy::type_complexity)]
        pub Either<Cons<Lt, Vec<Cons<Except<Gt>, AngleTokenTree>>, Gt>, TokenTree>,
    );

    /// Represents an algebraic data type (ADT) declaration, which can be either a struct or enum.
    pub enum AdtDecl {
        /// A struct ADT variant.
        Struct(Struct),
        /// An enum ADT variant.
        Enum(Enum),
    }

    /// Represents visibility modifiers for items.
    pub enum Vis {
        /// `pub(in? crate::foo::bar)`/`pub(in? ::foo::bar)`
        PubIn(Cons<KPub, ParenthesisGroupContaining<Cons<Option<KIn>, ModPath>>>),
        /// Public visibility, indicated by the "pub" keyword.
        Pub(KPub),
    }

    /// Represents an attribute annotation on a field, typically in the form `#[attr]`.
    pub struct Attribute {
        /// The pound sign preceding the attribute.
        pub _pound: Pound,
        /// The content of the attribute enclosed in square brackets.
        pub body: BracketGroupContaining<AttributeInner>,
    }

    /// Represents the inner content of an attribute annotation.
    pub enum AttributeInner {
        /// A facet attribute that can contain specialized metadata.
        Facet(FacetAttr),
        /// A documentation attribute typically used for generating documentation.
        Doc(DocInner),
        /// A representation attribute that specifies how data should be laid out.
        Repr(ReprInner),
        /// Any other attribute represented as a sequence of token trees.
        Any(Vec<TokenTree>),
    }

    /// Represents a facet attribute that can contain specialized metadata.
    pub struct FacetAttr {
        /// The keyword for the facet attribute.
        pub _facet: KFacet,
        /// The inner content of the facet attribute.
        pub inner: ParenthesisGroupContaining<CommaDelimitedVec<FacetInner>>,
    }

    /// Represents the inner content of a facet attribute.
    pub enum FacetInner {
        /// A namespaced attribute like `kdl::child` or `args::short = 'v'`
        Namespaced(NamespacedAttr),
        /// A non-namespaced (builtin) attribute like `sensitive` or `rename = "foo"`
        Simple(SimpleAttr),
    }

    /// A namespaced attribute like `kdl::child` or `args::short = 'v'`
    pub struct NamespacedAttr {
        /// The namespace (e.g., "kdl", "args")
        pub ns: Ident,
        /// The path separator ::
        pub _sep: PathSep,
        /// The key (e.g., "child", "short")
        pub key: Ident,
        /// Optional arguments
        pub args: Option<AttrArgs>,
    }

    /// A simple (builtin) attribute like `sensitive` or `rename = "foo"`
    pub struct SimpleAttr {
        /// The key (e.g., "sensitive", "rename")
        pub key: Ident,
        /// Optional arguments
        pub args: Option<AttrArgs>,
    }

    /// Arguments for attributes - either parenthesized `(args)` or with equals `= value`
    pub enum AttrArgs {
        /// Parenthesized arguments like `(auto_increment)`
        Parens(ParenthesisGroupContaining<Vec<TokenTree>>),
        /// Equals-style arguments like `= 'v'`
        Equals(AttrEqualsArgs),
    }

    /// Equals-style arguments like `= 'v'` or `= "value"`
    pub struct AttrEqualsArgs {
        /// The equals sign
        pub _eq: Eq,
        /// The value (tokens until comma or end)
        pub value: VerbatimUntil<Comma>,
    }

    /// Represents documentation for an item.
    pub struct DocInner {
        /// The "doc" keyword.
        pub _kw_doc: KDoc,
        /// The equality operator.
        pub _eq: Eq,
        /// The documentation content as a literal string.
        pub value: LiteralString,
    }

    /// Represents the inner content of a `repr` attribute.
    pub struct ReprInner {
        /// The "repr" keyword.
        pub _kw_repr: KRepr,
        /// The representation attributes enclosed in parentheses.
        pub attr: ParenthesisGroupContaining<CommaDelimitedVec<Ident>>,
    }

    /// Represents a struct definition.
    pub struct Struct {
        /// Attributes applied to the struct.
        pub attributes: Vec<Attribute>,
        /// The visibility modifier of the struct (e.g., `pub`).
        pub _vis: Option<Vis>,
        /// The "struct" keyword.
        pub _kw_struct: KStruct,
        /// The name of the struct.
        pub name: Ident,
        /// Generic parameters for the struct, if any.
        pub generics: Option<GenericParams>,
        /// The variant of struct (unit, tuple, or regular struct with named fields).
        pub kind: StructKind,
    }

    /// Represents the generic parameters of a struct or enum definition.
    pub struct GenericParams {
        /// The opening angle bracket `<`.
        pub _lt: Lt,
        /// The comma-delimited list of generic parameters.
        pub params: CommaDelimitedVec<GenericParam>,
        /// The closing angle bracket `>`.
        pub _gt: Gt,
    }

    /// Represents a single generic parameter.
    pub enum GenericParam {
        /// A lifetime parameter, e.g., `'a` or `'a: 'b + 'c`.
        Lifetime {
            /// The lifetime identifier.
            name: Lifetime,
            /// Optional lifetime bounds.
            bounds: Option<Cons<Colon, VerbatimUntil<Either<Comma, Gt>>>>,
        },
        /// A const generic parameter, e.g., `const N: usize = 10`.
        Const {
            /// The `const` keyword.
            _const: KConst,
            /// The name of the const parameter.
            name: Ident,
            /// The colon separating the name and type.
            _colon: Colon,
            /// The type of the const parameter.
            typ: VerbatimUntil<Either<Comma, Gt, Eq>>,
            /// An optional default value.
            default: Option<Cons<Eq, VerbatimUntil<Either<Comma, Gt>>>>,
        },
        /// A type parameter, e.g., `T: Trait = DefaultType`.
        Type {
            /// The name of the type parameter.
            name: Ident,
            /// Optional type bounds.
            bounds: Option<Bounds>,
            /// An optional default type.
            default: Option<Cons<Eq, VerbatimUntil<Either<Comma, Gt>>>>,
        },
    }

    /// Represents a `where` clause attached to a definition.
    #[derive(Clone)]
    pub struct WhereClauses {
        /// The `where` keyword.
        pub _kw_where: KWhere,
        /// The comma-delimited list of where clause predicates.
        pub clauses: CommaDelimitedVec<WhereClause>,
    }

    /// Represents a single predicate within a `where` clause.
    #[derive(Clone)]
    pub struct WhereClause {
        /// The type or lifetime being constrained.
        pub _pred: VerbatimUntil<Cons<Colon, Except<Colon>>>,
        /// The colon separating the constrained item and its bounds.
        pub _colon: Colon,
        /// The bounds applied to the type or lifetime.
        pub bounds: VerbatimUntil<Either<Comma, Semicolon, BraceGroup>>,
    }

    /// Represents the kind of a struct definition.
    pub enum StructKind {
        /// A regular struct with named fields.
        Struct {
            /// Optional where clauses.
            clauses: Option<WhereClauses>,
            /// The fields enclosed in braces.
            fields: BraceGroupContaining<CommaDelimitedVec<StructField>>,
        },
        /// A tuple struct.
        TupleStruct {
            /// The fields enclosed in parentheses.
            fields: ParenthesisGroupContaining<CommaDelimitedVec<TupleField>>,
            /// Optional where clauses.
            clauses: Option<WhereClauses>,
            /// The trailing semicolon.
            semi: Semi,
        },
        /// A unit struct.
        UnitStruct {
            /// Optional where clauses.
            clauses: Option<WhereClauses>,
            /// The trailing semicolon.
            semi: Semi,
        },
    }

    /// Represents a lifetime annotation, like `'a`.
    pub struct Lifetime {
        /// The apostrophe `'` starting the lifetime.
        pub _apostrophe: PunctJoint<'\''>,
        /// The identifier name of the lifetime.
        pub name: Ident,
    }

    /// Represents a simple expression.
    pub enum Expr {
        /// An integer literal expression.
        Integer(LiteralInteger),
    }

    /// Represents either the `const` or `mut` keyword.
    pub enum ConstOrMut {
        /// The `const` keyword.
        Const(KConst),
        /// The `mut` keyword.
        Mut(KMut),
    }

    /// Represents a field within a regular struct definition.
    pub struct StructField {
        /// Attributes applied to the field.
        pub attributes: Vec<Attribute>,
        /// Optional visibility modifier.
        pub _vis: Option<Vis>,
        /// The name of the field.
        pub name: Ident,
        /// The colon separating the name and type.
        pub _colon: Colon,
        /// The type of the field.
        pub typ: VerbatimUntil<Comma>,
    }

    /// Represents a field within a tuple struct definition.
    pub struct TupleField {
        /// Attributes applied to the field.
        pub attributes: Vec<Attribute>,
        /// Optional visibility modifier.
        pub vis: Option<Vis>,
        /// The type of the field.
        pub typ: VerbatimUntil<Comma>,
    }

    /// Represents an enum definition.
    pub struct Enum {
        /// Attributes applied to the enum.
        pub attributes: Vec<Attribute>,
        /// Optional visibility modifier.
        pub _vis: Option<Vis>,
        /// The `enum` keyword.
        pub _kw_enum: KEnum,
        /// The name of the enum.
        pub name: Ident,
        /// Optional generic parameters.
        pub generics: Option<GenericParams>,
        /// Optional where clauses.
        pub clauses: Option<WhereClauses>,
        /// The enum variants enclosed in braces.
        pub body: BraceGroupContaining<CommaDelimitedVec<EnumVariantLike>>,
    }

    /// Represents a variant of an enum, including the optional discriminant value.
    pub struct EnumVariantLike {
        /// The actual variant.
        pub variant: EnumVariantData,
        /// The optional discriminant value.
        pub discriminant: Option<Cons<Eq, VerbatimUntil<Comma>>>,
    }

    /// Represents the different kinds of variants an enum can have.
    pub enum EnumVariantData {
        /// A tuple-like variant.
        Tuple(TupleVariant),
        /// A struct-like variant.
        Struct(StructEnumVariant),
        /// A unit-like variant.
        Unit(UnitVariant),
    }

    /// Represents a unit-like enum variant.
    pub struct UnitVariant {
        /// Attributes applied to the variant.
        pub attributes: Vec<Attribute>,
        /// The name of the variant.
        pub name: Ident,
    }

    /// Represents a tuple-like enum variant.
    pub struct TupleVariant {
        /// Attributes applied to the variant.
        pub attributes: Vec<Attribute>,
        /// The name of the variant.
        pub name: Ident,
        /// The fields enclosed in parentheses.
        pub fields: ParenthesisGroupContaining<CommaDelimitedVec<TupleField>>,
    }

    /// Represents a struct-like enum variant.
    pub struct StructEnumVariant {
        /// Attributes applied to the variant.
        pub attributes: Vec<Attribute>,
        /// The name of the variant.
        pub name: Ident,
        /// The fields enclosed in braces.
        pub fields: BraceGroupContaining<CommaDelimitedVec<StructField>>,
    }

    /// A lifetime or a tokentree.
    pub enum LifetimeOrTt {
        /// A lifetime annotation.
        Lifetime(Lifetime),
        /// A single token tree.
        TokenTree(TokenTree),
    }
}

// ============================================================================
// DISPLAY IMPLEMENTATIONS
// ============================================================================

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
            Either::Second(it) => write!(f, "{it}")?,
            Either::Third(Invalid) => unreachable!(),
            Either::Fourth(Invalid) => unreachable!(),
        };
        Ok(())
    }
}

/// Display the verbatim tokens until the given token.
pub struct VerbatimDisplay<'a, C>(pub &'a VerbatimUntil<C>);

impl<C> core::fmt::Display for VerbatimDisplay<'_, C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for tt in self.0.iter() {
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

impl core::fmt::Display for WhereClauses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "where ")?;
        for clause in self.clauses.iter() {
            write!(f, "{},", clause.value)?;
        }
        Ok(())
    }
}

impl core::fmt::Display for WhereClause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}",
            VerbatimDisplay(&self._pred),
            VerbatimDisplay(&self.bounds)
        )
    }
}

impl core::fmt::Display for Expr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Expr::Integer(int) => write!(f, "{}", int.value()),
        }
    }
}

impl Struct {
    /// Returns an iterator over the `FacetInner` content of `#[facet(...)]` attributes.
    pub fn facet_attributes(&self) -> impl Iterator<Item = &FacetInner> {
        self.attributes
            .iter()
            .filter_map(|attr| match &attr.body.content {
                AttributeInner::Facet(f) => Some(f.inner.content.as_slice()),
                _ => None,
            })
            .flatten()
            .map(|d| &d.value)
    }
}
