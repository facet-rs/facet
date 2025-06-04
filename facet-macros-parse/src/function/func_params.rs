use crate::VerbatimUntil;
use proc_macro2::TokenStream;
use unsynn::*;

unsynn! {
    /// A function parameter with name and type
    pub struct Parameter {
        /// Parameter name
        pub name: Ident,
        /// Colon separator
        pub _colon: Colon,
        /// Parameter type (everything until comma or end)
        pub param_type: VerbatimUntil<Comma>,
    }
}

impl Parameter {
    /// Convert the parameter type to TokenStream for use with quote!
    pub fn param_type_tokens(&self) -> TokenStream {
        self.param_type.to_token_stream()
    }
}

/// Parse function parameters from a TokenStream (content of parentheses)
/// Returns a Vec of Parameter structs
pub fn parse_fn_parameters(params_ts: TokenStream) -> Vec<Parameter> {
    let mut it = params_ts.to_token_iter();

    // Parse as comma-delimited list of parameters
    match it.parse::<CommaDelimitedVec<Parameter>>() {
        Ok(params) => params.0.into_iter().map(|delim| delim.value).collect(),
        Err(_) => Vec::new(), // Empty parameter list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_empty_parameters() {
        let input = quote! {};
        let params = parse_fn_parameters(input);
        assert_eq!(params.len(), 0);
    }

    #[test]
    fn test_single_parameter() {
        let input = quote! { x: i32 };
        let params = parse_fn_parameters(input);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name.to_string(), "x");
        assert_eq!(params[0].param_type.tokens_to_string().trim(), "i32");
    }

    #[test]
    fn test_multiple_parameters() {
        let input = quote! { x: i32, y: String };
        let params = parse_fn_parameters(input);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name.to_string(), "x");
        assert_eq!(params[0].param_type.tokens_to_string().trim(), "i32");
        assert_eq!(params[1].name.to_string(), "y");
        assert_eq!(params[1].param_type.tokens_to_string().trim(), "String");
    }

    #[test]
    fn test_complex_types() {
        let input = quote! { callback: fn(i32) -> String, data: Vec<Option<u64>> };
        let params = parse_fn_parameters(input);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name.to_string(), "callback");
        assert_eq!(
            params[0].param_type.tokens_to_string().trim(),
            "fn (i32) -> String"
        );
        assert_eq!(params[1].name.to_string(), "data");
        assert_eq!(
            params[1].param_type.tokens_to_string().trim(),
            "Vec < Option < u64 > >"
        );
    }
}
