use core::fmt::Write;
use facet::Facet;
use facet_pretty::{FacetPretty, PrettyPrinter};
use facet_testhelpers::test;
use insta::assert_snapshot;

#[derive(Debug, Facet)]
struct Person {
    name: String,
    age: u32,
    address: Address,
}

#[derive(Debug, Facet)]
struct Address {
    street: String,
    city: String,
    country: String,
}

// Used for testing sensitive fields with a real structure
#[derive(Debug, Facet)]
struct TestSecrets {
    // these are only read through reflection
    #[allow(dead_code)]
    normal_field: String,

    // these are only read through reflection
    #[allow(dead_code)]
    #[facet(sensitive)]
    sensitive_field: String,
}

#[test]
fn test_pretty_print_simple() {
    // This is a simplified test that just ensures the code runs without panicking
    // In a real test, we would verify the output format

    let address = Address {
        street: "123 Main St".to_string(),
        city: "Wonderland".to_string(),
        country: "Imagination".to_string(),
    };

    let person = Person {
        name: "Alice".to_string(),
        age: 30,
        address,
    };

    // Test the PrettyPrinter directly
    let printer = PrettyPrinter::new();
    let output = printer.format(&person);

    eprintln!("{}", output);

    // Ensure the output contains the field names
    assert!(output.contains("name"));
    assert!(output.contains("age"));
    assert!(output.contains("address"));
    assert!(output.contains("street"));
    assert!(output.contains("city"));
    assert!(output.contains("country"));

    // Test the FacetPretty trait
    let mut buffer = String::new();
    write!(buffer, "{}", person.pretty())?;

    // Ensure the output contains the field names
    assert!(buffer.contains("name"));
    assert!(buffer.contains("age"));
    assert!(buffer.contains("address"));
    assert!(buffer.contains("street"));
    assert!(buffer.contains("city"));
    assert!(buffer.contains("country"));

    // Test with custom printer settings
    let custom_printer = PrettyPrinter::new()
        .with_indent_size(4)
        .with_max_depth(3)
        .with_colors(false);

    let custom_output = custom_printer.format(&person);

    // Ensure the output contains the field names
    assert!(custom_output.contains("name"));
    assert!(custom_output.contains("age"));
    assert!(custom_output.contains("address"));

    // Test the FacetPretty trait with custom printer
    let mut custom_buffer = String::new();
    write!(custom_buffer, "{}", person.pretty_with(custom_printer))?;
    // Ensure the output contains the field names
    assert!(custom_buffer.contains("name"));
    assert!(custom_buffer.contains("age"));
    assert!(custom_buffer.contains("address"));
}

#[test]
fn test_sensitive_fields() {
    // Create an actual instance with sensitive data
    let test_data = TestSecrets {
        normal_field: "This is visible".to_string(),
        sensitive_field: "TOP SECRET PASSWORD".to_string(),
    };

    // Test using the PrettyPrinter
    let printer = PrettyPrinter::new();
    let output = printer.format(&test_data);

    eprintln!("{}", output);

    // Verify normal field is visible
    assert!(output.contains("normal_field"));
    assert!(output.contains("This is visible"));

    // Verify sensitive field name is visible but value is redacted
    assert!(output.contains("sensitive_field"));
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("TOP SECRET PASSWORD"));

    // Test pretty with trait
    let mut buffer = String::new();
    write!(buffer, "{}", test_data.pretty())?;

    eprintln!("{}", buffer);

    // Verify normal field is visible
    assert!(buffer.contains("normal_field"));
    assert!(buffer.contains("This is visible"));

    // Verify sensitive field name is visible but value is redacted
    assert!(buffer.contains("sensitive_field"));
    assert!(buffer.contains("[REDACTED]"));
    assert!(!buffer.contains("TOP SECRET PASSWORD"));
}

#[test]
fn test_tuple() {
    let printer = PrettyPrinter::new().with_colors(false);
    assert_eq!(
        printer.format(&(1, 2)),
        r#"
(i32, i32) (
  1,
  2,
)
        "#
        .trim()
    );
}

#[test]
fn test_str() {
    let printer = PrettyPrinter::new().with_colors(false);
    assert_snapshot!(printer.format(&"hello"));
}

#[test]
fn test_vec_u8() {
    let printer = PrettyPrinter::new().with_colors(false);
    let bytes = vec![1u8, 2u8, 3u8, 4u8];
    assert_snapshot!(printer.format(&bytes));
}

#[test]
fn test_byte_slice() {
    let printer = PrettyPrinter::new().with_colors(false);
    let bytes = [1, 2, 3, 4];
    assert_snapshot!(printer.format(&&bytes[..]));
}

#[test]
fn test_vec_u32() {
    let printer = PrettyPrinter::new().with_colors(false);
    let nums = vec![1u32, 2u32, 3u32, 4u32];
    assert_snapshot!(printer.format(&nums));
}

#[test]
fn test_u32_slice() {
    let printer = PrettyPrinter::new().with_colors(false);
    let nums = [1u32, 2u32, 3u32, 4u32];
    assert_snapshot!(printer.format(&&nums[..]));
}
