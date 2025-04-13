use facet_reflect::ConstValue;

#[test]
fn peek_option() {
    // Test with Some value
    let some_value = Some(42);
    let peek_value = ConstValue::new(&some_value);

    // Convert to option
    let peek_option = peek_value
        .into_option()
        .expect("Should be convertible to option");

    // Check the Some variant methods
    assert!(peek_option.is_some());
    assert!(!peek_option.is_none());

    // Get the inner value
    let inner_value = peek_option.value().expect("Should have a value");
    let value = inner_value.get::<i32>();
    assert_eq!(*value, 42);

    // Test with None value
    let none_value: Option<i32> = None;
    let peek_value = ConstValue::new(&none_value);

    // Convert to option
    let peek_option = peek_value
        .into_option()
        .expect("Should be convertible to option");

    // Check the None variant methods
    assert!(!peek_option.is_some());
    assert!(peek_option.is_none());
    assert!(peek_option.value().is_none());
}
