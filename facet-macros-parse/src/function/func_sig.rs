use unsynn::*;

// Re-use the types from our other modules
use crate::func_params::Parameter;
use crate::generics::GenericParams;
use crate::ret_type::ReturnType;
use crate::{Attribute, AttributeInner};

keyword! {
    /// The "fn" keyword.
    pub KFn = "fn";
}

// We need to define how to parse different types of attributes
unsynn! {
    /// A complete function signature with optional attributes
    pub struct FnSignature {
        /// Zero or more attributes (including doc comments)
        pub attributes: Option<Many<Attribute>>,
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

/// Parsed function signature with extracted components including documentation
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
    /// Extracted documentation from doc comments as separate lines
    pub documentation: Vec<String>,
}

/// Extract documentation from attributes
pub fn extract_documentation(attributes: &Option<Many<Attribute>>) -> Vec<String> {
    let attrs = match attributes {
        Some(many_attrs) => &many_attrs.0,
        None => return Vec::new(),
    };

    let mut doc_lines = Vec::new();

    for attr in attrs.iter() {
        // Pattern match on the AttributeInner enum
        match &attr.value.body.content {
            AttributeInner::Doc(doc_attr) => {
                // LiteralString lets you access the unquoted value, but must be unescaped
                let doc_str = doc_attr.value.as_str().replace("\\\"", "\"");
                doc_lines.push(doc_str);
            }
            AttributeInner::Facet(_) | AttributeInner::Repr(_) | AttributeInner::Any(_) => {
                // Not a doc attribute, skipping
            }
        }
    }

    doc_lines
}

/// Parse a complete function signature from TokenStream
pub fn parse_function_signature(input: TokenStream) -> FunctionSignature {
    let mut it = input.to_token_iter();

    match it.parse::<FnSignature>() {
        Ok(sig) => {
            // Extract parameters from the parenthesis group
            let params_content = {
                let params_tokens = sig.params.to_token_stream();
                let mut it = params_tokens.to_token_iter();
                if let Ok(TokenTree::Group(group)) = it.parse::<TokenTree>() {
                    group.stream()
                } else {
                    TokenStream::new()
                }
            };
            let parameters = crate::func_params::parse_fn_parameters(params_content);

            // Extract generics if present
            let generics = sig.generics.map(|g| g.to_token_stream());

            // Extract return type if present
            let return_type = sig
                .return_type
                .map(|rt| rt.return_type.to_token_stream())
                .unwrap_or_else(|| quote! { () });

            // Extract body
            let body = sig.body.to_token_stream();

            // Extract documentation from attributes
            let documentation = extract_documentation(&sig.attributes);

            FunctionSignature {
                name: sig.name,
                generics,
                parameters,
                return_type,
                body,
                documentation,
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

    #[test]
    fn test_function_with_doc_comments() {
        let input = quote! {
            #[doc = " This is a test function"]
            #[doc = " that does addition of two numbers"]
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

        // Check documentation
        assert!(!parsed.documentation.is_empty());
        assert_eq!(parsed.documentation.len(), 2); // Two doc lines
        assert_eq!(parsed.documentation[0], " This is a test function");
        assert_eq!(
            parsed.documentation[1],
            " that does addition of two numbers"
        );
    }

    #[test]
    fn test_function_with_single_doc_comment() {
        let input = quote! {
            #[doc = " Single line documentation"]
            fn greet(name: String) -> String {
                format!("Hello, {}!", name)
            }
        };

        let parsed = parse_function_signature(input);
        assert!(!parsed.documentation.is_empty());
        assert_eq!(parsed.documentation.len(), 1);
        assert_eq!(parsed.documentation[0], " Single line documentation");
    }

    #[test]
    fn test_function_without_doc_comments() {
        let input = quote! {
            fn no_docs(x: i32) -> i32 {
                x * 2
            }
        };

        let parsed = parse_function_signature(input);
        assert_eq!(parsed.name.to_string(), "no_docs");
        assert!(parsed.documentation.is_empty());
    }

    #[test]
    fn test_function_with_mixed_attributes() {
        let input = quote! {
            #[doc = " Documentation comment"]
            #[derive(Debug)]
            #[doc = " More documentation"]
            fn mixed_attrs() {
                println!("test");
            }
        };

        let parsed = parse_function_signature(input);
        assert_eq!(parsed.name.to_string(), "mixed_attrs");
        assert!(!parsed.documentation.is_empty());
        assert_eq!(parsed.documentation.len(), 2); // Two doc lines

        assert_eq!(parsed.documentation[0], " Documentation comment");
        assert_eq!(parsed.documentation[1], " More documentation");
    }

    #[test]
    fn test_generic_function_with_docs() {
        // Imitate `///d` with `#[doc = "d"]` because `quote!` gives `r"d"` not `"d"`
        // which `unsynn::LiteralString` will not match on when parsing.
        let input = quote! {
            #[doc = " Generic function that adds two values"]
            fn generic_add<T: Add<Output = T>>(x: T, y: T) -> T {
                x + y
            }
        };

        let parsed = parse_function_signature(input);
        assert_eq!(parsed.name.to_string(), "generic_add");
        assert!(parsed.generics.is_some());
        assert!(!parsed.documentation.is_empty());
        assert_eq!(parsed.documentation.len(), 1);

        // Check the content
        assert_eq!(
            parsed.documentation[0],
            " Generic function that adds two values"
        );
    }
}
