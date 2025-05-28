use facet::Facet;
use facet_reflect::Peek;
use facet_testhelpers::test;

#[derive(Facet)]
struct TestStruct {
    number: i32,
    text: String,
}

#[test]
fn peek_struct() {
    // Create test struct instance
    let test_struct = TestStruct {
        number: 42,
        text: "hello".to_string(),
    };
    let peek_value = Peek::new(&test_struct);

    // Convert to struct and check we can convert to PeekStruct
    let peek_struct = peek_value
        .into_struct()
        .expect("Should be convertible to struct");

    // Test field access by name
    let number_field = peek_struct
        .field_by_name("number")
        .expect("Should have a number field");
    let text_field = peek_struct
        .field_by_name("text")
        .expect("Should have a text field");

    // Test field values
    let number_value = number_field.get::<i32>()?;
    assert_eq!(*number_value, 42);

    let text_value = text_field.get::<String>()?;
    assert_eq!(text_value, "hello");
}
