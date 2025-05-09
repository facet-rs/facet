use facet_reflect::{HasFields, Peek};
use owo_colors::OwoColorize;

#[test]
fn peek_option() {
    facet_testhelpers::setup();

    // Test with Some value
    let some_value = Some(42);
    let peek_value = Peek::new(&some_value);

    // Convert to option
    let peek_option = peek_value
        .into_option()
        .expect("Should be convertible to option");

    // Check the Some variant methods
    assert!(peek_option.is_some());
    assert!(!peek_option.is_none());

    // Get the inner value
    let inner_value = peek_option.value().expect("Should have a value");
    let value = inner_value.get::<i32>().unwrap();
    assert_eq!(*value, 42);

    // Test with None value
    let none_value: Option<i32> = None;
    let peek_value = Peek::new(&none_value);

    // Convert to option
    let peek_option = peek_value
        .into_option()
        .expect("Should be convertible to option");

    // Check the None variant methods
    assert!(!peek_option.is_some());
    assert!(peek_option.is_none());
    assert!(peek_option.value().is_none());
}

#[test]
fn peek_option_as_enum() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let opt: Option<String> = Some("IAMA String AMA".into());

    let peek = Peek::new(&opt);
    eprintln!("peek shape: {}", peek.shape().yellow());
    eprintln!("peek type: {:#?}", peek.shape().ty.blue());

    let en = peek.into_enum()?;
    let fields = en.fields_for_serialize().collect::<Vec<_>>();
    assert_eq!(fields.len(), 1);
    let (_field, field_peek) = fields[0];
    eprintln!("field peek shape: {}", field_peek.shape().yellow());
    eprintln!("field peek type: {:#?}", field_peek.shape().ty.blue());
    assert!(field_peek.get::<u32>().is_err());
    assert!(field_peek.get::<&str>().is_err());
    assert!(field_peek.get::<String>().is_ok());
    let s = field_peek.get::<String>().unwrap();
    assert_eq!(s, "IAMA String AMA");

    let opt: Option<String> = None;

    let peek = Peek::new(&opt);

    let en = peek.into_enum()?;
    let fields = en.fields_for_serialize().collect::<Vec<_>>();
    assert_eq!(fields.len(), 1);
    let (_field, field_peek) = fields[0];
    eprintln!("field peek shape: {}", field_peek.shape().yellow());
    eprintln!("field peek type: {:#?}", field_peek.shape().ty.blue());
    assert!(field_peek.get::<u32>().is_err());
    assert!(field_peek.get::<&str>().is_err());
    assert!(field_peek.get::<String>().is_ok());
    let s = field_peek.get::<String>().unwrap();
    assert_eq!(s, "IAMA String AMA");

    Ok(())
}
