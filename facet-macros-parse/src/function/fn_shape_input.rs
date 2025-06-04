use proc_macro2::TokenStream;
use unsynn::*;

// Re-use the generics parser from our other module
use crate::generics::GenericParams;

unsynn! {
    /// Input to fn_shape! macro: function_name or function_name<generics>
    pub struct FnShapeInput {
        /// Function name
        pub name: Ident,
        /// Optional generic parameters
        pub generics: Option<GenericParams>,
    }
}

/// Parsed fn_shape input with extracted components
pub struct ParsedFnShapeInput {
    /// Function name
    pub name: Ident,
    /// Optional generic type parameters
    pub generics: Option<TokenStream>,
}

/// Parse fn_shape! macro input from TokenStream
pub fn parse_fn_shape_input(input: TokenStream) -> ParsedFnShapeInput {
    let mut it = input.to_token_iter();

    match it.parse::<FnShapeInput>() {
        Ok(shape_input) => {
            let name = shape_input.name;
            let generics = shape_input.generics.map(|g| g.to_token_stream());

            ParsedFnShapeInput { name, generics }
        }
        Err(err) => {
            panic!("Failed to parse fn_shape input: {}", err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_simple_function_name() {
        let input = quote! { add };
        let parsed = parse_fn_shape_input(input);
        assert_eq!(parsed.name.to_string(), "add");
        assert!(parsed.generics.is_none());
    }

    #[test]
    fn test_function_with_single_generic() {
        let input = quote! { generic_add<T> };
        let parsed = parse_fn_shape_input(input);
        assert_eq!(parsed.name.to_string(), "generic_add");
        assert!(parsed.generics.is_some());
        assert_eq!(parsed.generics.unwrap().to_string().trim(), "< T >");
    }

    #[test]
    fn test_function_with_multiple_generics() {
        let input = quote! { multi_ty_param_fn<T, U> };
        let parsed = parse_fn_shape_input(input);
        assert_eq!(parsed.name.to_string(), "multi_ty_param_fn");
        assert!(parsed.generics.is_some());
        assert_eq!(parsed.generics.unwrap().to_string().trim(), "< T , U >");
    }

    #[test]
    fn test_function_with_bounded_generics() {
        let input = quote! { bounded_fn<U: Clone> };
        let parsed = parse_fn_shape_input(input);
        assert_eq!(parsed.name.to_string(), "bounded_fn");
        assert!(parsed.generics.is_some());
        assert_eq!(parsed.generics.unwrap().to_string().trim(), "< U : Clone >");
    }

    #[test]
    fn test_function_with_complex_generics() {
        let input = quote! { nested_fn<T: Add<Output = T>> };
        let parsed = parse_fn_shape_input(input);
        assert_eq!(parsed.name.to_string(), "nested_fn");
        assert!(parsed.generics.is_some());
        assert_eq!(
            parsed.generics.unwrap().to_string().trim(),
            "< T : Add < Output = T > >"
        );
    }
}
