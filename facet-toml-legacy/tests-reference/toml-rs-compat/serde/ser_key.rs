use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;
use snapbox::assert_data_eq;
use snapbox::str;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Document<K: Ord> {
    map: Map<K>,
}

type Map<K> = BTreeMap<K, String>;

/// Verify that `ser` produces values compatible with `serde_json`
#[track_caller]
fn json_from_toml_value_str<T>(s: &'_ str) -> T
where
    T: serde::de::DeserializeOwned,
{
    let value = t!(crate::value_from_str::<crate::SerdeValue>(s));
    let value = t!(value.try_into::<serde_json::Value>());
    let json = t!(serde_json::to_string_pretty(&value));
    t!(serde_json::from_str::<T>(&json))
}

/// Verify that `ser` produces documents compatible with `serde_json`
#[track_caller]
fn json_from_toml_str<T>(s: &'_ str) -> T
where
    T: serde::de::DeserializeOwned,
{
    let value = t!(crate::from_str::<crate::SerdeTable>(s));
    let value = t!(value.try_into::<serde_json::Value>());
    let json = t!(serde_json::to_string_pretty(&value));
    t!(serde_json::from_str::<T>(&json))
}

mod str_key {
    use super::*;

    type Map = super::Map<String>;
    type Document = super::Document<String>;

    fn key() -> String {
        "key".to_owned()
    }

    #[test]
    fn to_string_value() {
        let expected = str![[r#"{ key = "value" }"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string() {
        let expected = str![[r#"
key = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string_pretty() {
        let expected = str![[r#"
key = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_value() {
        let expected = str![[r#"{ map = { key = "value" } }"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string() {
        let expected = str![[r#"
[map]
key = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_pretty() {
        let expected = str![[r#"
[map]
key = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }
}

mod variant_key {
    use super::*;

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
    enum Keys {
        #[allow(non_camel_case_types)]
        key,
    }

    type Map = super::Map<Keys>;
    type Document = super::Document<Keys>;

    fn key() -> Keys {
        Keys::key
    }

    #[test]
    fn to_string_value() {
        let expected = str![[r#"{ key = "value" }"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string() {
        let expected = str![[r#"
key = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string_pretty() {
        let expected = str![[r#"
key = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_value() {
        let expected = str![[r#"{ map = { key = "value" } }"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string() {
        let expected = str![[r#"
[map]
key = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_pretty() {
        let expected = str![[r#"
[map]
key = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }
}

mod bool_key {
    use super::*;

    type Map = super::Map<bool>;
    type Document = super::Document<bool>;

    fn key() -> bool {
        false
    }

    #[test]
    fn to_string_value() {
        let expected = str![[r#"{ false = "value" }"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string() {
        let expected = str![[r#"
false = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string_pretty() {
        let expected = str![[r#"
false = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_value() {
        let expected = str![[r#"{ map = { false = "value" } }"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string() {
        let expected = str![[r#"
[map]
false = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_pretty() {
        let expected = str![[r#"
[map]
false = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }
}

mod i16_key {
    use super::*;

    type Map = super::Map<i16>;
    type Document = super::Document<i16>;

    fn key() -> i16 {
        42
    }

    #[test]
    fn to_string_value() {
        let expected = str![[r#"{ 42 = "value" }"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string() {
        let expected = str![[r#"
42 = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string_pretty() {
        let expected = str![[r#"
42 = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_value() {
        let expected = str![[r#"{ map = { 42 = "value" } }"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string() {
        let expected = str![[r#"
[map]
42 = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_pretty() {
        let expected = str![[r#"
[map]
42 = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }
}

mod char_key {
    use super::*;

    type Map = super::Map<char>;
    type Document = super::Document<char>;

    fn key() -> char {
        'k'
    }

    #[test]
    fn to_string_value() {
        let expected = str![[r#"{ k = "value" }"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string() {
        let expected = str![[r#"
k = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn to_string_pretty() {
        let expected = str![[r#"
k = "value"

"#]];
        let input = [(key(), "value".to_owned())].into_iter().collect::<Map>();
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Map>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Map>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_value() {
        let expected = str![[r#"{ map = { k = "value" } }"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_value(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::value_from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_value_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string() {
        let expected = str![[r#"
[map]
k = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }

    #[test]
    fn nested_to_string_pretty() {
        let expected = str![[r#"
[map]
k = "value"

"#]];
        let input = Document {
            map: [(key(), "value".to_owned())].into_iter().collect::<Map>(),
        };
        let toml = t!(crate::to_string_pretty(&input));
        assert_data_eq!(&toml, expected);
        let roundtrip = t!(crate::from_str::<Document>(&toml));
        assert_eq!(roundtrip, input);
        let json = json_from_toml_str::<Document>(&toml);
        assert_eq!(json, input);
    }
}
