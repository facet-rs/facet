//! Tests for TOML values to different forms of options.

use eyre::Result;
use facet::Facet;
use facet_toml::TomlDeErrorKind;

#[test]
fn test_option_scalar() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<i32>,
    }

    assert_eq!(facet_toml::from_str::<Root>("")?, Root { value: None },);
    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
        Root { value: Some(1) },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("value = false")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );

    Ok(())
}

#[test]
fn test_nested_option() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Option<i32>>,
    }

    assert_eq!(facet_toml::from_str::<Root>("")?, Root { value: None },);
    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
        Root {
            value: Some(Some(1))
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("value = false")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );

    Ok(())
}

#[test]
fn test_option_struct() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item {
        value: i32,
    }

    assert_eq!(facet_toml::from_str::<Root>("")?, Root { value: None },);
    assert_eq!(
        facet_toml::from_str::<Root>("value.value = 1")?,
        Root {
            value: Some(Item { value: 1 })
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("value.wrong-key = 2")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedFieldWithName("value")
    );

    Ok(())
}

#[test]
fn test_option_struct_with_option() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item {
        sub: Option<i32>,
    }

    assert_eq!(facet_toml::from_str::<Root>("")?, Root { value: None },);
    assert_eq!(
        facet_toml::from_str::<Root>("value.sub = 1")?,
        Root {
            value: Some(Item { sub: Some(1) })
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("value.sub = false")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );

    Ok(())
}

#[test]
fn test_option_enum() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Item {
        A,
        B(i32),
    }

    assert_eq!(facet_toml::from_str::<Root>("")?, Root { value: None },);
    assert_eq!(
        facet_toml::from_str::<Root>("value = 'A'")?,
        Root {
            value: Some(Item::A)
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value.B = 1")?,
        Root {
            value: Some(Item::B(1))
        },
    );

    assert!(matches!(
        facet_toml::from_str::<Root>("value.non-existing = false")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::GenericReflect(_)
    ));

    Ok(())
}

#[test]
fn test_option_enum_option_scalar() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Root {
        A(Option<String>),
        B { b1: Option<i32>, b2: Option<bool> },
    }

    assert_eq!(
        facet_toml::from_str::<Root>("A = 'hi'")?,
        Root::A(Some("hi".to_owned())),
    );
    assert_eq!(
        facet_toml::from_str::<Root>("B.b1 = 1")?,
        Root::B {
            b1: Some(1),
            b2: None
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("B.b2 = true")?,
        Root::B {
            b1: None,
            b2: Some(true)
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("B = { b1 = 1, b2 = true }")?,
        Root::B {
            b1: Some(1),
            b2: Some(true)
        },
    );
    assert_eq!(facet_toml::from_str::<Root>("[A]")?, Root::A(None),);
    assert_eq!(
        facet_toml::from_str::<Root>("[B]")?,
        Root::B { b1: None, b2: None },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("A = false").unwrap_err().kind,
        TomlDeErrorKind::ExpectedType {
            expected: "string",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("B.b1 = false")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );

    Ok(())
}

#[test]
fn test_option_enum_with_option_variant() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Item {
        A,
        B(Option<i32>),
    }

    assert_eq!(facet_toml::from_str::<Root>("")?, Root { value: None },);
    assert_eq!(
        facet_toml::from_str::<Root>("value = 'A'")?,
        Root {
            value: Some(Item::A)
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value.B = 1")?,
        Root {
            value: Some(Item::B(Some(1)))
        },
    );

    Ok(())
}
