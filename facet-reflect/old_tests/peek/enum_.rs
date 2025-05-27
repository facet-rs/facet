use facet::Facet;
use facet_reflect::Peek;
use facet_testhelpers::test;

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
    let peek_value = Peek::new(&some_value);

    // Convert to enum and check we can convert to PeekEnum
    let peek_enum = peek_value.into_enum()?;
    let peek_def_not_enum = peek_enum;

    assert!(peek_def_not_enum.variant_name_active()? == "Some");

    // Check if it's the Some variant
    if peek_def_not_enum.variant_name_active()? == "Some" {
        // Get the value field using the field method with index
        let inner_value = peek_def_not_enum.field(0)?.unwrap();
        let value = *inner_value.get::<u32>()?;
        assert_eq!(value, 42);
    } else {
        return Err(eyre::eyre!("Expected Some variant"));
    }

    // Test with None value
    let none_value = DefinitelyNotAnEnum::None;
    let peek_value = Peek::new(&none_value);

    // Convert to enum and check we can convert to PeekEnum
    let peek_enum = peek_value.into_enum()?;
    let peek_def_not_enum = peek_enum;

    assert!(peek_def_not_enum.variant_name_active()? == "None");
    // None variant has no fields to check
}

#[derive(Facet)]
#[repr(C)]
#[allow(dead_code)]
enum ReprCEnum {
    Unit,
    Tuple(u32),
    Struct { a: u8, b: String },
}

#[test]
fn peek_repr_c_enum() {
    // Test with unit
    let unit_value = ReprCEnum::Unit;
    let peek_value = Peek::new(&unit_value);

    // Convert to enum and check we can convert to PeekEnum
    let peek_enum = peek_value.into_enum()?;
    assert!(peek_enum.variant_name_active()? == "Unit");

    // Test with tuple
    let unit_value = ReprCEnum::Tuple(42);
    let peek_value = Peek::new(&unit_value);

    // Convert to enum and check we can convert to PeekEnum
    let peek_enum = peek_value.into_enum()?;

    assert!(peek_enum.variant_name_active()? == "Tuple");
    // Get the value field using the field method with index
    let inner_value = peek_enum.field(0)?.unwrap();
    let value = *inner_value.get::<u32>()?;
    assert_eq!(value, 42);

    // Test with struct
    let unit_value = ReprCEnum::Struct {
        a: 42,
        b: "Hello".to_string(),
    };
    let peek_value = Peek::new(&unit_value);
    // Convert to enum and check we can convert to PeekEnum
    let peek_enum = peek_value.into_enum()?;
    assert!(peek_enum.variant_name_active()? == "Struct");
    // Get the value field using the field method with index
    let inner_value = peek_enum.field(0)?.unwrap();
    let value = *inner_value.get::<u8>()?;
    assert_eq!(value, 42);
    // Get the value field using the field method with name
    let inner_value = peek_enum.field_by_name("b")?.unwrap();
    let value = inner_value.get::<String>()?;
    assert_eq!(value, "Hello");
}
