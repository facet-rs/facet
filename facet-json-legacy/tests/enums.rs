use facet::Facet;
use facet_json_legacy::from_str;
use facet_testhelpers::test;

#[test]
fn json_read_unit_enum_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum FontStyle {
        Italic,
        Oblique,
    }
    let json_italic = r#""Italic""#;
    let json_oblique = r#""Oblique""#;

    let s_italic: FontStyle = from_str(json_italic).unwrap();
    assert_eq!(s_italic, FontStyle::Italic);

    let s_oblique: FontStyle = from_str(json_oblique).unwrap();
    assert_eq!(s_oblique, FontStyle::Oblique);
}

#[test]
fn json_read_unit_enum_variant_lowercase() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename_all = "snake_case")]
    #[repr(u8)]
    enum FontStyle {
        Italic,
        Oblique,
    }
    let json_italic = r#""italic""#;
    let json_oblique = r#""oblique""#;

    let s_italic: FontStyle = from_str(json_italic).unwrap();
    assert_eq!(s_italic, FontStyle::Italic);

    let s_oblique: FontStyle = from_str(json_oblique).unwrap();
    assert_eq!(s_oblique, FontStyle::Oblique);
}

#[test]
fn json_read_tuple_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Point {
        X(u64),
        Y(String, bool),
    }

    let json_x = r#"{ "X": 123 }"#;
    let json_y = r#"{ "Y": [ "hello", true ] }"#;

    let p_x: Point = from_str(json_x).unwrap();
    assert_eq!(p_x, Point::X(123));

    let p_y: Point = from_str(json_y).unwrap();
    assert_eq!(p_y, Point::Y("hello".to_string(), true));
}

#[test]
fn json_read_struct_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Point {
        Thing,
        Well { made: String, i: bool, guess: i32 },
        Other(i32),
    }

    let json1 = r#"{ "Well": { "made": "in germany", "i": false, "guess": 3 } }"#;

    let point1: Point = from_str(json1).unwrap();
    assert_eq!(
        point1,
        Point::Well {
            made: "in germany".to_string(),
            i: false,
            guess: 3
        }
    );
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
        facet_json_legacy::to_string(&good_point),
        r#"{"Variant1":{"field1":"aaa","field2":"bbb"}}"#
    );

    let bad_point = Point::Variant2("aaa".to_string());
    assert_eq!(
        facet_json_legacy::to_string(&bad_point),
        r#"{"Variant2":"aaa"}"#
    );

    let medium_point = Point::Variant3("aaa".to_string(), "bbb".to_string());
    assert_eq!(
        facet_json_legacy::to_string(&medium_point),
        r#"{"Variant3":["aaa","bbb"]}"#
    );
}

#[test]
fn enum_variants() {
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
    let json_italic = facet_json_legacy::to_string(&italic);
    assert_eq!(json_italic, r#""Italic""#);

    let deserialized_italic: FontStyle = facet_json_legacy::from_str(&json_italic).unwrap();
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

    let json_good = facet_json_legacy::to_string(&good);
    assert_eq!(
        json_good,
        r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#
    );

    // Test struct variant deserialization
    let deserialized_good: Message = facet_json_legacy::from_str(&json_good).unwrap();
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
    let json_x = facet_json_legacy::to_string(&x);
    assert_eq!(json_x, r#"{"X":123}"#);

    let y = Point::Y("hello".to_string(), true);
    let json_y = facet_json_legacy::to_string(&y);
    assert_eq!(json_y, r#"{"Y":["hello",true]}"#);

    // Test tuple variant deserialization
    let deserialized_x: Point = facet_json_legacy::from_str(&json_x).unwrap();
    assert_eq!(deserialized_x, x);

    let deserialized_y: Point = facet_json_legacy::from_str(&json_y).unwrap();
    assert_eq!(deserialized_y, y);
}

#[test]
fn enum_unit_variants() {
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
    let json_italic = facet_json_legacy::to_string(&italic);
    assert_eq!(json_italic, r#""Italic""#);

    let deserialized_italic: FontStyle = facet_json_legacy::from_str(&json_italic).unwrap();
    assert_eq!(deserialized_italic, italic);
}

#[test]
fn enum_tuple_primitive_variants() {
    // Tuple variants with primitive types
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Point {
        X(u64),
        Y(i32, bool),
    }

    // Test tuple variant with a primitive type
    let x = Point::X(123);
    let json_x = facet_json_legacy::to_string(&x);
    assert_eq!(json_x, r#"{"X":123}"#);

    let deserialized_x: Point = facet_json_legacy::from_str(&json_x).unwrap();
    assert_eq!(deserialized_x, x);

    // Test tuple variant with multiple primitive types
    let y = Point::Y(456, true);
    let json_y = facet_json_legacy::to_string(&y);
    assert_eq!(json_y, r#"{"Y":[456,true]}"#);

    let deserialized_y: Point = facet_json_legacy::from_str(&json_y).unwrap();
    assert_eq!(deserialized_y, y);
}

#[test]
fn enum_struct_variants_1() {
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
        facet_json_legacy::to_string(&good),
        r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#
    );

    let tenant = Message::Tenant {
        id: "tenant-123".to_string(),
        action: "login".to_string(),
    };

    assert_eq!(
        facet_json_legacy::to_string(&tenant),
        r#"{"Tenant":{"id":"tenant-123","action":"login"}}"#
    );

    // Test struct variant deserialization
    let json_good = r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#;
    let deserialized_good: Message = facet_json_legacy::from_str(json_good).unwrap();
    assert_eq!(deserialized_good, good);

    let json_tenant = r#"{"Tenant":{"id":"tenant-123","action":"login"}}"#;
    let deserialized_tenant: Message = facet_json_legacy::from_str(json_tenant).unwrap();
    assert_eq!(deserialized_tenant, tenant);

    // Test roundtrip
    let json = facet_json_legacy::to_string(&good);
    let roundtrip: Message = facet_json_legacy::from_str(&json).unwrap();
    assert_eq!(roundtrip, good);
}

#[test]
fn tuple_struct_variants() {
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
    assert_eq!(facet_json_legacy::to_string(&good), expected_good);

    let tenant = MomEvent::Tenant(TenantEvent {
        tenant_id: "tenant-123".to_string(),
        action: "login".to_string(),
    });

    let expected_tenant = r#"{"Tenant":{"tenant_id":"tenant-123","action":"login"}}"#;
    assert_eq!(facet_json_legacy::to_string(&tenant), expected_tenant);

    // Test deserialization
    let json_good = r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#;
    let deserialized_good: MomEvent = facet_json_legacy::from_str(json_good).unwrap();

    match deserialized_good {
        MomEvent::Good(gm) => {
            assert_eq!(gm.greeting, "Hello, sunshine!");
            assert_eq!(gm.time, 800);
        }
        _ => panic!("Expected Good variant"),
    }

    let json_tenant = r#"{"Tenant":{"tenant_id":"tenant-123","action":"login"}}"#;
    let deserialized_tenant: MomEvent = facet_json_legacy::from_str(json_tenant).unwrap();

    match deserialized_tenant {
        MomEvent::Tenant(te) => {
            assert_eq!(te.tenant_id, "tenant-123");
            assert_eq!(te.action, "login");
        }
        _ => panic!("Expected Tenant variant"),
    }
}

#[test]
fn enum_struct_variants_2() {
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

    let json_good = facet_json_legacy::to_string(&good);
    assert_eq!(json_good, r#"{"Good":{"time":800}}"#);

    // Test struct variant deserialization
    let deserialized_good: Message = facet_json_legacy::from_str(&json_good).unwrap();
    assert_eq!(deserialized_good, good);
}

// ===========================================================================
// Internally Tagged Enums (#[facet(tag = "type")])
// ===========================================================================

#[test]
fn test_internally_tagged_struct_variant_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(tag = "type")]
    enum Message {
        Request { id: String, method: String },
        Response { id: String, result: String },
    }

    // Test serialization - tag field is at same level as content fields
    let request = Message::Request {
        id: "1".to_string(),
        method: "ping".to_string(),
    };
    let json = facet_json_legacy::to_string(&request);
    assert_eq!(json, r#"{"type":"Request","id":"1","method":"ping"}"#);

    // Test other variant
    let response = Message::Response {
        id: "1".to_string(),
        result: "pong".to_string(),
    };
    let json_resp = facet_json_legacy::to_string(&response);
    assert_eq!(json_resp, r#"{"type":"Response","id":"1","result":"pong"}"#);
}

#[test]
fn test_internally_tagged_unit_variant_serialize() {
    #[allow(dead_code)]
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(tag = "status")]
    enum Status {
        Active,
        Inactive,
    }

    // Unit variants with internal tag serialize as {"tag": "VariantName"}
    let active = Status::Active;
    let json = facet_json_legacy::to_string(&active);
    assert_eq!(json, r#"{"status":"Active"}"#);
}

// ===========================================================================
// Adjacently Tagged Enums (#[facet(tag = "t", content = "c")])
// ===========================================================================

#[test]
fn test_adjacently_tagged_struct_variant_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(tag = "t", content = "c")]
    enum Block {
        Para { text: String },
        Header { level: u8, text: String },
    }

    // Test serialization - tag and content are sibling fields
    let para = Block::Para {
        text: "Hello".to_string(),
    };
    let json = facet_json_legacy::to_string(&para);
    assert_eq!(json, r#"{"t":"Para","c":{"text":"Hello"}}"#);

    // Test other variant
    let header = Block::Header {
        level: 2,
        text: "Title".to_string(),
    };
    let json_header = facet_json_legacy::to_string(&header);
    assert_eq!(
        json_header,
        r#"{"t":"Header","c":{"level":2,"text":"Title"}}"#
    );
}

#[test]
fn test_adjacently_tagged_tuple_variant_serialize() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(tag = "type", content = "data")]
    enum Value {
        Str(String),
        Pair(i32, i32),
    }

    // Newtype variant
    let s = Value::Str("hello".to_string());
    let json = facet_json_legacy::to_string(&s);
    assert_eq!(json, r#"{"type":"Str","data":"hello"}"#);

    // Tuple variant with multiple elements
    let pair = Value::Pair(10, 20);
    let json_pair = facet_json_legacy::to_string(&pair);
    assert_eq!(json_pair, r#"{"type":"Pair","data":[10,20]}"#);
}

#[test]
fn test_adjacently_tagged_unit_variant_serialize() {
    #[allow(dead_code)]
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(tag = "kind", content = "value")]
    enum Signal {
        Start,
        Stop,
    }

    // Unit variants with adjacent tagging omit the content field when empty
    let start = Signal::Start;
    let json = facet_json_legacy::to_string(&start);
    assert_eq!(json, r#"{"kind":"Start"}"#);
}

// ===========================================================================
// Untagged Enums (#[facet(untagged)])
// ===========================================================================

#[test]
fn test_untagged_newtype_variants() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum StringOrInt {
        Int(i64),
        Str(String),
    }

    // Int variant serializes as just the number
    let int_val = StringOrInt::Int(42);
    let json_int = facet_json_legacy::to_string(&int_val);
    assert_eq!(json_int, "42");

    // String variant serializes as just the string
    let str_val = StringOrInt::Str("hello".to_string());
    let json_str = facet_json_legacy::to_string(&str_val);
    assert_eq!(json_str, r#""hello""#);
}

#[test]
fn test_untagged_newtype_tuple_variant_issue_1189() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum Counter {
        Unit(String),
        Weight((String, f64)),
    }

    let json = r#"["AGRICIBPAR", 1.0]"#;
    let counter: Counter = facet_json_legacy::from_str(json).unwrap();
    assert_eq!(counter, Counter::Weight(("AGRICIBPAR".to_string(), 1.0)));
}

#[test]
fn test_untagged_struct_variants() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    // Struct variants serialize without any tag
    let circle = Shape::Circle { radius: 5.0 };
    let json = facet_json_legacy::to_string(&circle);
    assert_eq!(json, r#"{"radius":5.0}"#);

    let rect = Shape::Rectangle {
        width: 10.0,
        height: 20.0,
    };
    let json_rect = facet_json_legacy::to_string(&rect);
    assert_eq!(json_rect, r#"{"width":10.0,"height":20.0}"#);
}

#[test]
fn test_untagged_unit_variant() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum MaybeNull {
        Null,
        Value(i32),
    }

    // Untagged unit variant serializes as variant name string
    let null_val = MaybeNull::Null;
    let json = facet_json_legacy::to_string(&null_val);
    assert_eq!(json, r#""Null""#);

    let val = MaybeNull::Value(42);
    let json_val = facet_json_legacy::to_string(&val);
    assert_eq!(json_val, "42");
}
