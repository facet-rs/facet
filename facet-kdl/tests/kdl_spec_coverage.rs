//! Comprehensive KDL 2.0.0 Spec Coverage Tests
//!
//! This test suite systematically covers every feature in the KDL 2.0.0 specification
//! to ensure facet-kdl correctly handles all valid KDL syntax.
//!
//! Spec reference: https://kdl.dev/spec/

use facet::Facet;
use facet_kdl::from_str;

// =============================================================================
// Section 3.7: Values
// =============================================================================
// "A value is either: a String, a Number, a Boolean, or Null."

mod values {
    use super::*;
    use facet_kdl as kdl;

    // --- Strings as values ---

    #[derive(Facet, Debug, PartialEq)]
    struct StringArg {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct StringArgDoc {
        #[facet(kdl::child)]
        node: StringArg,
    }

    #[test]
    fn string_value_as_argument() {
        let doc: StringArgDoc = from_str(r#"node "hello""#).unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    #[derive(Facet, Debug, PartialEq)]
    struct StringProp {
        #[facet(kdl::property)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct StringPropDoc {
        #[facet(kdl::child)]
        node: StringProp,
    }

    #[test]
    fn string_value_as_property() {
        let doc: StringPropDoc = from_str(r#"node value="hello""#).unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    // --- Numbers as values ---

    #[derive(Facet, Debug, PartialEq)]
    struct NumberArg {
        #[facet(kdl::argument)]
        value: i64,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct NumberArgDoc {
        #[facet(kdl::child)]
        node: NumberArg,
    }

    #[test]
    fn number_value_as_argument() {
        let doc: NumberArgDoc = from_str(r#"node 42"#).unwrap();
        assert_eq!(doc.node.value, 42);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct NumberProp {
        #[facet(kdl::property)]
        value: i64,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct NumberPropDoc {
        #[facet(kdl::child)]
        node: NumberProp,
    }

    #[test]
    fn number_value_as_property() {
        let doc: NumberPropDoc = from_str(r#"node value=42"#).unwrap();
        assert_eq!(doc.node.value, 42);
    }

    // --- Booleans as values ---

    #[derive(Facet, Debug, PartialEq)]
    struct BoolArg {
        #[facet(kdl::argument)]
        value: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct BoolArgDoc {
        #[facet(kdl::child)]
        node: BoolArg,
    }

    #[test]
    fn bool_true_as_argument() {
        let doc: BoolArgDoc = from_str(r#"node #true"#).unwrap();
        assert!(doc.node.value);
    }

    #[test]
    fn bool_false_as_argument() {
        let doc: BoolArgDoc = from_str(r#"node #false"#).unwrap();
        assert!(!doc.node.value);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct BoolProp {
        #[facet(kdl::property)]
        value: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct BoolPropDoc {
        #[facet(kdl::child)]
        node: BoolProp,
    }

    #[test]
    fn bool_true_as_property() {
        let doc: BoolPropDoc = from_str(r#"node value=#true"#).unwrap();
        assert!(doc.node.value);
    }

    #[test]
    fn bool_false_as_property() {
        let doc: BoolPropDoc = from_str(r#"node value=#false"#).unwrap();
        assert!(!doc.node.value);
    }

    // --- Null as value ---

    #[derive(Facet, Debug, PartialEq)]
    struct NullArg {
        #[facet(kdl::argument)]
        value: Option<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct NullArgDoc {
        #[facet(kdl::child)]
        node: NullArg,
    }

    #[test]
    fn null_as_argument() {
        let doc: NullArgDoc = from_str(r#"node #null"#).unwrap();
        assert_eq!(doc.node.value, None);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct NullProp {
        #[facet(kdl::property)]
        value: Option<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct NullPropDoc {
        #[facet(kdl::child)]
        node: NullProp,
    }

    #[test]
    fn null_as_property() {
        let doc: NullPropDoc = from_str(r#"node value=#null"#).unwrap();
        assert_eq!(doc.node.value, None);
    }
}

// =============================================================================
// Section 3.9-3.13: Strings
// =============================================================================

mod strings {
    use super::*;
    use facet_kdl as kdl;

    #[derive(Facet, Debug, PartialEq)]
    struct SingleString {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SingleStringDoc {
        #[facet(kdl::child)]
        node: SingleString,
    }

    // --- 3.10: Identifier Strings ---

    #[test]
    fn identifier_string_simple() {
        let doc: SingleStringDoc = from_str(r#"node foo"#).unwrap();
        assert_eq!(doc.node.value, "foo");
    }

    #[test]
    fn identifier_string_with_hyphen() {
        let doc: SingleStringDoc = from_str(r#"node my-identifier"#).unwrap();
        assert_eq!(doc.node.value, "my-identifier");
    }

    #[test]
    fn identifier_string_with_underscore() {
        let doc: SingleStringDoc = from_str(r#"node my_identifier"#).unwrap();
        assert_eq!(doc.node.value, "my_identifier");
    }

    #[test]
    fn identifier_string_starting_with_underscore() {
        let doc: SingleStringDoc = from_str(r#"node _private"#).unwrap();
        assert_eq!(doc.node.value, "_private");
    }

    #[test]
    fn identifier_string_unicode() {
        let doc: SingleStringDoc = from_str(r#"node café"#).unwrap();
        assert_eq!(doc.node.value, "café");
    }

    #[test]
    fn identifier_string_signed_ident() {
        // Identifiers can start with - or + if not followed by digit
        let doc: SingleStringDoc = from_str(r#"node --flag"#).unwrap();
        assert_eq!(doc.node.value, "--flag");
    }

    // --- 3.11: Quoted Strings ---

    #[test]
    fn quoted_string_simple() {
        let doc: SingleStringDoc = from_str(r#"node "hello world""#).unwrap();
        assert_eq!(doc.node.value, "hello world");
    }

    #[test]
    fn quoted_string_empty() {
        let doc: SingleStringDoc = from_str(r#"node """#).unwrap();
        assert_eq!(doc.node.value, "");
    }

    #[test]
    fn quoted_string_with_spaces() {
        let doc: SingleStringDoc = from_str(r#"node "  spaces  ""#).unwrap();
        assert_eq!(doc.node.value, "  spaces  ");
    }

    // --- 3.11.1: Escapes ---

    #[test]
    fn escape_newline() {
        let doc: SingleStringDoc = from_str(r#"node "line1\nline2""#).unwrap();
        assert_eq!(doc.node.value, "line1\nline2");
    }

    #[test]
    fn escape_carriage_return() {
        let doc: SingleStringDoc = from_str(r#"node "a\rb""#).unwrap();
        assert_eq!(doc.node.value, "a\rb");
    }

    #[test]
    fn escape_tab() {
        let doc: SingleStringDoc = from_str(r#"node "a\tb""#).unwrap();
        assert_eq!(doc.node.value, "a\tb");
    }

    #[test]
    fn escape_backslash() {
        let doc: SingleStringDoc = from_str(r#"node "a\\b""#).unwrap();
        assert_eq!(doc.node.value, "a\\b");
    }

    #[test]
    fn escape_quote() {
        let doc: SingleStringDoc = from_str(r#"node "say \"hello\"""#).unwrap();
        assert_eq!(doc.node.value, "say \"hello\"");
    }

    #[test]
    fn escape_backspace() {
        let doc: SingleStringDoc = from_str(r#"node "a\bb""#).unwrap();
        assert_eq!(doc.node.value, "a\x08b");
    }

    #[test]
    fn escape_form_feed() {
        let doc: SingleStringDoc = from_str(r#"node "a\fb""#).unwrap();
        assert_eq!(doc.node.value, "a\x0Cb");
    }

    #[test]
    fn escape_space() {
        let doc: SingleStringDoc = from_str(r#"node "a\sb""#).unwrap();
        assert_eq!(doc.node.value, "a b");
    }

    #[test]
    fn escape_unicode() {
        let doc: SingleStringDoc = from_str(r#"node "\u{1F600}""#).unwrap();
        assert_eq!(doc.node.value, "😀");
    }

    #[test]
    fn escape_unicode_lowercase() {
        let doc: SingleStringDoc = from_str(r#"node "\u{e9}""#).unwrap();
        assert_eq!(doc.node.value, "é");
    }

    #[test]
    fn escape_whitespace() {
        // Whitespace escape discards literal whitespace
        let doc: SingleStringDoc = from_str("node \"hello\\   world\"").unwrap();
        assert_eq!(doc.node.value, "helloworld");
    }

    // --- 3.12: Multi-line Strings ---

    #[test]
    fn multiline_string_basic() {
        let input = r#"node """
    hello
    world
    """"#;
        let doc: SingleStringDoc = from_str(input).unwrap();
        assert_eq!(doc.node.value, "hello\nworld");
    }

    #[test]
    fn multiline_string_with_indentation() {
        let input = r#"node """
        indented
    base
        also indented
    """"#;
        let doc: SingleStringDoc = from_str(input).unwrap();
        assert_eq!(doc.node.value, "    indented\nbase\n    also indented");
    }

    #[test]
    fn multiline_string_empty() {
        let input = "node \"\"\"\n\"\"\"";
        let doc: SingleStringDoc = from_str(input).unwrap();
        assert_eq!(doc.node.value, "");
    }

    #[test]
    fn multiline_string_single_line_content() {
        let input = "node \"\"\"\n    single\n    \"\"\"";
        let doc: SingleStringDoc = from_str(input).unwrap();
        assert_eq!(doc.node.value, "single");
    }

    // --- 3.13: Raw Strings ---

    #[test]
    fn raw_string_basic() {
        let doc: SingleStringDoc = from_str(r##"node #"hello"#"##).unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    #[test]
    fn raw_string_no_escapes() {
        let doc: SingleStringDoc = from_str(r##"node #"no\nescapes"#"##).unwrap();
        assert_eq!(doc.node.value, "no\\nescapes");
    }

    #[test]
    fn raw_string_with_quotes() {
        let doc: SingleStringDoc = from_str(r##"node #"has "quotes" inside"#"##).unwrap();
        assert_eq!(doc.node.value, "has \"quotes\" inside");
    }

    #[test]
    fn raw_string_double_hash() {
        let doc: SingleStringDoc = from_str(r###"node ##"contains "# here"##"###).unwrap();
        assert_eq!(doc.node.value, "contains \"# here");
    }

    #[test]
    fn raw_string_multiline() {
        let input = r###"node #"""
    raw multiline
    no escapes \n
    """#"###;
        let doc: SingleStringDoc = from_str(input).unwrap();
        assert_eq!(doc.node.value, "raw multiline\nno escapes \\n");
    }
}

// =============================================================================
// Section 3.14: Numbers
// =============================================================================

mod numbers {
    use super::*;
    use facet_kdl as kdl;

    #[derive(Facet, Debug, PartialEq)]
    struct IntArg {
        #[facet(kdl::argument)]
        value: i64,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct IntArgDoc {
        #[facet(kdl::child)]
        node: IntArg,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FloatArg {
        #[facet(kdl::argument)]
        value: f64,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FloatArgDoc {
        #[facet(kdl::child)]
        node: FloatArg,
    }

    // --- Decimal integers ---

    #[test]
    fn decimal_positive() {
        let doc: IntArgDoc = from_str(r#"node 123"#).unwrap();
        assert_eq!(doc.node.value, 123);
    }

    #[test]
    fn decimal_negative() {
        let doc: IntArgDoc = from_str(r#"node -456"#).unwrap();
        assert_eq!(doc.node.value, -456);
    }

    #[test]
    fn decimal_explicit_positive() {
        let doc: IntArgDoc = from_str(r#"node +789"#).unwrap();
        assert_eq!(doc.node.value, 789);
    }

    #[test]
    fn decimal_zero() {
        let doc: IntArgDoc = from_str(r#"node 0"#).unwrap();
        assert_eq!(doc.node.value, 0);
    }

    #[test]
    fn decimal_with_underscores() {
        let doc: IntArgDoc = from_str(r#"node 1_000_000"#).unwrap();
        assert_eq!(doc.node.value, 1_000_000);
    }

    // --- Decimal floats ---

    #[test]
    fn float_simple() {
        let doc: FloatArgDoc = from_str(r#"node 3.5"#).unwrap();
        assert!((doc.node.value - 3.5).abs() < 1e-10);
    }

    #[test]
    fn float_negative() {
        let doc: FloatArgDoc = from_str(r#"node -2.5"#).unwrap();
        assert!((doc.node.value - (-2.5)).abs() < 1e-10);
    }

    #[test]
    fn float_leading_zero() {
        let doc: FloatArgDoc = from_str(r#"node 0.5"#).unwrap();
        assert!((doc.node.value - 0.5).abs() < 1e-10);
    }

    #[test]
    fn float_with_exponent() {
        let doc: FloatArgDoc = from_str(r#"node 1.5e10"#).unwrap();
        assert!((doc.node.value - 1.5e10).abs() < 1e5);
    }

    #[test]
    fn float_with_negative_exponent() {
        let doc: FloatArgDoc = from_str(r#"node 1.5e-10"#).unwrap();
        assert!((doc.node.value - 1.5e-10).abs() < 1e-15);
    }

    #[test]
    fn float_with_uppercase_exponent() {
        let doc: FloatArgDoc = from_str(r#"node 1.5E10"#).unwrap();
        assert!((doc.node.value - 1.5e10).abs() < 1e5);
    }

    #[test]
    fn float_integer_with_exponent() {
        let doc: FloatArgDoc = from_str(r#"node 5e3"#).unwrap();
        assert!((doc.node.value - 5000.0).abs() < 1e-10);
    }

    // --- Binary numbers ---

    #[test]
    fn binary_basic() {
        let doc: IntArgDoc = from_str(r#"node 0b1010"#).unwrap();
        assert_eq!(doc.node.value, 0b1010);
    }

    #[test]
    fn binary_with_underscores() {
        let doc: IntArgDoc = from_str(r#"node 0b1111_0000"#).unwrap();
        assert_eq!(doc.node.value, 0b1111_0000);
    }

    #[test]
    fn binary_negative() {
        let doc: IntArgDoc = from_str(r#"node -0b1010"#).unwrap();
        assert_eq!(doc.node.value, -0b1010);
    }

    // --- Octal numbers ---

    #[test]
    fn octal_basic() {
        let doc: IntArgDoc = from_str(r#"node 0o755"#).unwrap();
        assert_eq!(doc.node.value, 0o755);
    }

    #[test]
    fn octal_with_underscores() {
        let doc: IntArgDoc = from_str(r#"node 0o7_5_5"#).unwrap();
        assert_eq!(doc.node.value, 0o755);
    }

    #[test]
    fn octal_negative() {
        let doc: IntArgDoc = from_str(r#"node -0o755"#).unwrap();
        assert_eq!(doc.node.value, -0o755);
    }

    // --- Hexadecimal numbers ---

    #[test]
    fn hex_lowercase() {
        let doc: IntArgDoc = from_str(r#"node 0xff"#).unwrap();
        assert_eq!(doc.node.value, 0xff);
    }

    #[test]
    fn hex_uppercase() {
        let doc: IntArgDoc = from_str(r#"node 0xFF"#).unwrap();
        assert_eq!(doc.node.value, 0xFF);
    }

    #[test]
    fn hex_mixed_case() {
        let doc: IntArgDoc = from_str(r#"node 0xDeAdBeEf"#).unwrap();
        assert_eq!(doc.node.value, 0xDEADBEEF);
    }

    #[test]
    fn hex_with_underscores() {
        let doc: IntArgDoc = from_str(r#"node 0xff_00_ff"#).unwrap();
        assert_eq!(doc.node.value, 0xff00ff);
    }

    #[test]
    fn hex_negative() {
        let doc: IntArgDoc = from_str(r#"node -0xff"#).unwrap();
        assert_eq!(doc.node.value, -0xff);
    }

    // --- Keyword numbers (3.14.1) ---

    #[test]
    fn keyword_infinity() {
        let doc: FloatArgDoc = from_str(r#"node #inf"#).unwrap();
        assert!(doc.node.value.is_infinite() && doc.node.value.is_sign_positive());
    }

    #[test]
    fn keyword_negative_infinity() {
        let doc: FloatArgDoc = from_str(r#"node #-inf"#).unwrap();
        assert!(doc.node.value.is_infinite() && doc.node.value.is_sign_negative());
    }

    #[test]
    fn keyword_nan() {
        let doc: FloatArgDoc = from_str(r#"node #nan"#).unwrap();
        assert!(doc.node.value.is_nan());
    }
}

// =============================================================================
// Section 3.2: Nodes
// =============================================================================

mod nodes {
    use super::*;
    use facet_kdl as kdl;

    // --- Basic node with name ---

    #[derive(Facet, Debug, PartialEq)]
    struct EmptyNode {}

    #[derive(Facet, Debug, PartialEq)]
    struct EmptyNodeDoc {
        #[facet(kdl::child)]
        node: EmptyNode,
    }

    #[test]
    fn node_bare_name() {
        let _doc: EmptyNodeDoc = from_str(r#"node"#).unwrap();
    }

    // --- Node names as strings ---

    #[derive(Facet, Debug, PartialEq)]
    struct CapturesName {
        #[facet(kdl::node_name)]
        name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct CapturesNameDoc {
        #[facet(kdl::children)]
        nodes: Vec<CapturesName>,
    }

    #[test]
    fn node_name_identifier() {
        let doc: CapturesNameDoc = from_str(r#"my-node"#).unwrap();
        assert_eq!(doc.nodes[0].name, "my-node");
    }

    #[test]
    fn node_name_quoted() {
        let doc: CapturesNameDoc = from_str(r#""quoted node name""#).unwrap();
        assert_eq!(doc.nodes[0].name, "quoted node name");
    }

    #[test]
    fn node_name_with_spaces_quoted() {
        let doc: CapturesNameDoc = from_str(r#""node with spaces""#).unwrap();
        assert_eq!(doc.nodes[0].name, "node with spaces");
    }

    // --- Semicolon terminator ---

    #[derive(Facet, Debug, PartialEq)]
    struct SimpleArg {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct MultiNodeDoc {
        #[facet(kdl::children)]
        nodes: Vec<SimpleArg>,
    }

    #[test]
    fn multiple_nodes_semicolon_separated() {
        let doc: MultiNodeDoc = from_str(r#"node "a"; node "b"; node "c""#).unwrap();
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(doc.nodes[0].value, "a");
        assert_eq!(doc.nodes[1].value, "b");
        assert_eq!(doc.nodes[2].value, "c");
    }

    #[test]
    fn multiple_nodes_newline_separated() {
        let doc: MultiNodeDoc = from_str("node \"a\"\nnode \"b\"\nnode \"c\"").unwrap();
        assert_eq!(doc.nodes.len(), 3);
    }
}

// =============================================================================
// Section 3.3: Line Continuation
// =============================================================================

mod line_continuation {
    use super::*;
    use facet_kdl as kdl;

    #[derive(Facet, Debug, PartialEq)]
    struct ThreeArgs {
        #[facet(kdl::arguments)]
        values: Vec<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ThreeArgsDoc {
        #[facet(kdl::child)]
        node: ThreeArgs,
    }

    #[test]
    fn line_continuation_basic() {
        let input = "node \"a\" \\\n\"b\" \\\n\"c\"";
        let doc: ThreeArgsDoc = from_str(input).unwrap();
        assert_eq!(doc.node.values, vec!["a", "b", "c"]);
    }

    #[test]
    fn line_continuation_with_comment() {
        let input = "node \"a\" \\ // comment\n\"b\"";
        let doc: ThreeArgsDoc = from_str(input).unwrap();
        assert_eq!(doc.node.values, vec!["a", "b"]);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ArgAndProp {
        #[facet(kdl::argument)]
        arg: String,
        #[facet(kdl::property)]
        key: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ArgAndPropDoc {
        #[facet(kdl::child)]
        node: ArgAndProp,
    }

    #[test]
    fn line_continuation_with_properties() {
        let input = "node \"arg\" \\\n    key=\"value\"";
        let doc: ArgAndPropDoc = from_str(input).unwrap();
        assert_eq!(doc.node.arg, "arg");
        assert_eq!(doc.node.key, "value");
    }
}

// =============================================================================
// Section 3.4: Properties
// =============================================================================

mod properties {
    use super::*;
    use facet_kdl as kdl;

    #[derive(Facet, Debug, PartialEq)]
    struct MultiProps {
        #[facet(kdl::property)]
        a: i32,
        #[facet(kdl::property)]
        b: String,
        #[facet(kdl::property)]
        c: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct MultiPropsDoc {
        #[facet(kdl::child)]
        node: MultiProps,
    }

    #[test]
    fn multiple_properties() {
        let doc: MultiPropsDoc = from_str(r#"node a=1 b="two" c=#true"#).unwrap();
        assert_eq!(doc.node.a, 1);
        assert_eq!(doc.node.b, "two");
        assert!(doc.node.c);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DuplicateProp {
        #[facet(kdl::property)]
        a: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DuplicatePropDoc {
        #[facet(kdl::child)]
        node: DuplicateProp,
    }

    #[test]
    fn duplicate_property_rightmost_wins() {
        // "rightmost properties with identical names override earlier properties"
        let doc: DuplicatePropDoc = from_str(r#"node a=1 a=2"#).unwrap();
        assert_eq!(doc.node.a, 2);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct QuotedKeyProp {
        #[facet(kdl::property, rename = "my key")]
        my_key: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct QuotedKeyPropDoc {
        #[facet(kdl::child)]
        node: QuotedKeyProp,
    }

    #[test]
    fn property_with_quoted_key() {
        let doc: QuotedKeyPropDoc = from_str(r#"node "my key"="value""#).unwrap();
        assert_eq!(doc.node.my_key, "value");
    }
}

// =============================================================================
// Section 3.5: Arguments
// =============================================================================

mod arguments {
    use super::*;
    use facet_kdl as kdl;

    // --- Single argument ---

    #[derive(Facet, Debug, PartialEq)]
    struct SingleArg {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SingleArgDoc {
        #[facet(kdl::child)]
        node: SingleArg,
    }

    #[test]
    fn single_argument() {
        let doc: SingleArgDoc = from_str(r#"node "hello""#).unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    // --- Multiple arguments (kdl::arguments) ---

    #[derive(Facet, Debug, PartialEq)]
    struct MultiArgs {
        #[facet(kdl::arguments)]
        values: Vec<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct MultiArgsDoc {
        #[facet(kdl::child)]
        node: MultiArgs,
    }

    #[test]
    fn multiple_arguments_strings() {
        let doc: MultiArgsDoc = from_str(r#"node "a" "b" "c""#).unwrap();
        assert_eq!(doc.node.values, vec!["a", "b", "c"]);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct MultiArgsWithDefault {
        #[facet(kdl::arguments, default)]
        values: Vec<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct MultiArgsWithDefaultDoc {
        #[facet(kdl::child)]
        node: MultiArgsWithDefault,
    }

    #[test]
    fn multiple_arguments_empty() {
        let doc: MultiArgsWithDefaultDoc = from_str(r#"node"#).unwrap();
        assert!(doc.node.values.is_empty());
    }

    #[test]
    fn multiple_arguments_single() {
        let doc: MultiArgsDoc = from_str(r#"node "only""#).unwrap();
        assert_eq!(doc.node.values, vec!["only"]);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct MultiArgsInt {
        #[facet(kdl::arguments)]
        values: Vec<i32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct MultiArgsIntDoc {
        #[facet(kdl::child)]
        node: MultiArgsInt,
    }

    #[test]
    fn multiple_arguments_integers() {
        let doc: MultiArgsIntDoc = from_str(r#"node 1 2 3 4 5"#).unwrap();
        assert_eq!(doc.node.values, vec![1, 2, 3, 4, 5]);
    }

    // --- Arguments with properties mixed ---

    #[derive(Facet, Debug, PartialEq)]
    struct ArgsAndProps {
        #[facet(kdl::arguments)]
        args: Vec<String>,
        #[facet(kdl::property)]
        key: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ArgsAndPropsDoc {
        #[facet(kdl::child)]
        node: ArgsAndProps,
    }

    #[test]
    fn arguments_interspersed_with_properties() {
        // "Arguments and Properties may be interspersed in any order"
        let doc: ArgsAndPropsDoc = from_str(r#"node "a" key="value" "b""#).unwrap();
        assert_eq!(doc.node.args, vec!["a", "b"]);
        assert_eq!(doc.node.key, "value");
    }

    #[test]
    fn arguments_order_preserved() {
        // "Arguments are ordered relative to each other and that order must be preserved"
        let doc: ArgsAndPropsDoc = from_str(r#"node "first" key="x" "second" "third""#).unwrap();
        assert_eq!(doc.node.args, vec!["first", "second", "third"]);
    }
}

// =============================================================================
// Section 3.6: Children Block
// =============================================================================

mod children {
    use super::*;
    use facet_kdl as kdl;

    #[derive(Facet, Debug, PartialEq)]
    struct Child {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Parent {
        #[facet(kdl::children)]
        children: Vec<Child>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ParentDoc {
        #[facet(kdl::child)]
        parent: Parent,
    }

    #[test]
    fn children_basic() {
        let input = r#"parent {
            child "a"
            child "b"
        }"#;
        let doc: ParentDoc = from_str(input).unwrap();
        assert_eq!(doc.parent.children.len(), 2);
        assert_eq!(doc.parent.children[0].value, "a");
        assert_eq!(doc.parent.children[1].value, "b");
    }

    #[test]
    fn children_single_line_semicolons() {
        let doc: ParentDoc = from_str(r#"parent { child "a"; child "b"; child "c" }"#).unwrap();
        assert_eq!(doc.parent.children.len(), 3);
    }

    #[test]
    fn children_empty() {
        let doc: ParentDoc = from_str(r#"parent {}"#).unwrap();
        assert!(doc.parent.children.is_empty());
    }

    // --- Nested children ---

    #[derive(Facet, Debug, PartialEq)]
    struct Level2 {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level1 {
        #[facet(kdl::children)]
        items: Vec<Level2>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level0 {
        #[facet(kdl::child)]
        level1: Level1,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level0Doc {
        #[facet(kdl::child)]
        level0: Level0,
    }

    #[test]
    fn deeply_nested_children() {
        let input = r#"level0 {
            level1 {
                level2 "deep"
            }
        }"#;
        let doc: Level0Doc = from_str(input).unwrap();
        assert_eq!(doc.level0.level1.items[0].value, "deep");
    }
}

// =============================================================================
// Section 3.8: Type Annotations
// =============================================================================

mod type_annotations {
    use super::*;
    use facet_kdl as kdl;

    // Type annotations are hints - facet-kdl should parse them but may not enforce

    #[derive(Facet, Debug, PartialEq)]
    struct TypedArg {
        #[facet(kdl::argument)]
        value: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct TypedArgDoc {
        #[facet(kdl::child)]
        node: TypedArg,
    }

    #[test]
    fn type_annotation_on_argument() {
        let doc: TypedArgDoc = from_str(r#"node (u8)255"#).unwrap();
        assert_eq!(doc.node.value, 255);
    }

    #[test]
    fn type_annotation_on_argument_with_spaces() {
        // "It may contain Whitespace after the ( and before the )"
        let doc: TypedArgDoc = from_str(r#"node ( u8 )255"#).unwrap();
        assert_eq!(doc.node.value, 255);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct TypedProp {
        #[facet(kdl::property)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct TypedPropDoc {
        #[facet(kdl::child)]
        node: TypedProp,
    }

    #[test]
    fn type_annotation_on_property_value() {
        let doc: TypedPropDoc = from_str(r#"node value=(date)"2024-01-01""#).unwrap();
        assert_eq!(doc.node.value, "2024-01-01");
    }

    // Type annotation on node name

    #[derive(Facet, Debug, PartialEq)]
    struct TypedNode {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct TypedNodeDoc {
        #[facet(kdl::child)]
        date: TypedNode,
    }

    #[test]
    fn type_annotation_on_node() {
        // "(published)date" - type annotation clarifies context
        let doc: TypedNodeDoc = from_str(r#"(published)date "2024-01-01""#).unwrap();
        assert_eq!(doc.date.value, "2024-01-01");
    }
}

// =============================================================================
// Section 3.17: Comments
// =============================================================================

mod comments {
    use super::*;
    use facet_kdl as kdl;

    #[derive(Facet, Debug, PartialEq)]
    struct SingleArg {
        #[facet(kdl::argument)]
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SingleArgDoc {
        #[facet(kdl::child)]
        node: SingleArg,
    }

    // --- 3.17.1: Single-line comments ---

    #[test]
    fn single_line_comment_after_node() {
        let doc: SingleArgDoc = from_str("node \"hello\" // this is a comment").unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    #[test]
    fn single_line_comment_own_line() {
        let input = "// comment\nnode \"hello\"";
        let doc: SingleArgDoc = from_str(input).unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    // --- 3.17.2: Multi-line comments ---

    #[test]
    fn multiline_comment_inline() {
        let doc: SingleArgDoc = from_str(r#"node /* comment */ "hello""#).unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    #[test]
    fn multiline_comment_spanning_lines() {
        let input = "node /* this\nspans\nlines */ \"hello\"";
        let doc: SingleArgDoc = from_str(input).unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    #[test]
    fn multiline_comment_nested() {
        // "These comments can span multiple lines. They are allowed in all positions
        //  where Whitespace is allowed and can be nested."
        let doc: SingleArgDoc = from_str(r#"node /* outer /* inner */ outer */ "hello""#).unwrap();
        assert_eq!(doc.node.value, "hello");
    }

    // --- 3.17.3: Slashdash comments ---

    #[derive(Facet, Debug, PartialEq)]
    struct TwoArgs {
        #[facet(kdl::arguments)]
        values: Vec<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct TwoArgsDoc {
        #[facet(kdl::child)]
        node: TwoArgs,
    }

    #[test]
    fn slashdash_argument() {
        let doc: TwoArgsDoc = from_str(r#"node /- "removed" "kept""#).unwrap();
        assert_eq!(doc.node.values, vec!["kept"]);
    }

    #[test]
    fn slashdash_multiple_arguments() {
        let doc: TwoArgsDoc = from_str(r#"node "a" /- "b" "c" /- "d" "e""#).unwrap();
        assert_eq!(doc.node.values, vec!["a", "c", "e"]);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct PropNode {
        #[facet(kdl::property)]
        kept: String,
        #[facet(kdl::property, default)]
        removed: Option<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct PropNodeDoc {
        #[facet(kdl::child)]
        node: PropNode,
    }

    #[test]
    fn slashdash_property() {
        let doc: PropNodeDoc = from_str(r#"node /- removed="gone" kept="here""#).unwrap();
        assert_eq!(doc.node.kept, "here");
        assert_eq!(doc.node.removed, None);
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ParentNode {
        #[facet(kdl::children, default)]
        children: Vec<SingleArg>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ParentNodeDoc {
        #[facet(kdl::child)]
        parent: ParentNode,
    }

    #[test]
    fn slashdash_children_block() {
        let doc: ParentNodeDoc = from_str(r#"parent /- { node "ignored" }"#).unwrap();
        assert!(doc.parent.children.is_empty());
    }

    #[derive(Facet, Debug, PartialEq)]
    struct MultiNodeDoc {
        #[facet(kdl::children)]
        nodes: Vec<SingleArg>,
    }

    #[test]
    fn slashdash_entire_node() {
        let input = "/- node \"removed\"\nnode \"kept\"";
        let doc: MultiNodeDoc = from_str(input).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0].value, "kept");
    }

    #[test]
    fn slashdash_with_whitespace() {
        // "A slashdash may be be followed by any amount of whitespace, including
        //  newlines and comments"
        let input = "node /-   \n  \"removed\" \"kept\"";
        let doc: TwoArgsDoc = from_str(input).unwrap();
        assert_eq!(doc.node.values, vec!["kept"]);
    }
}

// =============================================================================
// Additional edge cases and integration tests
// =============================================================================

mod edge_cases {
    use super::*;
    use facet_kdl as kdl;

    // --- Empty document ---

    #[derive(Facet, Debug, PartialEq, Default)]
    #[facet(traits(Default))]
    struct EmptyDoc {
        #[facet(kdl::children, default)]
        nodes: Vec<()>,
    }

    #[test]
    fn empty_document() {
        let doc: EmptyDoc = from_str("").unwrap();
        assert!(doc.nodes.is_empty());
    }

    #[test]
    fn whitespace_only_document() {
        let doc: EmptyDoc = from_str("   \n\n   \t  ").unwrap();
        assert!(doc.nodes.is_empty());
    }

    #[test]
    fn comments_only_document() {
        let doc: EmptyDoc = from_str("// just a comment\n/* another */").unwrap();
        assert!(doc.nodes.is_empty());
    }

    // --- Property key edge cases ---

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct KebabProps {
        #[facet(kdl::property)]
        my_property: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct KebabPropsDoc {
        #[facet(kdl::child)]
        node: KebabProps,
    }

    #[test]
    fn rename_all_kebab_case() {
        let doc: KebabPropsDoc = from_str(r#"node my-property="value""#).unwrap();
        assert_eq!(doc.node.my_property, "value");
    }

    // --- Large numbers ---

    #[derive(Facet, Debug, PartialEq)]
    struct LargeNum {
        #[facet(kdl::argument)]
        value: i64,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargeNumDoc {
        #[facet(kdl::child)]
        node: LargeNum,
    }

    #[test]
    fn large_positive_number() {
        let doc: LargeNumDoc = from_str(r#"node 9223372036854775807"#).unwrap();
        assert_eq!(doc.node.value, i64::MAX);
    }

    #[test]
    fn large_negative_number() {
        let doc: LargeNumDoc = from_str(r#"node -9223372036854775808"#).unwrap();
        assert_eq!(doc.node.value, i64::MIN);
    }

    // --- Unicode in various positions ---

    #[derive(Facet, Debug, PartialEq)]
    struct UnicodeNode {
        #[facet(kdl::node_name)]
        name: String,
        #[facet(kdl::argument)]
        arg: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct UnicodeNodeDoc {
        #[facet(kdl::children)]
        nodes: Vec<UnicodeNode>,
    }

    #[test]
    fn unicode_node_name() {
        let doc: UnicodeNodeDoc = from_str(r#"日本語 "hello""#).unwrap();
        assert_eq!(doc.nodes[0].name, "日本語");
    }

    #[test]
    fn unicode_argument() {
        let doc: UnicodeNodeDoc = from_str(r#"node "こんにちは""#).unwrap();
        assert_eq!(doc.nodes[0].arg, "こんにちは");
    }

    #[test]
    fn emoji_in_string() {
        let doc: UnicodeNodeDoc = from_str(r#"node "hello 👋 world 🌍""#).unwrap();
        assert_eq!(doc.nodes[0].arg, "hello 👋 world 🌍");
    }
}
