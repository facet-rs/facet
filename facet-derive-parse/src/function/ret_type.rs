#[cfg(test)]
use proc_macro2::TokenStream;
use unsynn::*;

/// Parses tokens until `C` is found on the current token tree level.
type VerbatimUntil<C> = Many<Cons<Except<C>, TokenTree>>;

unsynn! {
    /// A return type annotation with arrow and type
    pub struct ReturnType {
        /// The "->" arrow
        pub _arrow: RArrow,
        /// Return type (everything until brace group)
        pub return_type: VerbatimUntil<BraceGroup>,
    }
}

/// Parse return type from tokens after parameters
/// Returns the return type as TokenStream, or unit type () if no return type found
#[cfg(test)]
pub fn parse_return_type(tokens: Vec<TokenTree>) -> TokenStream {
    let mut token_stream = TokenStream::new();
    for token in &tokens {
        token_stream.extend(core::iter::once(token.clone()));
    }

    let mut it = token_stream.to_token_iter();

    match it.parse::<ReturnType>() {
        Ok(ret_type) => ret_type.return_type.to_token_stream(),
        Err(_) => {
            // No return type found, default to unit type
            quote::quote! { () }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_no_return_type() {
        let input = vec![];
        let ret_type = parse_return_type(input);
        assert_eq!(ret_type.to_string().trim(), "()");
    }

    #[test]
    fn test_simple_return_type() {
        let input: Vec<TokenTree> = quote! { -> i32 }.into_iter().collect();
        let ret_type = parse_return_type(input);
        assert_eq!(ret_type.to_string().trim(), "i32");
    }

    #[test]
    fn test_complex_return_type() {
        let input: Vec<TokenTree> = quote! { -> Result<String, Box<dyn Error>> }
            .into_iter()
            .collect();
        let ret_type = parse_return_type(input);
        assert_eq!(
            ret_type.to_string().trim(),
            "Result < String , Box < dyn Error >>"
        );
    }

    #[test]
    fn test_reference_return_type() {
        let input: Vec<TokenTree> = quote! { -> &'static str }.into_iter().collect();
        let ret_type = parse_return_type(input);
        assert_eq!(ret_type.to_string().trim(), "& 'static str");
    }
}
