use facet::Facet;
use facet_json::from_str;

#[test]
fn transparent_string() -> eyre::Result<()> {
    let markup = r#"
        "I look like a string"
    "#;

    let s: String = from_str(markup)?;
    assert_eq!(s, "I look like a string");
    Ok(())
}

#[test]
fn transparent_tuple_struct() -> eyre::Result<()> {
    let markup = r#"
        "I look like a string"
    "#;

    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    struct MyString(String);

    let t: MyString = from_str(markup)?;
    assert_eq!(t.0, "I look like a string".to_string());

    Ok(())
}

#[cfg(feature = "camino")]
#[test]
fn transparent_utf8_path_buf() -> eyre::Result<()> {
    use camino::Utf8PathBuf;

    let markup = r#"
        "/some/test/path"
    "#;

    // Test direct deserialization of Utf8PathBuf
    let path: Utf8PathBuf = from_str(markup)?;
    assert_eq!(path, Utf8PathBuf::from("/some/test/path"));

    Ok(())
}

#[test]
fn transparent_non_zero_u64_with_42_value() -> eyre::Result<()> {
    use std::num::NonZeroU64;

    let markup = r#"
        42
    "#;

    // Test deserialization of NonZeroU64
    let number: NonZeroU64 = from_str(markup)?;
    assert_eq!(number, NonZeroU64::new(42).unwrap());

    Ok(())
}

#[test]
fn transparent_non_zero_u64_with_zero_value() {
    use std::num::NonZeroU64;

    let markup = r#"
        0
    "#;

    // Test deserializing 0 into NonZeroU64, which should fail
    let result: Result<NonZeroU64, _> = from_str(markup);
    assert!(result.is_err());
}

#[test]
fn transparent_arc_string() -> eyre::Result<()> {
    use std::sync::Arc;

    let markup = r#"
        "I'm in an Arc"
    "#;

    // Test deserializing directly into Arc<String>
    let arc_string: Arc<String> = from_str(markup)?;
    assert_eq!(*arc_string, "I'm in an Arc".to_string());

    Ok(())
}

#[test]
fn transparent_option_string() -> eyre::Result<()> {
    let markup = r#"
        "I'm optional"
    "#;

    // Test deserializing a JSON string into Option<String>
    let opt: Option<String> = from_str(markup)?;
    assert_eq!(opt, Some("I'm optional".to_string()));

    Ok(())
}

#[test]
fn transparent_option_non_zero_u64() -> eyre::Result<()> {
    use std::num::NonZeroU64;

    // Test deserializing a valid non-zero value
    let markup = r#"
        10
    "#;
    let opt_num: Option<NonZeroU64> = from_str(markup)?;
    assert_eq!(opt_num, Some(NonZeroU64::new(10).unwrap()));

    // Test deserializing a null into Option<NonZeroU64>, which should yield None
    let markup = r#"
        null
    "#;
    let opt_none: Option<NonZeroU64> = from_str(markup)?;
    assert_eq!(opt_none, None);

    Ok(())
}

#[test]
fn transparent_option_non_zero_u16() -> eyre::Result<()> {
    use std::num::NonZeroU16;

    // Test deserializing a valid non-zero value
    let markup = r#"
        10
    "#;
    let opt_num: Option<NonZeroU16> = from_str(markup)?;
    assert_eq!(opt_num, Some(NonZeroU16::new(10).unwrap()));

    // Test deserializing a null into Option<NonZeroU16>, which should yield None
    let markup = r#"
        null
    "#;
    let opt_none: Option<NonZeroU16> = from_str(markup)?;
    assert_eq!(opt_none, None);

    Ok(())
}

#[cfg(feature = "ordered-float")]
#[test]
fn transparent_ordered_float_f64() -> eyre::Result<()> {
    use ordered_float::OrderedFloat;

    let markup = r#"
        98.4148
    "#;

    // Test deserializing directly into OrderedFloat<f64>
    let float: OrderedFloat<f64> = from_str(markup)?;
    assert_eq!(float, OrderedFloat(98.4148));

    Ok(())
}

#[cfg(feature = "ordered-float")]
#[test]
fn transparent_not_nan_f32() -> eyre::Result<()> {
    use ordered_float::NotNan;

    let markup = r#"
        53.208
    "#;

    // Test deserializing directly into NotNan<f32>
    let not_nan: NotNan<f32> = from_str(markup)?;
    assert_eq!(not_nan, NotNan::new(53.208).unwrap());

    // Test that deserializing a NaN fails
    let markup_nan = r#"
        NaN
    "#;
    let result: Result<NotNan<f32>, _> = from_str(markup_nan);
    assert!(result.is_err());

    Ok(())
}

#[cfg(feature = "uuid")]
#[test]
fn transparent_uuid() -> eyre::Result<()> {
    use uuid::Uuid;

    let markup = r#"
        "f47ac10b-58cc-4372-a567-0e02b2c3d479"
    "#;

    // Test deserializing into Uuid
    let uuid: Uuid = from_str(markup)?;
    assert_eq!(
        uuid,
        Uuid::parse_str("f47ac10b-58cc-4372-a567-0e02b2c3d479")?
    );

    // Test direct usage of Uuid
    let direct_uuid = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000")?;
    assert_eq!(
        direct_uuid.to_string(),
        "123e4567-e89b-12d3-a456-426614174000"
    );

    Ok(())
}
