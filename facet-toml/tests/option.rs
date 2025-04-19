//! Tests for TOML values to different forms of options.

use facet::Facet;

#[test]
fn test_option_scalar() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<i32>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("").expect("Failed to parse TOML"),
        Root { value: None },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: Some(1) },
    );
}

#[test]
fn test_nested_option() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Option<i32>>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("").expect("Failed to parse TOML"),
        Root { value: None },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: Some(Some(1))
        },
    );
}

#[test]
fn test_option_struct() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item {
        value: i32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("").expect("Failed to parse TOML"),
        Root { value: None },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value.value = 1").expect("Failed to parse TOML"),
        Root {
            value: Some(Item { value: 1 })
        },
    );
}

#[test]
fn test_option_struct_with_option() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item {
        sub: Option<i32>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("").expect("Failed to parse TOML"),
        Root { value: None },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value.sub = 1").expect("Failed to parse TOML"),
        Root {
            value: Some(Item { sub: Some(1) })
        },
    );
}

#[test]
fn test_option_enum() {
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

    assert_eq!(
        facet_toml::from_str::<Root>("").expect("Failed to parse TOML"),
        Root { value: None },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 'A'").expect("Failed to parse TOML"),
        Root {
            value: Some(Item::A)
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value.B = 1").expect("Failed to parse TOML"),
        Root {
            value: Some(Item::B(1))
        },
    );
}

#[test]
fn test_option_enum_option_scalar() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Root {
        A(Option<String>),
        B { b1: Option<i32>, b2: Option<bool> },
    }

    assert_eq!(
        facet_toml::from_str::<Root>("A = 'hi'").expect("Failed to parse TOML"),
        Root::A(Some("hi".to_owned())),
    );
    assert_eq!(
        facet_toml::from_str::<Root>("B.b1 = 1").expect("Failed to parse TOML"),
        Root::B {
            b1: Some(1),
            b2: None
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("B.b2 = true").expect("Failed to parse TOML"),
        Root::B {
            b1: None,
            b2: Some(true)
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("B = { b1 = 1, b2 = true }").expect("Failed to parse TOML"),
        Root::B {
            b1: Some(1),
            b2: Some(true)
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("[A]").expect("Failed to parse TOML"),
        Root::A(None),
    );
    assert_eq!(
        facet_toml::from_str::<Root>("[B]").expect("Failed to parse TOML"),
        Root::B { b1: None, b2: None },
    );
}

#[test]
fn test_option_enum_with_option_variant() {
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

    assert_eq!(
        facet_toml::from_str::<Root>("").expect("Failed to parse TOML"),
        Root { value: None },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 'A'").expect("Failed to parse TOML"),
        Root {
            value: Some(Item::A)
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value.B = 1").expect("Failed to parse TOML"),
        Root {
            value: Some(Item::B(Some(1)))
        },
    );
}
