#[cfg(test)]
use proc_macro2::TokenStream;
use unsynn::*;

/// Parses tokens until `C` is found on the current token tree level.
type VerbatimUntil<C> = Many<Cons<Except<C>, TokenTree>>;

unsynn! {
    /// A function signature followed by its body
    pub struct FunctionWithBody {
        /// Everything before the function body (return type, etc.) - optional
        pub prefix: Option<VerbatimUntil<BraceGroup>>,
        /// The function body in braces
        pub body: BraceGroup,
    }
}

/// Parse function body from tokens using declarative parsing
/// Returns the function body as TokenStream
#[cfg(test)]
pub fn parse_function_body(tokens: &[TokenTree]) -> TokenStream {
    // Convert tokens to TokenStream for parsing
    let mut token_stream = TokenStream::new();
    for token in tokens {
        token_stream.extend(core::iter::once(token.clone()));
    }

    let mut it = token_stream.to_token_iter();

    match it.parse::<FunctionWithBody>() {
        Ok(func_with_body) => func_with_body.body.to_token_stream(),
        Err(_) => {
            // No function body found, return empty braces
            quote::quote! { {} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_simple_function_body() {
        let input: Vec<TokenTree> = quote! { { x + y } }.into_iter().collect();
        let body = parse_function_body(&input);
        assert_eq!(body.to_string().trim(), "{ x + y }");
    }

    #[test]
    fn test_complex_function_body() {
        let input: Vec<TokenTree> = quote! { {
            let result = x + y;
            println!("Result: {}", result);
            result
        } }
        .into_iter()
        .collect();
        let body = parse_function_body(&input);
        let body_str = body.to_string();
        assert!(body_str.contains("let result"));
        assert!(body_str.contains("println !"));
        assert!(body_str.starts_with('{'));
        assert!(body_str.ends_with('}'));
    }

    #[test]
    fn test_function_with_trailing_tokens() {
        // Simulate tokens like: -> i32 { x + y }
        let input: Vec<TokenTree> = quote! { -> i32 { x + y } }.into_iter().collect();
        let body = parse_function_body(&input);
        assert_eq!(body.to_string().trim(), "{ x + y }");
    }

    #[test]
    fn test_no_function_body() {
        let input: Vec<TokenTree> = quote! { -> i32 }.into_iter().collect();
        let body = parse_function_body(&input);
        assert_eq!(body.to_string().trim(), "{ }");
    }

    #[test]
    fn test_nested_braces() {
        let input: Vec<TokenTree> = quote! { {
            if condition {
                inner_block();
            }
        } }
        .into_iter()
        .collect();
        let body = parse_function_body(&input);
        let body_str = body.to_string();
        assert!(body_str.contains("if condition"));
        assert!(body_str.contains("inner_block"));
    }
}
