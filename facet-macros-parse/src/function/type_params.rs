use proc_macro2::TokenStream;
use unsynn::*;

// Re-use the generics parser
use crate::generics::GenericParams;

/// Extract just the type parameter names from generic parameters
/// Returns a TokenStream suitable for PhantomData<(A, B, C)>
pub fn extract_type_params(generics_ts: TokenStream) -> TokenStream {
    let mut it = generics_ts.to_token_iter();

    match it.parse::<GenericParams>() {
        Ok(generics) => {
            let type_param_names: Vec<_> = generics
                .params
                .0
                .into_iter()
                .map(|delim| delim.value.name)
                .collect();

            if type_param_names.is_empty() {
                quote::quote! { () }
            } else if type_param_names.len() == 1 {
                let param = &type_param_names[0];
                quote::quote! { #param }
            } else {
                quote::quote! { ( #( #type_param_names ),* ) }
            }
        }
        Err(_) => {
            // Fallback to unit type if parsing fails
            quote::quote! { () }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_single_type_param() {
        let input = quote! { <T> };
        let result = extract_type_params(input);
        assert_eq!(result.to_string().trim(), "T");
    }

    #[test]
    fn test_multiple_type_params() {
        let input = quote! { <A, B, C> };
        let result = extract_type_params(input);
        assert_eq!(result.to_string().trim(), "(A , B , C)");
    }

    #[test]
    fn test_type_params_with_bounds() {
        let input = quote! { <T: Clone, U: Send> };
        let result = extract_type_params(input);
        assert_eq!(result.to_string().trim(), "(T , U)");
    }

    #[test]
    fn test_empty_generics() {
        let input = quote! { <> };
        let result = extract_type_params(input);
        assert_eq!(result.to_string().trim(), "()");
    }
}
