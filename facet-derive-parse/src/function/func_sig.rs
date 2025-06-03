use proc_macro2::TokenStream;
use unsynn::*;

// Re-use the types from our other modules
use crate::func_params::Parameter;
use crate::generics::GenericParams;
use crate::ret_type::ReturnType;

keyword! {
    /// The "fn" keyword.
    pub KFn = "fn";
}

unsynn! {
    /// A complete function signature
    pub struct FnSignature {
        /// The "fn" keyword
        pub _fn_keyword: KFn,
        /// Function name
        pub name: Ident,
        /// Optional generic parameters
        pub generics: Option<GenericParams>,
        /// Parameter list in parentheses
        pub params: ParenthesisGroup,
        /// Optional return type
        pub return_type: Option<ReturnType>,
        /// Function body
        pub body: BraceGroup,
    }
}

/// Parsed function signature with extracted components
pub struct FunctionSignature {
    /// Function name
    pub name: Ident,
    /// Optional generic parameters
    pub generics: Option<TokenStream>,
    /// Function parameters
    pub parameters: Vec<Parameter>,
    /// Optional return type
    pub return_type: TokenStream,
    /// Function body
    pub body: TokenStream,
}

/// Parse a complete function signature from TokenStream
pub fn parse_function_signature(input: TokenStream) -> FunctionSignature {
    let mut it = input.to_token_iter();

    match it.parse::<FnSignature>() {
        Ok(sig) => {
            // Extract parameters from the parenthesis group
            // ParenthesisGroup contains the content, we need to get its stream
            let params_content = {
                let params_tokens = sig.params.to_token_stream();
                // Remove the outer parentheses by parsing as a group and getting its stream
                let mut it = params_tokens.to_token_iter();
                if let Ok(TokenTree::Group(group)) = it.parse::<TokenTree>() {
                    group.stream()
                } else {
                    TokenStream::new() // Empty if can't parse
                }
            };
            let parameters = crate::func_params::parse_fn_parameters(params_content);

            // Extract generics if present
            let generics = sig.generics.map(|g| g.to_token_stream());

            // Extract return type if present
            let return_type = sig
                .return_type
                .map(|rt| rt.return_type.to_token_stream())
                .unwrap_or_else(|| quote::quote! { () });

            // Extract body
            let body = sig.body.to_token_stream();

            FunctionSignature {
                name: sig.name,
                generics,
                parameters,
                return_type,
                body,
            }
        }
        Err(err) => {
            panic!("Failed to parse function signature: {}", err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_simple_function() {
        let input = quote! {
            fn add(x: i32, y: i32) -> i32 {
                x + y
            }
        };

        let parsed = parse_function_signature(input);
        assert_eq!(parsed.name.to_string(), "add");
        assert!(parsed.generics.is_none());
        assert_eq!(parsed.parameters.len(), 2);
        assert_eq!(parsed.parameters[0].name.to_string(), "x");
        assert_eq!(parsed.parameters[1].name.to_string(), "y");
        assert_eq!(parsed.return_type.to_string().trim(), "i32");
    }

    #[test]
    fn test_generic_function() {
        let input = quote! {
            fn generic_add<T>(x: T, y: T) -> T {
                x + y
            }
        };

        let parsed = parse_function_signature(input);
        assert_eq!(parsed.name.to_string(), "generic_add");
        assert!(parsed.generics.is_some());
        assert_eq!(parsed.parameters.len(), 2);
        assert_eq!(parsed.return_type.to_string().trim(), "T");
    }

    #[test]
    fn test_no_params_function() {
        let input = quote! {
            fn no_params() -> &'static str {
                "hello"
            }
        };

        let parsed = parse_function_signature(input);
        assert_eq!(parsed.name.to_string(), "no_params");
        assert_eq!(parsed.parameters.len(), 0);
        assert_eq!(parsed.return_type.to_string().trim(), "& 'static str");
    }

    #[test]
    fn test_no_return_type() {
        let input = quote! {
            fn no_return(x: i32) {
                println!("{}", x);
            }
        };

        let parsed = parse_function_signature(input);
        assert_eq!(parsed.name.to_string(), "no_return");
        assert_eq!(parsed.parameters.len(), 1);
        assert_eq!(parsed.return_type.to_string().trim(), "()");
    }
}
