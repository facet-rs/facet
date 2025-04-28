use facet_derive_parse::*;

mod renamerule;
pub use renamerule::*;

mod generics;
pub use generics::*;

mod attributes;
pub use attributes::*;

mod process_enum;
mod process_struct;

mod derive;
pub use derive::*;

#[derive(Clone)]
pub struct LifetimeName(pub facet_derive_parse::Ident);

impl quote::ToTokens for LifetimeName {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let punct = facet_derive_parse::TokenTree::Punct(facet_derive_parse::Punct::new(
            '\'',
            facet_derive_parse::Spacing::Joint,
        ));
        let name = &self.0;
        tokens.extend(quote::quote! {
            #punct #name
        });
    }
}
