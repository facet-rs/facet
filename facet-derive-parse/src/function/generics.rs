use crate::VerbatimUntil;
#[cfg(test)]
use proc_macro2::TokenStream;
use unsynn::*;

unsynn! {
    /// Parses either a `TokenTree` or `<...>` grouping (which is not a [`Group`] as far as proc-macros
    /// are concerned).
    #[derive(Clone)]
    pub struct AngleTokenTree(
        #[allow(clippy::type_complexity)] // look,
        pub Either<Cons<Lt, Vec<Cons<Except<Gt>, AngleTokenTree>>, Gt>, TokenTree>,
    );

    /// A generic type parameter with name and optional bounds
    pub struct TypeParam {
        /// Type parameter name
        pub name: Ident,
        /// Optional colon and bounds (e.g., ": Clone + Send")
        pub bounds: Option<Cons<Colon, VerbatimUntil<Either<Comma,Gt>>>>,
    }

    /// Generic parameters with angle brackets
    pub struct GenericParams {
        /// Opening angle bracket
        pub _lt: Lt,
        /// Comma-delimited list of generic parameters
        pub params: CommaDelimitedVec<TypeParam>,
        /// Closing angle bracket
        pub _gt: Gt,
    }
}

/// Parse generics from TokenStream
#[cfg(test)]
pub fn parse_generics_for_test(input: TokenStream) -> Option<GenericParams> {
    let mut it = input.to_token_iter();
    it.parse::<GenericParams>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_no_generics() {
        let input = quote! { fn_name() };
        let generics = parse_generics_for_test(input);
        assert!(generics.is_none());
    }

    #[test]
    fn test_simple_generics() {
        let input = quote! { <T> };
        let generics = parse_generics_for_test(input).expect("should parse");
        assert_eq!(generics.params.0.len(), 1);
        assert_eq!(generics.params.0[0].value.name.to_string(), "T");
        assert!(generics.params.0[0].value.bounds.is_none());
    }

    #[test]
    fn test_multiple_generics() {
        let input = quote! { <T, U, V> };
        let generics = parse_generics_for_test(input).expect("should parse");
        assert_eq!(generics.params.0.len(), 3);
        assert_eq!(generics.params.0[0].value.name.to_string(), "T");
        assert_eq!(generics.params.0[1].value.name.to_string(), "U");
        assert_eq!(generics.params.0[2].value.name.to_string(), "V");
    }

    #[test]
    fn test_generics_with_bounds() {
        let input = quote! { <T: Clone, U: Send> };
        let generics = parse_generics_for_test(input).expect("should parse");
        assert_eq!(generics.params.0.len(), 2);
        assert_eq!(generics.params.0[0].value.name.to_string(), "T");
        assert!(generics.params.0[0].value.bounds.is_some());
        assert_eq!(generics.params.0[1].value.name.to_string(), "U");
        assert!(generics.params.0[1].value.bounds.is_some());
    }

    #[test]
    fn test_complex_generics() {
        let input = quote! { <T: Add<Output = T>, U: Iterator<Item = String>> };
        let generics = parse_generics_for_test(input).expect("should parse");
        assert_eq!(generics.params.0.len(), 2);
        assert_eq!(generics.params.0[0].value.name.to_string(), "T");
        assert_eq!(generics.params.0[1].value.name.to_string(), "U");
        assert!(generics.params.0[0].value.bounds.is_some());
        assert!(generics.params.0[1].value.bounds.is_some());
    }

    #[test]
    fn test_empty_generics() {
        let input = quote! { <> };
        let generics = parse_generics_for_test(input).expect("should parse");
        assert_eq!(generics.params.0.len(), 0);
    }
}
