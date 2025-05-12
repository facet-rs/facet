#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub use unsynn::*;

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
    /// The "sensitive" keyword.
    pub KSensitive = "sensitive";
    /// The "invariants" keyword.
    pub KInvariants = "invariants";
    /// The "opaque" keyword.
    pub KOpaque = "opaque";
    /// The "deny_unknown_fields" keyword.
    pub KDenyUnknownFields = "deny_unknown_fields";
    /// The "default" keyword.
    pub KDefault = "default";
    /// The "transparent" keyword.
    pub KTransparent = "transparent";
    /// The "rename" keyword.
    pub KRename = "rename";
    /// The "rename_all" keyword.
    pub KRenameAll = "rename_all";
    /// The "flatten" keyword
    pub KFlatten = "flatten";
    /// The "child" keyword
    pub KChild = "child";
    /// The "skip_serializing" keyword.
    pub KSkipSerializing = "skip_serializing";
    /// The "skip_serializing_if" keyword.
    pub KSkipSerializingIf = "skip_serializing_if";
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

/// Parses tokens and groups until `C` is found on the current token tree level.
pub type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

/// Represents a module path, consisting of an optional path separator followed by
/// a path-separator-delimited sequence of identifiers.
pub type ModPath = Cons<Option<PathSep>, PathSepDelimited<Ident>>;

/// Represents type bounds, consisting of a colon followed by tokens until
/// a comma, equals sign, or closing angle bracket is encountered.
pub type Bounds = Cons<Colon, VerbatimUntil<Either<Comma, Eq, Gt>>>;

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping (which is not a [`Group`] as far as proc-macros
    /// are concerned).
    #[derive(Clone)]
    pub struct AngleTokenTree(
        #[allow(clippy::type_complexity)] // look,
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
        /// A sensitive attribute that specifies sensitivity information.
        Sensitive(KSensitive),
        /// An invariants attribute that specifies invariants for the type.
        Invariants(InvariantInner),
        /// An opaque attribute that specifies opaque information.
        Opaque(KOpaque),
        /// A deny_unknown_fields attribute that specifies whether unknown fields are allowed.
        DenyUnknownFields(KDenyUnknownFields),
        /// A default attribute with an explicit value (#[facet(default = "myfunc")])
        DefaultEquals(DefaultEqualsInner),
        /// A default attribute with no explicit value (#[facet(default)])
        Default(KDefault),
        /// A transparent attribute for containers
        Transparent(KTransparent),
        /// A rename_all attribute that specifies a case conversion for all fields/variants (#[facet(rename_all = "camelCase")])
        RenameAll(RenameAllInner),
        /// A rename attribute that specifies a custom name for a field/variant (#[facet(rename = "custom_name")])
        Rename(RenameInner),
        /// A flatten attribute that marks a field to be flattened into the parent structure
        Flatten(FlattenInner),
        /// A child attribute that marks a field as a child node
        Child(ChildInner),
        /// A skip_serializing attribute that specifies whether a field should be skipped during serialization.
        SkipSerializing(SkipSerializingInner),
        /// A skip_serializing_if attribute that specifies a condition for skipping serialization.
        SkipSerializingIf(SkipSerializingIfInner),
        /// Any other attribute represented as a sequence of token trees.
        Arbitrary(VerbatimUntil<Comma>),
    }

    /// Inner value for #[facet(flatten)]
    pub struct FlattenInner {
        /// The "flatten" keyword.
        pub _kw_flatten: KFlatten,
    }

    /// Inner value for #[facet(child)]
    pub struct ChildInner {
        /// The "child" keyword.
        pub _kw_child: KChild,
    }

    /// Inner value for #[facet(skip_serializing)]
    pub struct SkipSerializingInner {
        /// The "skip_serializing" keyword.
        pub _kw_skip_serializing: KSkipSerializing,
    }

    /// Inner value for #[facet(skip_serializing_if = ...)]
    pub struct SkipSerializingIfInner {
        /// The "skip_serializing_if" keyword.
        pub _kw_skip_serializing_if: KSkipSerializingIf,
        /// The equals sign '='.
        pub _eq: Eq,
        /// The conditional expression as verbatim until comma.
        pub expr: VerbatimUntil<Comma>,
    }

    /// Inner value for #[facet(default = ...)]
    pub struct DefaultEqualsInner {
        /// The "default" keyword.
        pub _kw_default: KDefault,
        /// The equals sign '='.
        pub _eq: Eq,
        /// The value assigned, as a literal string.
        pub expr: VerbatimUntil<Comma>,
    }

    /// Inner value for #[facet(rename = ...)]
    pub struct RenameInner {
        /// The "rename" keyword.
        pub _kw_rename: KRename,
        /// The equals sign '='.
        pub _eq: Eq,
        /// The value assigned, as a literal string.
        pub value: LiteralString,
    }

    /// Inner value for #[facet(rename_all = ...)]
    pub struct RenameAllInner {
        /// The "rename_all" keyword.
        pub _kw_rename_all: KRenameAll,
        /// The equals sign '='.
        pub _eq: Eq,
        /// The value assigned, as a literal string.
        pub value: LiteralString,
    }

    /// Represents invariants for a type.
    pub struct InvariantInner {
        /// The "invariants" keyword.
        pub _kw_invariants: KInvariants,
        /// The equality operator.
        pub _eq: Eq,
        /// The invariant value
        pub expr: VerbatimUntil<Comma>,
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

    /// Represents the inner content of a `repr` attribute, typically used for specifying
    /// memory layout or representation hints.
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

    /// Represents the generic parameters of a struct or enum definition, enclosed in angle brackets.
    /// e.g., `<'a, T: Trait, const N: usize>`.
    pub struct GenericParams {
        /// The opening angle bracket `<`.
        pub _lt: Lt,
        /// The comma-delimited list of generic parameters.
        pub params: CommaDelimitedVec<GenericParam>,
        /// The closing angle bracket `>`.
        pub _gt: Gt,
    }

    /// Represents a single generic parameter within a `GenericParams` list.
    pub enum GenericParam {
        /// A lifetime parameter, e.g., `'a` or `'a: 'b + 'c`.
        Lifetime {
            /// The lifetime identifier (e.g., `'a`).
            name: Lifetime,
            /// Optional lifetime bounds (e.g., `: 'b + 'c`).
            bounds: Option<Cons<Colon, VerbatimUntil<Either<Comma, Gt>>>>,
        },
        /// A const generic parameter, e.g., `const N: usize = 10`.
        Const {
            /// The `const` keyword.
            _const: KConst,
            /// The name of the const parameter (e.g., `N`).
            name: Ident,
            /// The colon separating the name and type.
            _colon: Colon,
            /// The type of the const parameter (e.g., `usize`).
            typ: VerbatimUntil<Either<Comma, Gt, Eq>>,
            /// An optional default value (e.g., `= 10`).
            default: Option<Cons<Eq, VerbatimUntil<Either<Comma, Gt>>>>,
        },
        /// A type parameter, e.g., `T: Trait = DefaultType`.
        Type {
            /// The name of the type parameter (e.g., `T`).
            name: Ident,
            /// Optional type bounds (e.g., `: Trait`).
            bounds: Option<Bounds>,
            /// An optional default type (e.g., `= DefaultType`).
            default: Option<Cons<Eq, VerbatimUntil<Either<Comma, Gt>>>>,
        },
    }

    /// Represents a `where` clause attached to a definition.
    /// e.g., `where T: Trait, 'a: 'b`.
    #[derive(Clone)]
    pub struct WhereClauses {
        /// The `where` keyword.
        pub _kw_where: KWhere,
        /// The comma-delimited list of where clause predicates.
        pub clauses: CommaDelimitedVec<WhereClause>,
    }

    /// Represents a single predicate within a `where` clause.
    /// e.g., `T: Trait` or `'a: 'b`.
    #[derive(Clone)]
    pub struct WhereClause {
        // FIXME: This likely breaks for absolute `::` paths
        /// The type or lifetime being constrained (e.g., `T` or `'a`).
        pub _pred: VerbatimUntil<Colon>,
        /// The colon separating the constrained item and its bounds.
        pub _colon: Colon,
        /// The bounds applied to the type or lifetime (e.g., `Trait` or `'b`).
        pub bounds: VerbatimUntil<Either<Comma, Semicolon, BraceGroup>>,
    }

    /// Represents the kind of a struct definition.
    pub enum StructKind {
        /// A regular struct with named fields, e.g., `struct Foo { bar: u32 }`.
        Struct {
            /// Optional where clauses.
            clauses: Option<WhereClauses>,
            /// The fields enclosed in braces `{}`.
            fields: BraceGroupContaining<CommaDelimitedVec<StructField>>,
        },
        /// A tuple struct, e.g., `struct Foo(u32, String);`.
        TupleStruct {
            /// The fields enclosed in parentheses `()`.
            fields: ParenthesisGroupContaining<CommaDelimitedVec<TupleField>>,
            /// Optional where clauses.
            clauses: Option<WhereClauses>,
            /// The trailing semicolon `;`.
            semi: Semi,
        },
        /// A unit struct, e.g., `struct Foo;`.
        UnitStruct {
            /// Optional where clauses.
            clauses: Option<WhereClauses>,
            /// The trailing semicolon `;`.
            semi: Semi,
        },
    }

    /// Represents a lifetime annotation, like `'a`.
    pub struct Lifetime {
        /// The apostrophe `'` starting the lifetime.
        pub _apostrophe: PunctJoint<'\''>,
        /// The identifier name of the lifetime (e.g., `a`).
        pub name: Ident,
    }

    /// Represents a simple expression, currently only integer literals.
    /// Used potentially for const generic default values.
    pub enum Expr {
        /// An integer literal expression.
        Integer(LiteralInteger),
    }

    /// Represents either the `const` or `mut` keyword, often used with pointers.
    pub enum ConstOrMut {
        /// The `const` keyword.
        Const(KConst),
        /// The `mut` keyword.
        Mut(KMut),
    }

    /// Represents a field within a regular struct definition.
    /// e.g., `pub name: String`.
    pub struct StructField {
        /// Attributes applied to the field (e.g., `#[doc = "..."]`).
        pub attributes: Vec<Attribute>,
        /// Optional visibility modifier (e.g., `pub`).
        pub _vis: Option<Vis>,
        /// The name of the field.
        pub name: Ident,
        /// The colon separating the name and type.
        pub _colon: Colon,
        /// The type of the field.
        pub typ: VerbatimUntil<Comma>,
    }

    /// Represents a field within a tuple struct definition.
    /// e.g., `pub String`.
    pub struct TupleField {
        /// Attributes applied to the field (e.g., `#[doc = "..."]`).
        pub attributes: Vec<Attribute>,
        /// Optional visibility modifier (e.g., `pub`).
        pub vis: Option<Vis>,
        /// The type of the field.
        pub typ: VerbatimUntil<Comma>,
    }

    /// Represents an enum definition.
    /// e.g., `#[repr(u8)] pub enum MyEnum<T> where T: Clone { Variant1, Variant2(T) }`.
    pub struct Enum {
        /// Attributes applied to the enum (e.g., `#[repr(...)]`).
        pub attributes: Vec<Attribute>,
        /// Optional visibility modifier (`pub` keyword).
        pub _pub: Option<KPub>, // FIXME: Needs to be proper Vis
        /// The `enum` keyword.
        pub _kw_enum: KEnum,
        /// The name of the enum.
        pub name: Ident,
        /// Optional generic parameters.
        pub generics: Option<GenericParams>,
        /// Optional where clauses.
        pub clauses: Option<WhereClauses>,
        /// The enum variants enclosed in braces `{}`.
        pub body: BraceGroupContaining<CommaDelimitedVec<EnumVariantLike>>,
    }

    /// Represents a variant of an enum, including the optional discriminant value
    pub struct EnumVariantLike {
        /// The actual variant
        pub variant: EnumVariantData,
        /// The optional discriminant value
        pub discriminant: Option<Cons<Eq, VerbatimUntil<Comma>>>
    }

    /// Represents the different kinds of variants an enum can have.
    pub enum EnumVariantData {
        /// A tuple-like variant, e.g., `Variant(u32, String)`.
        Tuple(TupleVariant),
        /// A struct-like variant, e.g., `Variant { field1: u32, field2: String }`.
        Struct(StructEnumVariant),
        /// A unit-like variant, e.g., `Variant`.
        Unit(UnitVariant),
    }

    /// Represents a unit-like enum variant.
    /// e.g., `MyVariant`.
    pub struct UnitVariant {
        /// Attributes applied to the variant.
        pub attributes: Vec<Attribute>,
        /// The name of the variant.
        pub name: Ident,
    }

    /// Represents a tuple-like enum variant.
    /// e.g., `MyVariant(u32, String)`.
    pub struct TupleVariant {
        /// Attributes applied to the variant.
        pub attributes: Vec<Attribute>,
        /// The name of the variant.
        pub name: Ident,
        /// The fields enclosed in parentheses `()`.
        pub fields: ParenthesisGroupContaining<CommaDelimitedVec<TupleField>>,
    }

    /// Represents a struct-like enum variant.
    /// e.g., `MyVariant { field1: u32, field2: String }`.
    pub struct StructEnumVariant {
        /// Attributes applied to the variant.
        pub attributes: Vec<Attribute>,
        /// The name of the variant.
        pub name: Ident,
        /// The fields enclosed in braces `{}`.
        pub fields: BraceGroupContaining<CommaDelimitedVec<StructField>>,
    }

    /// A lifetime or a tokentree, used to gather lifetimes in type definitions
    pub enum LifetimeOrTt {
        /// A lifetime annotation.
        Lifetime(Lifetime),
        /// A single token tree.
        TokenTree(TokenTree),
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

/// Display the verbatim tokens until the given token.
pub struct VerbatimDisplay<'a, C>(pub &'a VerbatimUntil<C>);

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

impl core::fmt::Display for WhereClauses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "where ")?;
        for clause in self.clauses.0.iter() {
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
    /// Returns an iterator over the `FacetInner` content of `#[facet(...)]` attributes
    /// applied to this struct.
    pub fn facet_attributes(&self) -> impl Iterator<Item = &FacetInner> {
        self.attributes
            .iter()
            .filter_map(|attr| match &attr.body.content {
                AttributeInner::Facet(f) => Some(&f.inner.content.0),
                _ => None,
            })
            .flatten()
            .map(|d| &d.value)
    }

    /// Returns `true` if the struct is marked `#[facet(transparent)]`.
    pub fn is_transparent(&self) -> bool {
        self.facet_attributes()
            .any(|inner| matches!(inner, FacetInner::Transparent(_)))
    }
}
