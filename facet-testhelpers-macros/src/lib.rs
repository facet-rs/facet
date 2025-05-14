use unsynn::*;

keyword! {
    KFn = "fn";
}

unsynn! {
    struct UntilFn {
        items: Any<Cons<Except<KFn>, TokenTree>>,
    }

    struct UntilBody {
        items: Any<Cons<Except<BraceGroup>, TokenTree>>,
    }

    struct Body {
        items: BraceGroup,
    }

    struct FunctionDecl {
        until_fn: UntilFn, _fn: KFn, name: Ident,
        until_body: UntilBody, body: Body
    }
}

impl quote::ToTokens for UntilFn {
    fn to_tokens(&self, tokens: &mut unsynn::TokenStream) {
        self.items.to_tokens(tokens)
    }
}

impl quote::ToTokens for UntilBody {
    fn to_tokens(&self, tokens: &mut unsynn::TokenStream) {
        self.items.to_tokens(tokens)
    }
}

impl quote::ToTokens for Body {
    fn to_tokens(&self, tokens: &mut unsynn::TokenStream) {
        tokens.extend(self.items.0.stream())
    }
}

#[proc_macro_attribute]
pub fn test(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item = TokenStream::from(item);
    let mut i = item.to_token_iter();
    let fdecl = i.parse::<FunctionDecl>().unwrap();

    let FunctionDecl {
        until_fn,
        _fn,
        name,
        until_body,
        body,
    } = fdecl;

    quote::quote! {
        #[::core::prelude::rust_2024::test]
        #until_fn fn #name #until_body -> ::facet_testhelpers::eyre::Result<()> {
            ::facet_testhelpers::setup();

            #body

            Ok(())
        }
    }
    .into()
}
