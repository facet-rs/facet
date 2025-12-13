//! Unsynn grammar definitions for parsing Rust struct items and attributes.

use unsynn::*;

// ============================================================================
// KEYWORDS AND OPERATORS
// ============================================================================

keyword! {
    /// The "pub" keyword.
    pub KPub = "pub";
    /// The "struct" keyword.
    pub KStruct = "struct";
    /// The "crate" keyword.
    pub KCrate = "crate";
    /// The "in" keyword.
    pub KIn = "in";
    /// The "where" keyword.
    pub KWhere = "where";
}

operator! {
    /// Represents the '=' operator.
    pub Eq = "=";
    /// Represents the ';' operator.
    pub Semi = ";";
    /// Represents the '::' operator.
    pub PathSep = "::";
    /// Represents the '<' operator.
    pub Lt = "<";
    /// Represents the '>' operator.
    pub Gt = ">";
}

// ============================================================================
// HELPER TYPES
// ============================================================================

/// Represents a module path, consisting of an optional path separator followed by
/// a path-separator-delimited sequence of identifiers.
pub type ModPath = Cons<Option<PathSep>, PathSepDelimited<Ident>>;

/// Parses tokens and groups until `C` is found on the current token tree level.
pub type VerbatimUntil<C> = Many<Cons<Except<C>, AngleTokenTree>>;

// ============================================================================
// UNSYNN GRAMMAR
// ============================================================================

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping (which is not a [`Group`] as far as proc-macros
    /// are concerned).
    #[derive(Clone)]
    pub struct AngleTokenTree(
        #[allow(clippy::type_complexity)]
        pub Either<Cons<Lt, Vec<Cons<Except<Gt>, AngleTokenTree>>, Gt>, TokenTree>,
    );

    /// Represents visibility modifiers for items.
    pub enum Vis {
        /// `pub(in? crate::foo::bar)`/`pub(in? ::foo::bar)`
        PubIn(Cons<KPub, ParenthesisGroupContaining<Cons<Option<KIn>, ModPath>>>),
        /// Public visibility, indicated by the "pub" keyword.
        Pub(KPub),
    }

    /// Represents an attribute annotation, typically in the form `#[attr]` or `#![inner_attr]`.
    #[derive(Clone)]
    pub struct Attribute {
        /// The pound sign preceding the attribute.
        pub _pound: Pound,
        /// Optional exclamation mark for inner attributes.
        pub _inner: Option<Bang>,
        /// The content of the attribute enclosed in square brackets.
        pub body: BracketGroupContaining<Vec<TokenTree>>,
    }

    /// Represents a struct item definition.
    pub struct ItemStruct {
        /// Attributes on the struct.
        pub attrs: Vec<Attribute>,
        /// Visibility of the struct.
        pub vis: Option<Vis>,
        /// The "struct" keyword.
        pub _struct_token: KStruct,
        /// The name of the struct.
        pub ident: Ident,
        /// Generic parameters.
        pub generics: Option<Generics>,
        /// Where clause.
        pub where_clause: Option<WhereClause>,
        /// The struct fields.
        pub fields: Fields,
        /// Optional semicolon for unit structs.
        pub semi_token: Option<Semicolon>,
    }

    /// Generic parameters: `<T, U: Bound>`
    pub struct Generics {
        /// The '<' token.
        pub _lt: Lt,
        /// Generic parameters.
        pub params: Vec<TokenTree>,
        /// The '>' token.
        pub _gt: Gt,
    }

    /// Where clause: `where T: Bound`
    pub struct WhereClause {
        /// The "where" keyword.
        pub _where: KWhere,
        /// Predicates.
        pub predicates: Vec<TokenTree>,
    }

    /// Struct fields - can be named, unnamed, or unit.
    pub enum Fields {
        /// Named fields: `{ x: i32, y: i32 }`
        Named(BraceGroupContaining<CommaDelimitedVec<NamedField>>),
        /// Unnamed fields: `(i32, i32)`
        Unnamed(ParenthesisGroupContaining<CommaDelimitedVec<UnnamedField>>),
    }

    /// A named field in a struct.
    pub struct NamedField {
        /// Attributes on the field.
        pub attrs: Vec<Attribute>,
        /// Visibility of the field.
        pub vis: Option<Vis>,
        /// The field name.
        pub ident: Ident,
        /// The colon separator.
        pub _colon: Colon,
        /// The field type (captured as raw tokens).
        pub ty: Type,
    }

    /// An unnamed field in a tuple struct.
    pub struct UnnamedField {
        /// Attributes on the field.
        pub attrs: Vec<Attribute>,
        /// Visibility of the field.
        pub vis: Option<Vis>,
        /// The field type (captured as raw tokens).
        pub ty: Type,
    }

    /// A type - captured as raw tokens until we hit a delimiter.
    #[derive(Clone)]
    pub struct Type {
        /// Type tokens.
        pub tokens: VerbatimUntil<Either<Comma, Gt, Semicolon>>,
    }
}

impl ItemStruct {
    /// Check if the struct has no fields.
    pub fn is_empty(&self) -> bool {
        match &self.fields {
            Fields::Named(f) => f.content.is_empty(),
            Fields::Unnamed(f) => f.content.is_empty(),
        }
    }
}

/// A field reference - either named or unnamed.
pub enum Field<'a> {
    /// A named field.
    Named(&'a NamedField),
    /// An unnamed field.
    Unnamed(&'a UnnamedField),
}

impl<'a> Field<'a> {
    /// Get the field attributes.
    pub fn attrs(&self) -> &[Attribute] {
        match self {
            Field::Named(f) => &f.attrs,
            Field::Unnamed(f) => &f.attrs,
        }
    }

    /// Get the field type.
    pub fn ty(&self) -> &Type {
        match self {
            Field::Named(f) => &f.ty,
            Field::Unnamed(f) => &f.ty,
        }
    }

    /// Get the field name, if it's a named field.
    pub fn ident(&self) -> Option<&Ident> {
        match self {
            Field::Named(f) => Some(&f.ident),
            Field::Unnamed(_) => None,
        }
    }

    /// Get the visibility.
    pub fn vis(&self) -> Option<&Vis> {
        match self {
            Field::Named(f) => f.vis.as_ref(),
            Field::Unnamed(f) => f.vis.as_ref(),
        }
    }
}

impl Type {
    /// Convert the type tokens to a TokenStream2 for code generation.
    pub fn to_token_stream(&self) -> proc_macro2::TokenStream {
        use quote::ToTokens;
        let mut tokens = proc_macro2::TokenStream::new();
        for cons in self.tokens.iter() {
            cons.value.to_tokens(&mut tokens);
        }
        tokens
    }
}

impl quote::ToTokens for Type {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(self.to_token_stream());
    }
}

impl quote::ToTokens for Attribute {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self._pound.to_tokens(tokens);
        if let Some(ref inner) = self._inner {
            inner.to_tokens(tokens);
        }
        self.body.to_tokens(tokens);
    }
}

impl quote::ToTokens for ItemStruct {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        // Output attributes
        for attr in &self.attrs {
            quote::ToTokens::to_tokens(attr, tokens);
        }

        // Output visibility
        if let Some(ref vis) = self.vis {
            tokens.extend(vis.to_token_stream());
        }

        // Output struct keyword
        tokens.extend(self._struct_token.to_token_stream());

        // Output name
        quote::ToTokens::to_tokens(&self.ident, tokens);

        // Output generics
        if let Some(ref generics) = self.generics {
            tokens.extend(generics.to_token_stream());
        }

        // Output where clause
        if let Some(ref where_clause) = self.where_clause {
            tokens.extend(where_clause.to_token_stream());
        }

        // Output fields
        tokens.extend(self.fields.to_token_stream());

        // Output semicolon if present
        if let Some(ref semi) = self.semi_token {
            tokens.extend(semi.to_token_stream());
        }
    }
}
