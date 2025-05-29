use crate::parser::*;

mod renamerule;
pub use renamerule::*;

mod generics;
pub use generics::*;

mod parser;

mod parsed;
pub use parsed::*;

mod process_enum;
mod process_struct;

mod derive;
pub use derive::*;

#[derive(Clone)]
pub struct LifetimeName(pub crate::parser::Ident);

impl quote::ToTokens for LifetimeName {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let punct = crate::parser::TokenTree::Punct(crate::parser::Punct::new(
            '\'',
            crate::parser::Spacing::Joint,
        ));
        let name = &self.0;
        tokens.extend(quote::quote! {
            #punct #name
        });
    }
}
