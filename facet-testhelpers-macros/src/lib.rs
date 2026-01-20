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

/// Test attribute macro that sets up tracing before running the test.
///
/// # Usage
///
/// Basic usage (uses `#[test]`):
/// ```ignore
/// #[facet_testhelpers::test]
/// fn my_test() {
///     // tracing is set up automatically
/// }
/// ```
///
/// With a custom test attribute (e.g., for async tests):
/// ```ignore
/// #[facet_testhelpers::test(tokio::test)]
/// async fn my_async_test() {
///     // tracing is set up automatically
/// }
/// ```
#[proc_macro_attribute]
pub fn test(
    attr: proc_macro::TokenStream,
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

    // If an attribute argument is provided, use it as the test attribute
    // e.g., #[facet_testhelpers::test(tokio::test)] -> #[tokio::test]
    let test_attr = if attr.is_empty() {
        quote::quote! { #[::core::prelude::rust_2024::test] }
    } else {
        let attr = TokenStream::from(attr);
        quote::quote! { #[#attr] }
    };

    quote::quote! {
        #test_attr
        #until_fn fn #name #until_body {
            ::facet_testhelpers::setup();

            #body
        }
    }
    .into()
}
