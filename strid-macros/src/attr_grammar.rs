//! Attribute parsing grammar for braid macro attributes.

// Import unsynn macros and types
use unsynn::{
    CommaDelimitedVec, Cons, Ident, Literal, ParenthesisGroupContaining, TokenTree, keyword,
    operator, unsynn,
};

keyword! {
    KBool = "bool";
    KOption = "Option";
}

operator! {
    Eq = "=";
}

unsynn! {
    /// Represents attribute arguments: name or name = value
    pub enum AttrArg {
        /// Name-value attribute: `debug = "impl"`
        NameValue(Cons<Ident, Eq, Literal>),
        /// List attribute: `ref_attr(...)`
        List(Cons<Ident, ParenthesisGroupContaining<Vec<TokenTree>>>),
        /// Path-only attribute: `validator`
        Path(Ident),
    }

    /// A comma-delimited list of attribute arguments
    pub struct AttrArgs {
        /// The arguments
        pub args: CommaDelimitedVec<AttrArg>,
    }
}

impl AttrArg {
    /// Get the path/name identifier
    pub fn name(&self) -> &Ident {
        match self {
            AttrArg::Path(name) => name,
            AttrArg::NameValue(nv) => &nv.first,
            AttrArg::List(list) => &list.first,
        }
    }

    /// Get the literal value if this is a name-value attribute
    pub fn value(&self) -> Option<&Literal> {
        match self {
            AttrArg::NameValue(nv) => Some(&nv.third),
            _ => None,
        }
    }

    /// Get the list contents if this is a list attribute
    pub fn list_contents(&self) -> Option<&[TokenTree]> {
        match self {
            AttrArg::List(list) => Some(&list.second.content),
            _ => None,
        }
    }
}

impl quote::ToTokens for AttrArg {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        use unsynn::ToTokens as _;
        match self {
            AttrArg::Path(ident) => quote::ToTokens::to_tokens(ident, tokens),
            AttrArg::NameValue(nv) => {
                quote::ToTokens::to_tokens(&nv.first, tokens);
                tokens.extend(nv.second.to_token_stream());
                quote::ToTokens::to_tokens(&nv.third, tokens);
            }
            AttrArg::List(list) => {
                quote::ToTokens::to_tokens(&list.first, tokens);
                tokens.extend(list.second.to_token_stream());
            }
        }
    }
}
