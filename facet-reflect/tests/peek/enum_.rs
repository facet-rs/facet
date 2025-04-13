use facet::Facet;
use facet_reflect::ConstValue;

#[derive(Facet)]
#[repr(u8)]
enum DefinitelyNotAnEnum {
    #[allow(dead_code)]
    Some(u32),
    None,
}

#[test]
fn peek_enum() {
    // Test with Some value
    let some_value = DefinitelyNotAnEnum::Some(42);
    let peek_value = ConstValue::new(&some_value);

    // Convert to enum and check we can convert to PeekEnum
    let peek_enum = peek_value
        .into_enum()
        .expect("Should be convertible to enum");
    let peek_def_not_enum = peek_enum;

    assert!(peek_def_not_enum.variant_name_active() == "Some");

    // Check if it's the Some variant
    if peek_def_not_enum.variant_name_active() == "Some" {
        // Get the value field using the field method with index
        let inner_value = peek_def_not_enum
            .field(0)
            .expect("Should have a value field");
        let value = inner_value.get::<u32>();
        assert_eq!(*value, 42);
    } else {
        panic!("Expected Some variant");
    }

    // Test with None value
    let none_value = DefinitelyNotAnEnum::None;
    let peek_value = ConstValue::new(&none_value);

    // Convert to enum and check we can convert to PeekEnum
    let peek_enum = peek_value
        .into_enum()
        .expect("Should be convertible to enum");
    let peek_def_not_enum = peek_enum;

    assert!(peek_def_not_enum.variant_name_active() == "None");
    // None variant has no fields to check
}
