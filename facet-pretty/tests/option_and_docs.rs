use facet::Facet;
use facet_pretty::PrettyPrinter;
use facet_testhelpers::test;

#[derive(Clone, Facet)]
/// A test struct with doc comments
struct TestStruct {
    /// Field with a doc comment
    field1: i32,
    /// Optional field with a doc comment
    opt_field: Option<String>,
}

#[derive(Clone, Facet)]
#[repr(u8)]
/// A test enum with doc comments
enum TestEnum {
    /// Unit variant with a doc comment
    Unit,
    /// Tuple variant with a doc comment
    #[allow(dead_code)]
    Tuple(i32, String),
    /// Struct variant with a doc comment
    #[allow(dead_code)]
    Struct {
        /// Field in struct variant with a doc comment
        field: bool,
    },
}

#[test]
fn test_option_pretty_printing() {
    // Test Option<String>
    let opt_some = Some("test string".to_string());
    let formatted = PrettyPrinter::new().format(&opt_some);

    // The implementation shows Option types differently
    assert!(formatted.contains("Option"));
    assert!(formatted.contains("test string"));

    // Test Option<i32>
    let opt_none: Option<i32> = None;
    let formatted = PrettyPrinter::new().format(&opt_none);

    // The implementation shows None differently
    assert!(formatted.contains("Option"));
    assert!(formatted.contains("None"));
}

#[test]
fn test_doc_comments_in_pretty_printing() {
    // Create test struct
    let test_struct = TestStruct {
        field1: 42,
        opt_field: Some("test".to_string()),
    };

    let formatted = PrettyPrinter::new().format(&test_struct);

    eprintln!("\n===== TestStruct with doc comments =====\n{}", formatted);

    // Check that field names are present
    assert!(formatted.contains("field1"));
    assert!(formatted.contains("opt_field"));

    // Check that doc comments are present
    assert!(formatted.contains("A test struct with doc comments"));
    assert!(formatted.contains("Field with a doc comment"));
    assert!(formatted.contains("Optional field with a doc comment"));

    // Create test enum instances
    let unit = TestEnum::Unit;
    let tuple = TestEnum::Tuple(123, "hello".to_string());
    let struct_variant = TestEnum::Struct { field: true };

    // Check unit variant formatting
    let formatted = PrettyPrinter::new().format(&unit);
    eprintln!(
        "\n===== TestEnum::Unit with doc comments =====\n{}",
        formatted
    );
    assert!(formatted.contains("TestEnum"));
    assert!(formatted.contains("Unit"));
    assert!(formatted.contains("A test enum with doc comments"));
    assert!(formatted.contains("Unit variant with a doc comment"));

    // Check tuple variant formatting
    let formatted = PrettyPrinter::new().format(&tuple);
    eprintln!(
        "\n===== TestEnum::Tuple with doc comments =====\n{}",
        formatted
    );
    assert!(formatted.contains("TestEnum"));
    assert!(formatted.contains("Tuple"));
    assert!(formatted.contains("A test enum with doc comments"));
    assert!(formatted.contains("Tuple variant with a doc comment"));

    // Check struct variant formatting
    let formatted = PrettyPrinter::new().format(&struct_variant);
    eprintln!(
        "\n===== TestEnum::Struct with doc comments =====\n{}",
        formatted
    );
    assert!(formatted.contains("TestEnum"));
    assert!(formatted.contains("Struct"));
    assert!(formatted.contains("field"));
    assert!(formatted.contains("A test enum with doc comments"));
    assert!(formatted.contains("Struct variant with a doc comment"));
    assert!(formatted.contains("Field in struct variant with a doc comment"));
}
