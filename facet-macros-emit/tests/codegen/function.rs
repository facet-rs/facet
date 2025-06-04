use facet_macros_emit::function::generate_function_shape;
use facet_macros_parse::TokenStream;
use facet_macros_parse::function::*;
use rust_format::{Formatter, RustFmt};

/// Helper function to parse and generate function shape code, then format it
fn expand_function(input: &str) -> String {
    let trimmed_input = input.trim();
    let input_ts: TokenStream = trimmed_input.parse().expect("Failed to parse input");

    let parsed = parse_function_signature(input_ts);
    let generated = generate_function_shape(parsed);

    RustFmt::default()
        .format_tokens(generated)
        .unwrap_or_else(|e| panic!("Format error: {e}"))
}

/// Helper function to parse function signature and verify its components
fn parse_and_verify_signature(input: &str) -> FunctionSignature {
    let trimmed_input = input.trim();
    let input_ts: TokenStream = trimmed_input.parse().expect("Failed to parse input");
    parse_function_signature(input_ts)
}

#[test]
fn test_simple_function_parsing() {
    let input = r#"
        fn add(x: i32, y: i32) -> i32 {
            x + y
        }
    "#;

    let parsed = parse_and_verify_signature(input);
    assert_eq!(parsed.name.to_string(), "add");
    assert!(parsed.generics.is_none());
    assert_eq!(parsed.parameters.len(), 2);
    assert_eq!(parsed.parameters[0].name.to_string(), "x");
    assert_eq!(parsed.parameters[1].name.to_string(), "y");
    assert_eq!(parsed.return_type.to_string().trim(), "i32");
}

#[test]
#[ignore]
fn test_no_params_function_parsing() {
    let input = r#"
        fn no_params() -> &'static str {
            "No parameters here!"
        }
    "#;

    let parsed = parse_and_verify_signature(input);
    assert_eq!(parsed.name.to_string(), "no_params");
    assert!(parsed.generics.is_none());
    assert_eq!(parsed.parameters.len(), 0);
    assert_eq!(parsed.return_type.to_string().trim(), "& 'static str");
}

#[test]
fn test_generic_function_parsing() {
    let input = r#"
        fn generic_add<T: Add<Output = T>>(x: T, y: T) -> T {
            x + y
        }
    "#;

    let parsed = parse_and_verify_signature(input);
    assert_eq!(parsed.name.to_string(), "generic_add");
    assert!(parsed.generics.is_some());
    assert_eq!(parsed.parameters.len(), 2);
    assert_eq!(parsed.parameters[0].name.to_string(), "x");
    assert_eq!(parsed.parameters[1].name.to_string(), "y");
    assert_eq!(parsed.return_type.to_string().trim(), "T");
}

#[test]
fn test_string_function_parsing() {
    let input = r#"
        fn greet(name: String) -> String {
            format!("Hello, {}!", name)
        }
    "#;

    let parsed = parse_and_verify_signature(input);
    assert_eq!(parsed.name.to_string(), "greet");
    assert!(parsed.generics.is_none());
    assert_eq!(parsed.parameters.len(), 1);
    assert_eq!(parsed.parameters[0].name.to_string(), "name");
    assert_eq!(parsed.return_type.to_string().trim(), "String");
}

#[test]
fn test_complex_generic_function_parsing() {
    let input = r#"
        fn complex_fn<T, U>(x: Vec<T>, y: Option<U>) -> Result<T, U> {
            todo!()
        }
    "#;

    let parsed = parse_and_verify_signature(input);
    assert_eq!(parsed.name.to_string(), "complex_fn");
    assert!(parsed.generics.is_some());
    assert_eq!(parsed.parameters.len(), 2);
    assert_eq!(parsed.parameters[0].name.to_string(), "x");
    assert_eq!(parsed.parameters[1].name.to_string(), "y");
    assert_eq!(parsed.return_type.to_string().trim(), "Result < T , U >");
}

#[test]
fn test_no_return_type_function_parsing() {
    let input = r#"
        fn no_return(x: i32) {
            println!("{}", x);
        }
    "#;

    let parsed = parse_and_verify_signature(input);
    assert_eq!(parsed.name.to_string(), "no_return");
    assert!(parsed.generics.is_none());
    assert_eq!(parsed.parameters.len(), 1);
    assert_eq!(parsed.parameters[0].name.to_string(), "x");
    assert_eq!(parsed.return_type.to_string().trim(), "()");
}

#[test]
fn simple_function() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn add(x: i32, y: i32) -> i32 {
            x + y
        }
        "#
    ));
}

#[test]
fn function_with_string_parameter() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn greet(name: String) -> String {
            format!("Hello, {}!", name)
        }
        "#
    ));
}

#[test]
fn function_with_no_parameters() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn no_params() -> &'static str {
            "No parameters here!"
        }
        "#
    ));
}

#[test]
fn function_with_no_return_type() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn no_return(x: i32) {
            println!("{}", x);
        }
        "#
    ));
}

#[test]
fn generic_function_simple() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn generic_add<T: Add<Output = T>>(x: T, y: T) -> T {
            x + y
        }
        "#
    ));
}

#[test]
fn generic_function_multiple_params() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn complex_fn<T, U>(x: Vec<T>, y: Option<U>) -> Result<T, U> {
            todo!()
        }
        "#
    ));
}

#[test]
fn generic_function_with_complex_bounds() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn bounded_fn<T: Clone + Send, U: Iterator<Item = T>>(data: U) -> Vec<T> {
            data.collect()
        }
        "#
    ));
}

#[test]
#[ignore]
fn function_with_complex_types() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn complex_types(
            callback: fn(i32) -> String, 
            data: Vec<Option<u64>>,
            result: Result<String, Box<dyn std::error::Error>>
        ) -> HashMap<String, Vec<i32>> {
            todo!()
        }
        "#
    ));
}

#[test]
#[ignore]
fn function_with_references_and_lifetimes() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn with_lifetimes<'a>(s: &'a str, data: &'a [u8]) -> &'a str {
            s
        }
        "#
    ));
}

#[test]
fn function_with_mutable_references() {
    insta::assert_snapshot!(expand_function(
        r#"
        fn with_mut_refs(x: &mut i32, y: &mut Vec<String>) -> usize {
            *x += 1;
            y.len()
        }
        "#
    ));
}

#[test]
fn function_with_single_doc_comment() {
    insta::assert_snapshot!(expand_function(
        r#"
        /// Single line documentation
        fn greet(name: String) -> String {
            format!("Hello, {}!", name)
        }
        "#
    ));
}

#[test]
fn function_with_multiple_doc_comments() {
    insta::assert_snapshot!(expand_function(
        r#"
        /// This is a test function
        /// that does addition of two numbers
        fn add(x: i32, y: i32) -> i32 {
            x + y
        }
        "#
    ));
}

#[test]
fn function_with_doc_comments_and_quotes() {
    insta::assert_snapshot!(expand_function(
        r#"
        /// Hello "world", if that is your real name
        fn greet(name: String) -> String {
            format!("Hello, {}...?", name)
        }
        "#
    ));
}

#[test]
fn function_with_complex_doc_comments() {
    insta::assert_snapshot!(expand_function(
        r###"
        /// This uses r#"raw strings"# and r##"nested"## syntax
        fn complex_doc() {
            println!("test");
        }
        "###
    ));
}

#[test]
fn function_with_mixed_attributes() {
    insta::assert_snapshot!(expand_function(
        r#"
        /// Documentation comment
        #[derive(Debug)]
        /// More documentation
        fn mixed_attrs() {
            println!("test");
        }
        "#
    ));
}

#[test]
fn generic_function_with_docs() {
    insta::assert_snapshot!(expand_function(
        r#"
        /// Generic function that adds two values
        fn generic_add<T: Add<Output = T>>(x: T, y: T) -> T {
            x + y
        }
        "#
    ));
}
