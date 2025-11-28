//! Enum deserialization tests ported from toml-rs.
//!
//! Original: https://github.com/toml-rs/toml/blob/main/crates/toml/tests/serde/de_enum.rs

use facet::Facet;
use snapbox::assert_data_eq;
use snapbox::prelude::*;
use snapbox::str;

#[derive(Debug, Facet, PartialEq)]
struct OuterStruct {
    inner: TheEnum,
}

#[derive(Debug, Facet, PartialEq)]
#[repr(u8)]
enum TheEnum {
    Plain,
    Tuple(i64, bool),
    NewType(String),
    Struct { value: i64 },
}

#[derive(Debug, Facet, PartialEq)]
struct Val {
    val: TheEnum,
}

#[derive(Debug, Facet, PartialEq)]
struct Multi {
    enums: Vec<TheEnum>,
}

// NOTE: These error message tests may need adjustment - facet_toml error messages differ from toml
#[test]
fn invalid_variant_returns_error_with_good_message_string() {
    let input = "\"NonExistent\"";
    let expected = str![[r#"
unknown variant `NonExistent`, expected one of `Plain`, `Tuple`, `NewType`, `Struct`

"#]]
    .raw();
    let result = crate::value_from_str::<TheEnum>(input);
    assert_data_eq!(result.unwrap_err().to_string(), expected);

    let input = "val = \"NonExistent\"";
    let expected = str![[r#"
TOML parse error at line 1, column 7
  |
1 | val = "NonExistent"
  |       ^^^^^^^^^^^^^
unknown variant `NonExistent`, expected one of `Plain`, `Tuple`, `NewType`, `Struct`

"#]]
    .raw();
    let result = crate::from_str::<Val>(input);
    assert_data_eq!(result.unwrap_err().to_string(), expected);
}

#[test]
fn invalid_variant_returns_error_with_good_message_inline_table() {
    let input = "{ NonExistent = {} }";
    let expected = str![[r#"
unknown variant `NonExistent`, expected one of `Plain`, `Tuple`, `NewType`, `Struct`

"#]]
    .raw();
    let result = crate::value_from_str::<TheEnum>(input);
    assert_data_eq!(result.unwrap_err().to_string(), expected);

    let input = "val = { NonExistent = {} }";
    let expected = str![[r#"
TOML parse error at line 1, column 9
  |
1 | val = { NonExistent = {} }
  |         ^^^^^^^^^^^
unknown variant `NonExistent`, expected one of `Plain`, `Tuple`, `NewType`, `Struct`

"#]]
    .raw();
    let result = crate::from_str::<Val>(input);
    assert_data_eq!(result.unwrap_err().to_string(), expected);
}

mod enum_unit {
    use super::*;

    #[test]
    fn value_from_str() {
        let input = "\"Plain\"";
        let expected = str![[r#"
Plain

"#]];
        let result = crate::value_from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_str() {
        let input = "val = \"Plain\"";
        let expected = str![[r#"
Val {
    val: Plain,
}

"#]];
        let result = crate::from_str::<Val>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn value_from_inline_table() {
        let input = "{ Plain = {} }";
        let expected = str![[r#"
Plain

"#]];
        let result = crate::value_from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_inline_table() {
        let input = "val = { Plain = {} }";
        let expected = str![[r#"
Val {
    val: Plain,
}

"#]];
        let result = crate::from_str::<Val>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_std_table() {
        let input = "[Plain]";
        let expected = str![[r#"
Plain

"#]];
        let result = crate::from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn extra_field_returns_expected_empty_table_error() {
        let input = "{ Plain = { extra_field = 404 } }";
        let expected = str![[r#"
expected empty table

"#]]
        .raw();
        let result = crate::value_from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap_err().to_string(), expected);

        let input = "val = { Plain = { extra_field = 404 } }";
        let expected = str![[r#"
TOML parse error at line 1, column 17
  |
1 | val = { Plain = { extra_field = 404 } }
  |                 ^^^^^^^^^^^^^^^^^^^^^
expected empty table

"#]]
        .raw();
        let result = crate::from_str::<Val>(input);
        assert_data_eq!(result.unwrap_err().to_string(), expected);
    }
}

mod enum_tuple {
    use super::*;

    #[test]
    fn value_from_inline_table() {
        let input = "{ Tuple = { 0 = -123, 1 = true } }";
        let expected = str![[r#"
Tuple(
    -123,
    true,
)

"#]];
        let result = crate::value_from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_inline_table() {
        let input = "val = { Tuple = { 0 = -123, 1 = true } }";
        let expected = str![[r#"
Val {
    val: Tuple(
        -123,
        true,
    ),
}

"#]];
        let result = crate::from_str::<Val>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_std_table() {
        let input = r#"[Tuple]
                0 = -123
                1 = true
                "#;
        let expected = str![[r#"
Tuple(
    -123,
    true,
)

"#]];
        let result = crate::from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }
}

mod enum_newtype {
    use super::*;

    #[test]
    fn value_from_inline_table() {
        let input = r#"{ NewType = "value" }"#;
        let expected = str![[r#"
NewType(
    "value",
)

"#]];
        let result = crate::value_from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_inline_table() {
        let input = r#"val = { NewType = "value" }"#;
        let expected = str![[r#"
Val {
    val: NewType(
        "value",
    ),
}

"#]];
        let result = crate::from_str::<Val>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_std_table() {
        let result = crate::from_str::<TheEnum>(r#"NewType = "value""#);
        let expected = str![[r#"
NewType(
    "value",
)

"#]];
        assert_data_eq!(result.unwrap().to_debug(), expected);

        let result = crate::from_str::<Val>(
            r#"[val]
                NewType = "value"
                "#,
        );
        let expected = str![[r#"
Val {
    val: NewType(
        "value",
    ),
}

"#]];
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }
}

mod enum_struct {
    use super::*;

    #[test]
    fn value_from_inline_table() {
        let input = "{ Struct = { value = -123 } }";
        let expected = str![[r#"
Struct {
    value: -123,
}

"#]];
        let result = crate::value_from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_inline_table() {
        let input = "val = { Struct = { value = -123 } }";
        let expected = str![[r#"
Val {
    val: Struct {
        value: -123,
    },
}

"#]];
        let result = crate::from_str::<Val>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_std_table() {
        let input = r#"[Struct]
                value = -123
                "#;
        let expected = str![[r#"
Struct {
    value: -123,
}

"#]];
        let result = crate::from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_nested_std_table() {
        let input = r#"[inner.Struct]
                value = -123
                "#;
        let expected = str![[r#"
OuterStruct {
    inner: Struct {
        value: -123,
    },
}

"#]];
        let result = crate::from_str::<OuterStruct>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn extra_field_returns_expected_empty_table_error_struct_variant() {
        let input = "{ Struct = { value = 123, extra_0 = 0, extra_1 = 1 } }";
        let expected = str![[r#"
unexpected keys in table: extra_0, extra_1, available keys: value

"#]]
        .raw();
        let result = crate::value_from_str::<TheEnum>(input);
        assert_data_eq!(result.unwrap_err().to_string(), expected);

        let input = "val = { Struct = { value = 123, extra_0 = 0, extra_1 = 1 } }";
        let expected = str![[r#"
TOML parse error at line 1, column 33
  |
1 | val = { Struct = { value = 123, extra_0 = 0, extra_1 = 1 } }
  |                                 ^^^^^^^
unexpected keys in table: extra_0, extra_1, available keys: value

"#]]
        .raw();
        let result = crate::from_str::<Val>(input);
        assert_data_eq!(result.unwrap_err().to_string(), expected);
    }
}

mod array_enum {
    use super::*;

    #[test]
    fn from_inline_tables() {
        let input = r#"
            enums = [
                { Plain = {} },
                { Tuple = { 0 = -123, 1 = true } },
                { NewType = "value" },
                { Struct = { value = -123 } }
            ]"#;
        let expected = str![[r#"
Multi {
    enums: [
        Plain,
        Tuple(
            -123,
            true,
        ),
        NewType(
            "value",
        ),
        Struct {
            value: -123,
        },
    ],
}

"#]];
        let result = crate::from_str::<Multi>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }

    #[test]
    fn from_std_table() {
        let input = r#"[[enums]]
            Plain = {}

            [[enums]]
            Tuple = { 0 = -123, 1 = true }

            [[enums]]
            NewType = "value"

            [[enums]]
            Struct = { value = -123 }
            "#;
        let expected = str![[r#"
Multi {
    enums: [
        Plain,
        Tuple(
            -123,
            true,
        ),
        NewType(
            "value",
        ),
        Struct {
            value: -123,
        },
    ],
}

"#]];
        let result = crate::from_str::<Multi>(input);
        assert_data_eq!(result.unwrap().to_debug(), expected);
    }
}
