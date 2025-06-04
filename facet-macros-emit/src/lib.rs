use facet_macros_parse::*;

mod renamerule;
pub use renamerule::*;

mod generics;
pub use generics::*;

mod parsed;
pub use parsed::*;

mod process_enum;
mod process_struct;

mod derive;
pub use derive::*;

#[cfg(feature = "function")]
pub mod function;

#[derive(Clone)]
pub struct LifetimeName(pub facet_macros_parse::Ident);

impl quote::ToTokens for LifetimeName {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let punct = facet_macros_parse::TokenTree::Punct(facet_macros_parse::Punct::new(
            '\'',
            facet_macros_parse::Spacing::Joint,
        ));
        let name = &self.0;
        tokens.extend(quote::quote! {
            #punct #name
        });
    }
}
