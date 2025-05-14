use eyre::Result;
use facet::Facet;
use facet_json::from_str;

#[test]
fn json_read_unit_enum_variant() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum FontStyle {
        Italic,
        Oblique,
    }
    let json_italic = r#""Italic""#;
    let json_oblique = r#""Oblique""#;

    let s_italic: FontStyle = from_str(json_italic)?;
    assert_eq!(s_italic, FontStyle::Italic);

    let s_oblique: FontStyle = from_str(json_oblique)?;
    assert_eq!(s_oblique, FontStyle::Oblique);

    Ok(())
}

#[test]
fn json_read_unit_enum_variant_lowercase() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "snake_case")]
    #[repr(u8)]
    enum FontStyle {
        Italic,
        Oblique,
    }
    let json_italic = r#""italic""#;
    let json_oblique = r#""oblique""#;

    let s_italic: FontStyle = from_str(json_italic)?;
    assert_eq!(s_italic, FontStyle::Italic);

    let s_oblique: FontStyle = from_str(json_oblique)?;
    assert_eq!(s_oblique, FontStyle::Oblique);

    Ok(())
}

#[test]
fn json_read_tuple_variant() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Point {
        X(u64),
        Y(String, bool),
    }

    let json_x = r#"{ "X": 123 }"#;
    let json_y = r#"{ "Y": [ "hello", true ] }"#;

    let p_x: Point = from_str(json_x)?;
    assert_eq!(p_x, Point::X(123));

    let p_y: Point = from_str(json_y)?;
    assert_eq!(p_y, Point::Y("hello".to_string(), true));

    Ok(())
}

#[test]
fn json_read_struct_variant() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Point {
        Thing,
        Well { made: String, i: bool, guess: i32 },
        Other(i32),
    }

    let json1 = r#"{ "Well": { "made": "in germany", "i": false, "guess": 3 } }"#;

    let point1: Point = from_str(json1)?;
    assert_eq!(
        point1,
        Point::Well {
            made: "in germany".to_string(),
            i: false,
            guess: 3
        }
    );

    Ok(())
}

#[test]
fn enum_generic_u8() {
    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    enum E<'a, T: core::hash::Hash, const C: usize = 3>
    where
        T: std::fmt::Debug,
        [u8; C]: std::fmt::Debug,
    {
        Unit,
        Tuple(T, core::marker::PhantomData<&'a [u8; C]>),
        Record {
            field: T,
            phantom: core::marker::PhantomData<&'a ()>,
            constant_val: [u8; C],
        },
    }
}

#[test]
fn enum_generic_c() {
    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(C)]
    enum E<'a, T: core::hash::Hash, const C: usize = 3>
    where
        T: std::fmt::Debug,
        [u8; C]: std::fmt::Debug,
    {
        Unit,
        Tuple(T, core::marker::PhantomData<&'a [u8; C]>),
        Record {
            field: T,
            phantom: core::marker::PhantomData<&'a ()>,
            constant_val: [u8; C],
        },
    }
}

#[test]
fn enum_() {
    facet_testhelpers::setup();

    #[allow(dead_code)]
    #[derive(facet::Facet)]
    #[repr(C)]
    enum Point {
        Variant1 { field1: String, field2: String },
        Variant2(String),
        Variant3(String, String),
    }

    let good_point = Point::Variant1 {
        field1: "aaa".to_string(),
        field2: "bbb".to_string(),
    };
    assert_eq!(
        facet_json::to_string(&good_point),
        r#"{"Variant1":{"field1":"aaa","field2":"bbb"}}"#
    );

    let bad_point = Point::Variant2("aaa".to_string());
    assert_eq!(facet_json::to_string(&bad_point), r#"{"Variant2":"aaa"}"#);

    let medium_point = Point::Variant3("aaa".to_string(), "bbb".to_string());
    assert_eq!(
        facet_json::to_string(&medium_point),
        r#"{"Variant3":["aaa","bbb"]}"#
    );
}

#[test]
fn enum_variants() -> Result<()> {
    facet_testhelpers::setup();

    // Unit variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum FontStyle {
        Italic,
        Oblique,
    }

    // Test unit variant serialization/deserialization
    let italic = FontStyle::Italic;
    let json_italic = facet_json::to_string(&italic);
    assert_eq!(json_italic, r#""Italic""#);

    let deserialized_italic: FontStyle =
        facet_json::from_str(&json_italic).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_italic, italic);

    // Struct variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[allow(dead_code)]
    enum Message {
        Good { greeting: String, time: i32 },
        Bad { error: String, code: i32 },
    }

    // Test struct variant serialization
    let good = Message::Good {
        greeting: "Hello, sunshine!".to_string(),
        time: 800,
    };

    let json_good = facet_json::to_string(&good);
    assert_eq!(
        json_good,
        r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#
    );

    // Test struct variant deserialization
    let deserialized_good: Message =
        facet_json::from_str(&json_good).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_good, good);

    // Tuple variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Point {
        X(u64),
        Y(String, bool),
    }

    // Test tuple variant serialization
    let x = Point::X(123);
    let json_x = facet_json::to_string(&x);
    assert_eq!(json_x, r#"{"X":123}"#);

    let y = Point::Y("hello".to_string(), true);
    let json_y = facet_json::to_string(&y);
    assert_eq!(json_y, r#"{"Y":["hello",true]}"#);

    // Test tuple variant deserialization
    let deserialized_x: Point = facet_json::from_str(&json_x).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_x, x);

    let deserialized_y: Point = facet_json::from_str(&json_y).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_y, y);

    Ok(())
}

#[test]
fn enum_unit_variants() -> Result<()> {
    facet_testhelpers::setup();

    // Unit variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum FontStyle {
        Italic,
        Oblique,
    }

    // Test unit variant serialization/deserialization
    let italic = FontStyle::Italic;
    let json_italic = facet_json::to_string(&italic);
    assert_eq!(json_italic, r#""Italic""#);

    let deserialized_italic: FontStyle =
        facet_json::from_str(&json_italic).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_italic, italic);

    Ok(())
}

#[test]
fn enum_tuple_primitive_variants() -> Result<()> {
    facet_testhelpers::setup();

    // Tuple variants with primitive types
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Point {
        X(u64),
        Y(i32, bool),
    }

    // Test tuple variant with a primitive type
    let x = Point::X(123);
    let json_x = facet_json::to_string(&x);
    assert_eq!(json_x, r#"{"X":123}"#);

    let deserialized_x: Point = facet_json::from_str(&json_x).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_x, x);

    // Test tuple variant with multiple primitive types
    let y = Point::Y(456, true);
    let json_y = facet_json::to_string(&y);
    assert_eq!(json_y, r#"{"Y":[456,true]}"#);

    let deserialized_y: Point = facet_json::from_str(&json_y).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_y, y);

    Ok(())
}

#[test]
fn enum_struct_variants_1() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    enum Message {
        Good { greeting: String, time: i32 },
        Tenant { id: String, action: String },
    }

    // Test struct variant serialization
    let good = Message::Good {
        greeting: "Hello, sunshine!".to_string(),
        time: 800,
    };

    assert_eq!(
        facet_json::to_string(&good),
        r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#
    );

    let tenant = Message::Tenant {
        id: "tenant-123".to_string(),
        action: "login".to_string(),
    };

    assert_eq!(
        facet_json::to_string(&tenant),
        r#"{"Tenant":{"id":"tenant-123","action":"login"}}"#
    );

    // Test struct variant deserialization
    let json_good = r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#;
    let deserialized_good: Message =
        facet_json::from_str(json_good).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_good, good);

    let json_tenant = r#"{"Tenant":{"id":"tenant-123","action":"login"}}"#;
    let deserialized_tenant: Message =
        facet_json::from_str(json_tenant).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_tenant, tenant);

    // Test roundtrip
    let json = facet_json::to_string(&good);
    let roundtrip: Message = facet_json::from_str(&json).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(roundtrip, good);

    Ok(())
}

#[test]
fn tuple_struct_variants() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct GoodMorning {
        greeting: String,
        time: i32,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct TenantEvent {
        tenant_id: String,
        action: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum MomEvent {
        Good(GoodMorning) = 1,
        Tenant(TenantEvent) = 2,
    }

    // Test serialization
    let good = MomEvent::Good(GoodMorning {
        greeting: "Hello, sunshine!".to_string(),
        time: 800,
    });

    // NOTE: The expected JSON is serialized with the variant name and the struct fields
    let expected_good = r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#;
    assert_eq!(facet_json::to_string(&good), expected_good);

    let tenant = MomEvent::Tenant(TenantEvent {
        tenant_id: "tenant-123".to_string(),
        action: "login".to_string(),
    });

    let expected_tenant = r#"{"Tenant":{"tenant_id":"tenant-123","action":"login"}}"#;
    assert_eq!(facet_json::to_string(&tenant), expected_tenant);

    // Test deserialization
    let json_good = r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#;
    let deserialized_good: MomEvent =
        facet_json::from_str(json_good).map_err(|e| eyre::eyre!("{}", e))?;

    match deserialized_good {
        MomEvent::Good(gm) => {
            assert_eq!(gm.greeting, "Hello, sunshine!");
            assert_eq!(gm.time, 800);
        }
        _ => panic!("Expected Good variant"),
    }

    let json_tenant = r#"{"Tenant":{"tenant_id":"tenant-123","action":"login"}}"#;
    let deserialized_tenant: MomEvent =
        facet_json::from_str(json_tenant).map_err(|e| eyre::eyre!("{}", e))?;

    match deserialized_tenant {
        MomEvent::Tenant(te) => {
            assert_eq!(te.tenant_id, "tenant-123");
            assert_eq!(te.action, "login");
        }
        _ => panic!("Expected Tenant variant"),
    }

    Ok(())
}

#[test]
fn enum_struct_variants_2() -> Result<()> {
    facet_testhelpers::setup();

    // Struct variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[allow(dead_code)]
    enum Message {
        Good { time: i32 },
        Bad { code: i32 },
    }

    // Test struct variant with primitive fields (no strings)
    let good = Message::Good { time: 800 };

    let json_good = facet_json::to_string(&good);
    assert_eq!(json_good, r#"{"Good":{"time":800}}"#);

    // Test struct variant deserialization
    let deserialized_good: Message =
        facet_json::from_str(&json_good).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_good, good);

    Ok(())
}
