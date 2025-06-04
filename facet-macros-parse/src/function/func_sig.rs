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

// We need to define how to parse different types of attributes
unsynn! {
    /// A doc attribute: #[doc = "content"]
    pub struct DocAttribute {
        /// The "doc" identifier
        pub _doc: Ident, // Should be "doc"
        /// The equals sign
        pub _equals: PunctAny<'='>,
        /// The documentation string
        pub lit_content: TokenStream,
    }

    /// Different types of attribute content
    pub enum AttributeContent {
        /// A doc comment attribute
        Doc(DocAttribute),
        /// Any other attribute (we don't parse these)
        Other(TokenStream),
    }

    /// An attribute like #[doc = "content"] or #[derive(Debug)]
    pub struct Attribute {
        /// The # symbol
        pub _hash: PunctAny<'#'>,
        /// The attribute content in brackets, parsed
        pub content: BracketGroupContaining<AttributeContent>,
    }

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
        // Pattern match on the AttributeContent enum (this was working!)
        match &attr.value.content.content {
            AttributeContent::Doc(doc_attr) => {
                // Extract content from the Literal using the robust parsing function
                let literal_str = &doc_attr.lit_content.to_string();
                if let Some(content) = parse_string_literal(literal_str) {
                    doc_lines.push(content);
                }
            }
            AttributeContent::Other(_) => {
                // Not a doc attribute, skipping
            }
        }
    }

    doc_lines
}

/// Parse the `proc_macro2::Literal` that we had to send `to_string()` to access
fn parse_string_literal(literal_str: &str) -> Option<String> {
    enum StringLiteralType {
        Raw { hash_count: usize },
        Regular,
    }

    let literal_type = if let Some(after_r) = literal_str.strip_prefix('r') {
        let hash_count = after_r.chars().take_while(|&c| c == '#').count();
        StringLiteralType::Raw { hash_count }
    } else if literal_str.starts_with('"') {
        StringLiteralType::Regular
    } else {
        return None;
    };

    match literal_type {
        StringLiteralType::Raw { hash_count } => {
            let quote_start = 1 + hash_count; // After 'r' and hashes
            let quote_char = literal_str.chars().nth(quote_start)?;

            if quote_char != '"' {
                return None;
            }

            let content_start = quote_start + 1; // After opening quote
            let content_end = literal_str.len().checked_sub(1 + hash_count)?; // Before closing quote and hashes

            if content_start <= content_end {
                Some(literal_str[content_start..content_end].to_string())
            } else {
                None
            }
        }

        StringLiteralType::Regular => {
            if literal_str.ends_with('"') && literal_str.len() >= 2 {
                let content = &literal_str[1..literal_str.len() - 1];
                let unescaped = content.replace("\\\"", "\"").replace("\\\\", "\\");
                Some(unescaped)
            } else {
                None
            }
        }
    }
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
                .unwrap_or_else(|| quote::quote! { () });

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

    #[test]
    fn test_function_with_doc_comments() {
        let input = quote! {
            /// This is a test function
            /// that does addition of two numbers
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
            /// Single line documentation
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
    fn test_function_with_double_quote_in_doc_comment() {
        let input = quote! {
            /// Hello "world", if that is your real name
            fn greet(name: String) -> String {
                format!("Hello, {}...?", name)
            }
        };

        let parsed = parse_function_signature(input);
        assert!(!parsed.documentation.is_empty());
        assert_eq!(parsed.documentation.len(), 1);
        assert_eq!(
            parsed.documentation[0],
            " Hello \"world\", if that is your real name"
        );
    }

    #[test]
    fn test_function_with_multiple_hashes_in_doc_comment() {
        let input = quote! {
            /// This uses r#"raw strings"# and r##"nested"## syntax
            fn complex_doc() {
                println!("test");
            }
        };

        let parsed = parse_function_signature(input);
        assert!(!parsed.documentation.is_empty());
        assert_eq!(parsed.documentation.len(), 1);
        assert_eq!(
            parsed.documentation[0],
            " This uses r#\"raw strings\"# and r##\"nested\"## syntax"
        );
    }

    #[test]
    fn test_function_with_mixed_attributes() {
        let input = quote! {
            /// Documentation comment
            #[derive(Debug)]
            /// More documentation
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
        let input = quote! {
            /// Generic function that adds two values
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
