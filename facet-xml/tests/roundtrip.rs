//! Standalone roundtrip tests for facet-xml.
//!
//! Each test defines its own local types and verifies that XML can be
//! deserialized and (where applicable) serialized back correctly.

use facet::Facet;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use test_log::test;

// ══════════════════════════════════════════════════════════════════════════════
// Basic struct tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn struct_single_field() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        name: String,
    }

    let xml = r#"<record><name>facet</name></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(
        parsed,
        Record {
            name: "facet".into()
        }
    );
}

#[test_log::test]
fn sequence_numbers() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "numbers")]
    struct Numbers {
        #[facet(rename = "value")]
        values: Vec<u32>,
    }

    let xml = r#"<numbers><value>1</value><value>2</value><value>3</value></numbers>"#;
    let parsed: Numbers = facet_xml::from_str(xml).unwrap();
    assert_eq!(
        parsed,
        Numbers {
            values: vec![1, 2, 3]
        }
    );
}

#[test_log::test]
fn struct_nested() {
    #[derive(Facet, Debug, PartialEq)]
    struct Child {
        code: String,
        active: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "parent")]
    struct Parent {
        id: u32,
        child: Child,
        #[facet(rename = "item")]
        tags: Vec<String>,
    }

    // Flat list: <item> elements appear directly as children (no <tags> wrapper)
    let xml = r#"<parent><id>42</id><child><code>alpha</code><active>true</active></child><item>core</item><item>json</item></parent>"#;
    let parsed: Parent = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.id, 42);
    assert_eq!(parsed.child.code, "alpha");
    assert!(parsed.child.active);
    assert_eq!(parsed.tags, vec!["core", "json"]);
}

// ══════════════════════════════════════════════════════════════════════════════
// Enum tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn enum_complex() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(C)]
    enum MyEnum {
        Label { name: String, level: u32 },
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "enum")]
    struct Wrapper {
        #[facet(flatten)]
        inner: MyEnum,
    }

    let xml = r#"<enum><Label><name>facet</name><level>7</level></Label></enum>"#;
    let parsed: Wrapper = facet_xml::from_str(xml).unwrap();
    assert_eq!(
        parsed.inner,
        MyEnum::Label {
            name: "facet".into(),
            level: 7
        }
    );
}

#[test]
fn enum_unit_variant() {
    // In XML, unit variants are represented as empty elements
    // The element name IS the variant discriminant
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Status {
        Active,
        Inactive,
    }

    let xml = r#"<Active/>"#;
    let parsed: Status = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed, Status::Active);
}

#[test]
fn enum_internally_tagged() {
    // In XML, internally-tagged enums are serialized the same as externally-tagged:
    // the element name IS the variant discriminant. The tag attribute is ignored.
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(tag = "type")]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    let xml = r#"<Circle><radius>5.0</radius></Circle>"#;
    let parsed: Shape = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed, Shape::Circle { radius: 5.0 });
}

#[test]
fn enum_adjacently_tagged() {
    // In XML, adjacently-tagged enums are serialized the same as externally-tagged:
    // the element name IS the variant discriminant. The tag/content attributes are ignored.
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(tag = "t", content = "c")]
    enum Message {
        Message(String),
        Count(u32),
    }

    let xml = r#"<Message>hello</Message>"#;
    let parsed: Message = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed, Message::Message("hello".into()));
}

#[test]
fn enum_variant_rename() {
    // Variant rename affects the element name in XML
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Status {
        #[facet(rename = "enabled")]
        Active,
        #[facet(rename = "disabled")]
        Inactive,
    }

    let xml = r#"<enabled/>"#;
    let parsed: Status = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed, Status::Active);
}

#[test]
fn enum_untagged() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(untagged, rename = "value")]
    enum Point {
        Coords { x: i32, y: i32 },
    }

    let xml = r#"<value><x>10</x><y>20</y></value>"#;
    let parsed: Point = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed, Point::Coords { x: 10, y: 20 });
}

// ══════════════════════════════════════════════════════════════════════════════
// Attribute tests (rename, default, skip, etc.)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn attr_rename_field() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        #[facet(rename = "userName")]
        user_name: String,
        age: u32,
    }

    let xml = r#"<record><userName>alice</userName><age>30</age></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.user_name, "alice");
    assert_eq!(parsed.age, 30);
}

#[test]
fn attr_rename_all_camel() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record", rename_all = "camelCase")]
    struct Record {
        first_name: String,
        last_name: String,
        is_active: bool,
    }

    let xml = r#"<record><firstName>Jane</firstName><lastName>Doe</lastName><isActive>true</isActive></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.first_name, "Jane");
    assert_eq!(parsed.last_name, "Doe");
    assert!(parsed.is_active);
}

#[test]
fn attr_rename_all_kebab() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record", rename_all = "kebab-case")]
    struct Record {
        first_name: String,
        last_name: String,
        user_id: u32,
    }

    let xml = r#"<record><first-name>John</first-name><last-name>Doe</last-name><user-id>42</user-id></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.first_name, "John");
    assert_eq!(parsed.user_id, 42);
}

#[test]
fn attr_rename_all_screaming() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record", rename_all = "SCREAMING_SNAKE_CASE")]
    struct Record {
        api_key: String,
        max_retry_count: u32,
    }

    let xml =
        r#"<record><API_KEY>secret-123</API_KEY><MAX_RETRY_COUNT>5</MAX_RETRY_COUNT></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.api_key, "secret-123");
    assert_eq!(parsed.max_retry_count, 5);
}

#[test]
fn attr_default_field() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        required: String,
        #[facet(default)]
        optional_count: u32,
    }

    let xml = r#"<record><required>present</required></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.required, "present");
    assert_eq!(parsed.optional_count, 0);
}

fn custom_default_value() -> u32 {
    42
}

#[test]
fn attr_default_function() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        name: String,
        #[facet(default = custom_default_value())]
        magic_number: u32,
    }

    let xml = r#"<record><name>hello</name></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.name, "hello");
    assert_eq!(parsed.magic_number, 42);
}

#[test]
fn option_none() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        name: String,
        nickname: Option<String>,
    }

    let xml = r#"<record><name>test</name></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.name, "test");
    assert_eq!(parsed.nickname, None);
}

#[test]
fn option_some() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        name: String,
        nickname: Option<String>,
    }

    let xml = r#"<record><name>test</name><nickname>nick</nickname></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.nickname, Some("nick".into()));
}

#[test]
fn attr_skip_serializing() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        visible: String,
        #[facet(skip_serializing, default)]
        hidden: String,
    }

    let xml = r#"<record><visible>shown</visible></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.visible, "shown");
    assert_eq!(parsed.hidden, "");
}

#[test]
fn attr_skip() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        visible: String,
        #[facet(skip, default)]
        internal: u32,
    }

    let xml = r#"<record><visible>data</visible></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.visible, "data");
    assert_eq!(parsed.internal, 0);
}

#[test]
fn attr_alias() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        #[facet(alias = "old_name")]
        new_name: String,
        count: u32,
    }

    let xml = r#"<record><old_name>value</old_name><count>5</count></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.new_name, "value");
}

// ══════════════════════════════════════════════════════════════════════════════
// Flatten tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn struct_flatten() {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        name: String,
        #[facet(flatten)]
        point: Point,
    }

    let xml = r#"<record><name>point</name><x>10</x><y>20</y></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.name, "point");
    assert_eq!(parsed.point.x, 10);
    assert_eq!(parsed.point.y, 20);
}

#[test]
fn flatten_optional_some() {
    #[derive(Facet, Debug, PartialEq)]
    struct Metadata {
        version: u32,
        author: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        name: String,
        #[facet(flatten)]
        meta: Option<Metadata>,
    }

    let xml = r#"<record><name>test</name><version>1</version><author>alice</author></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(
        parsed.meta,
        Some(Metadata {
            version: 1,
            author: "alice".into()
        })
    );
}

#[test]
fn flatten_optional_none() {
    #[derive(Facet, Debug, PartialEq, Default)]
    struct Metadata {
        version: u32,
        author: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        name: String,
        #[facet(flatten, default)]
        meta: Option<Metadata>,
    }

    let xml = r#"<record><name>test</name></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.name, "test");
}

// ══════════════════════════════════════════════════════════════════════════════
// Transparent newtype tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn transparent_newtype() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(transparent)]
    struct UserId(u64);

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        id: UserId,
        name: String,
    }

    let xml = r#"<record><id>42</id><name>alice</name></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.id.0, 42);
    assert_eq!(parsed.name, "alice");
}

// ══════════════════════════════════════════════════════════════════════════════
// Scalar tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn scalar_bool() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        yes: bool,
        no: bool,
    }

    let xml = r#"<record><yes>true</yes><no>false</no></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert!(parsed.yes);
    assert!(!parsed.no);
}

#[test]
fn scalar_integers() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        signed_8: i8,
        unsigned_8: u8,
        signed_32: i32,
        unsigned_32: u32,
        signed_64: i64,
        unsigned_64: u64,
    }

    let xml = r#"<record><signed_8>-128</signed_8><unsigned_8>255</unsigned_8><signed_32>-2147483648</signed_32><unsigned_32>4294967295</unsigned_32><signed_64>-9223372036854775808</signed_64><unsigned_64>18446744073709551615</unsigned_64></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.signed_8, -128);
    assert_eq!(parsed.unsigned_8, 255);
    assert_eq!(parsed.signed_64, i64::MIN);
    assert_eq!(parsed.unsigned_64, u64::MAX);
}

#[test]
fn scalar_floats() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        float_32: f32,
        float_64: f64,
    }

    let xml = r#"<record><float_32>1.5</float_32><float_64>2.25</float_64></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.float_32, 1.5);
    assert_eq!(parsed.float_64, 2.25);
}

#[test]
fn char_scalar() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        letter: char,
        emoji: char,
    }

    let xml = r#"<record><letter>A</letter><emoji>🦀</emoji></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.letter, 'A');
    assert_eq!(parsed.emoji, '🦀');
}

// ══════════════════════════════════════════════════════════════════════════════
// Collection tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn map_string_keys() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        data: HashMap<String, u32>,
    }

    let xml = r#"<record><data><alpha>1</alpha><beta>2</beta></data></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.data.get("alpha"), Some(&1));
    assert_eq!(parsed.data.get("beta"), Some(&2));
}

#[test]
fn tuple_simple() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        data: (i32, String, bool),
    }

    // Tuples use <item> elements matched by position
    let xml = r#"<record><data><item>42</item><item>hello</item><item>true</item></data></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.data.0, 42);
    assert_eq!(parsed.data.1, "hello");
    assert!(parsed.data.2);
}

#[test]
fn set_btree() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        #[facet(rename = "item")]
        items: BTreeSet<String>,
    }

    // Flat list: <item> elements appear directly as children (no <items> wrapper)
    let xml = r#"<record><item>alpha</item><item>beta</item><item>gamma</item></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert!(parsed.items.contains("alpha"));
    assert!(parsed.items.contains("beta"));
    assert!(parsed.items.contains("gamma"));
}

#[test]
fn hashset() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        #[facet(rename = "item")]
        items: HashSet<String>,
    }

    // Flat list: <item> elements appear directly as children (no <items> wrapper)
    let xml = r#"<record><item>alpha</item><item>beta</item></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert!(parsed.items.contains("alpha"));
    assert!(parsed.items.contains("beta"));
}

#[test]
fn vec_nested() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        #[facet(rename = "item")]
        matrix: Vec<Vec<u32>>,
    }

    let xml = r#"<record><matrix><item><value>1</value><value>2</value></item><item><value>3</value><value>4</value><value>5</value></item></matrix></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.matrix.len(), 2);
}

#[test]
fn array_fixed_size() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        #[facet(rename = "value")]
        values: [u32; 3],
    }

    // Flat list: repeated <value> elements directly as children (no wrapper)
    let xml = r#"<record><value>1</value><value>2</value><value>3</value></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.values, [1, 2, 3]);
}

/// Test explicit wrapper struct for wrapped list format.
///
/// Since 0.43.0, facet-xml uses flat lists by default. If you need the old
/// wrapped format (where list items are inside a wrapper element named after
/// the field), you can use an explicit wrapper struct.
#[test]
fn explicit_wrapper_for_wrapped_lists() {
    #[derive(Facet, Debug, PartialEq)]
    struct Track {
        title: String,
    }

    // The wrapper struct holds the Vec and specifies the item element name
    #[derive(Facet, Debug, PartialEq)]
    struct TrackList {
        #[facet(rename = "track")]
        items: Vec<Track>,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "Playlist")]
    struct Playlist {
        name: String,
        // Use xml::element (single) pointing to the wrapper struct
        tracks: TrackList,
    }

    // This is the "wrapped" format: tracks wrapper contains track children
    let xml = r#"<Playlist><name>Favorites</name><tracks><track><title>Song A</title></track><track><title>Song B</title></track></tracks></Playlist>"#;
    let parsed: Playlist = facet_xml::from_str(xml).unwrap();

    assert_eq!(parsed.name, "Favorites");
    assert_eq!(parsed.tracks.items.len(), 2);
    assert_eq!(parsed.tracks.items[0].title, "Song A");
    assert_eq!(parsed.tracks.items[1].title, "Song B");

    // Roundtrip: serialize and deserialize again
    let serialized = facet_xml::to_string(&parsed).unwrap();
    let reparsed: Playlist = facet_xml::from_str(&serialized).unwrap();
    assert_eq!(parsed, reparsed);
}

/// Test multiple flat lists in the same struct.
///
/// With flat lists, each list uses its renamed element name to distinguish items.
/// NOTE: Elements for each list must be contiguous (all books together, all magazines together).
#[test]
fn multiple_flat_lists_in_struct() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "library")]
    struct Library {
        #[facet(rename = "book")]
        books: Vec<String>,
        #[facet(rename = "magazine")]
        magazines: Vec<String>,
    }

    // Elements for each list are contiguous (not interleaved)
    let xml = r#"<library><book>1984</book><book>Dune</book><book>Foundation</book><magazine>Time</magazine><magazine>Nature</magazine></library>"#;
    let parsed: Library = facet_xml::from_str(xml).unwrap();

    assert_eq!(parsed.books, vec!["1984", "Dune", "Foundation"]);
    assert_eq!(parsed.magazines, vec!["Time", "Nature"]);
}

// ══════════════════════════════════════════════════════════════════════════════
// Smart pointer tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn box_wrapper() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        inner: Box<u32>,
    }

    let xml = r#"<record><inner>42</inner></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(*parsed.inner, 42);
}

#[test]
fn arc_wrapper() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        inner: Arc<u32>,
    }

    let xml = r#"<record><inner>42</inner></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(*parsed.inner, 42);
}

#[test]
fn rc_wrapper() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        inner: Rc<u32>,
    }

    let xml = r#"<record><inner>42</inner></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(*parsed.inner, 42);
}

#[test]
fn box_str() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        inner: Box<str>,
    }

    let xml = r#"<record><inner>hello world</inner></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(&*parsed.inner, "hello world");
}

#[test]
fn arc_str() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        inner: Arc<str>,
    }

    let xml = r#"<record><inner>hello world</inner></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(&*parsed.inner, "hello world");
}

#[test]
fn arc_slice() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        #[facet(rename = "item")]
        items: Arc<[u32]>,
    }

    // Flat list: repeated <item> elements directly as children (serde-xml style)
    let xml = r#"<record><item>1</item><item>2</item><item>3</item><item>4</item></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(&*parsed.items, &[1, 2, 3, 4]);
}

// ══════════════════════════════════════════════════════════════════════════════
// Cow and borrowed string tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn cow_str() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        owned: Cow<'static, str>,
        message: Cow<'static, str>,
    }

    let xml = r#"<record><owned>hello world</owned><message>borrowed</message></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(&*parsed.owned, "hello world");
    assert_eq!(&*parsed.message, "borrowed");
}

// ══════════════════════════════════════════════════════════════════════════════
// Newtype tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn newtype_u64() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(transparent)]
    struct Wrapper(u64);

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        value: Wrapper,
    }

    let xml = r#"<record><value>42</value></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.value.0, 42);
}

#[test]
fn newtype_string() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(transparent)]
    struct Wrapper(String);

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        value: Wrapper,
    }

    let xml = r#"<record><value>hello</value></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.value.0, "hello");
}

// ══════════════════════════════════════════════════════════════════════════════
// String escape tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn string_escapes() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        text: String,
    }

    let xml = r#"<record><text>line1&#10;line2&#9;tab&quot;quote\backslash</text></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert!(parsed.text.contains('\n'));
    assert!(parsed.text.contains('\t'));
    assert!(parsed.text.contains('"'));
    assert!(parsed.text.contains('\\'));
}

// ══════════════════════════════════════════════════════════════════════════════
// Unit struct tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn unit_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct UnitStruct;

    let xml = r#"<UnitStruct/>"#;
    let parsed: UnitStruct = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed, UnitStruct);
}

// ══════════════════════════════════════════════════════════════════════════════
// Unknown field handling tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn skip_unknown_fields() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        known: String,
    }

    let xml = r#"<record><unknown>ignored</unknown><known>value</known></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.known, "value");
}

// ══════════════════════════════════════════════════════════════════════════════
// Error case tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn deny_unknown_fields() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record", deny_unknown_fields)]
    struct Record {
        foo: String,
        bar: u32,
    }

    let xml = r#"<record><foo>abc</foo><bar>42</bar><baz>true</baz></record>"#;
    let result: Result<Record, _> = facet_xml::from_str(xml);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("unknown") || err.contains("baz"),
        "Expected unknown field error, got: {}",
        err
    );
}

#[test]
fn error_type_mismatch_string_to_int() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        value: u32,
    }

    let xml = r#"<record><value>not_a_number</value></record>"#;
    let result: Result<Record, _> = facet_xml::from_str(xml);
    assert!(result.is_err());
}

#[test]
fn error_missing_required_field() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        name: String,
        age: u32,
        email: String,
    }

    let xml = r#"<record><name>Alice</name><age>30</age></record>"#;
    let result: Result<Record, _> = facet_xml::from_str(xml);
    assert!(result.is_err());
}

// ══════════════════════════════════════════════════════════════════════════════
// Bytes/binary data tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn bytes_vec_u8() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "record")]
    struct Record {
        #[facet(rename = "value")]
        data: Vec<u8>,
    }

    // Flat list: repeated <value> elements directly as children (no wrapper)
    let xml =
        r#"<record><value>0</value><value>128</value><value>255</value><value>42</value></record>"#;
    let parsed: Record = facet_xml::from_str(xml).unwrap();
    assert_eq!(parsed.data, vec![0, 128, 255, 42]);
}
