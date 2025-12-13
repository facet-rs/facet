#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_format_json::{JsonError, JsonParser, to_vec};
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
use indoc::indoc;
use libtest_mimic::{Arguments, Failed, Trial};

struct JsonSlice;

impl FormatSuite for JsonSlice {
    type Error = DeserializeError<JsonError>;

    fn format_name() -> &'static str {
        "facet-format-json/slice"
    }

    fn highlight_language() -> Option<&'static str> {
        Some("json")
    }

    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        T: Facet<'static> + core::fmt::Debug,
    {
        let parser = JsonParser::new(input);
        let mut de = FormatDeserializer::new(parser);
        de.deserialize_root::<T>()
    }

    fn serialize<T>(value: &T) -> Option<Result<Vec<u8>, String>>
    where
        for<'facet> T: Facet<'facet>,
        T: core::fmt::Debug,
    {
        Some(to_vec(value).map_err(|e| e.to_string()))
    }

    fn struct_single_field() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            {
                "name":"facet"
            }
        "#
        ))
    }

    fn sequence_numbers() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            [1,2,3]
        "#
        ))
    }

    fn sequence_mixed_scalars() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            [-1, 4.625, null, true]
        "#
        ))
    }

    fn struct_nested() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            {
                "id": 42,
                "child": {
                    "code": "alpha",
                    "active": true
                },
                "tags": ["core", "json"]
            }
        "#
        ))
    }

    fn enum_complex() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            {
                "Label": {
                    "name": "facet",
                    "level": 7
                }
            }
        "#
        ))
    }

    // ── Attribute cases ──

    fn attr_rename_field() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            {
                "userName": "alice",
                "age": 30
            }
        "#
        ))
    }

    fn attr_rename_all_camel() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            {
                "firstName": "Jane",
                "lastName": "Doe",
                "isActive": true
            }
        "#
        ))
    }

    fn attr_default_field() -> CaseSpec {
        // optional_count is missing, should default to 0
        CaseSpec::from_str(indoc!(
            r#"
            {
                "required": "present"
            }
        "#
        ))
    }

    fn option_none() -> CaseSpec {
        // nickname is missing, should be None
        CaseSpec::from_str(indoc!(
            r#"
            {
                "name": "test"
            }
        "#
        ))
    }

    fn attr_skip_serializing() -> CaseSpec {
        // hidden field not in input (will use default), not serialized on roundtrip
        CaseSpec::from_str(indoc!(
            r#"
            {
                "visible": "shown"
            }
        "#
        ))
    }

    fn attr_skip() -> CaseSpec {
        // internal field is completely ignored - not read from input, not written on output
        CaseSpec::from_str(indoc!(
            r#"
            {
                "visible": "data"
            }
        "#
        ))
    }

    // ── Enum tagging cases ──

    fn enum_internally_tagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            {
                "type": "Circle",
                "radius": 5.0
            }
        "#
        ))
    }

    fn enum_adjacently_tagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            {
                "t": "Message",
                "c": "hello"
            }
        "#
        ))
    }

    // ── Advanced cases ──

    fn struct_flatten() -> CaseSpec {
        // x and y are flattened into the outer object
        CaseSpec::from_str(indoc!(
            r#"
            {
                "name": "point",
                "x": 10,
                "y": 20
            }
        "#
        ))
    }

    fn transparent_newtype() -> CaseSpec {
        // UserId(42) serializes as just 42, not {"0": 42}
        CaseSpec::from_str(indoc!(
            r#"
            {
                "id": 42,
                "name": "alice"
            }
        "#
        ))
    }

    // ── Error cases ──

    fn deny_unknown_fields() -> CaseSpec {
        // Input has extra field "baz" which should trigger an error
        CaseSpec::expect_error(r#"{"foo":"abc","bar":42,"baz":true}"#, "unknown field")
    }

    // ── Alias cases ──

    fn attr_alias() -> CaseSpec {
        // Input uses the alias "old_name" which should map to field "new_name"
        CaseSpec::from_str(r#"{"old_name":"value","count":5}"#)
            .without_roundtrip("alias is only for deserialization, serializes as new_name")
    }

    // ── Proxy cases ──

    fn proxy_container() -> CaseSpec {
        // ProxyInt deserializes from a string "42" via IntAsString proxy
        CaseSpec::from_str(r#""42""#)
    }

    // ── Scalar cases ──

    fn scalar_bool() -> CaseSpec {
        CaseSpec::from_str(r#"{"yes":true,"no":false}"#)
    }

    fn scalar_integers() -> CaseSpec {
        CaseSpec::from_str(
            r#"{"signed_8":-128,"unsigned_8":255,"signed_32":-2147483648,"unsigned_32":4294967295,"signed_64":-9223372036854775808,"unsigned_64":18446744073709551615}"#,
        )
    }

    fn scalar_floats() -> CaseSpec {
        CaseSpec::from_str(r#"{"float_32":1.5,"float_64":2.25}"#)
    }

    // ── Collection cases ──

    fn map_string_keys() -> CaseSpec {
        CaseSpec::from_str(r#"{"data":{"alpha":1,"beta":2}}"#)
    }

    fn tuple_simple() -> CaseSpec {
        CaseSpec::from_str(r#"{"triple":["hello",42,true]}"#)
    }

    // ── Enum variant cases ──

    fn enum_unit_variant() -> CaseSpec {
        CaseSpec::from_str(r#""Active""#)
    }

    fn enum_untagged() -> CaseSpec {
        CaseSpec::from_str(r#"{"x":10,"y":20}"#)
    }

    // ── Smart pointer cases ──

    fn box_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"{"inner":42}"#)
    }

    fn arc_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"{"inner":42}"#)
    }

    fn rc_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"{"inner":42}"#)
    }

    // ── Set cases ──

    fn set_btree() -> CaseSpec {
        CaseSpec::from_str(r#"{"items":["alpha","beta","gamma"]}"#)
    }

    // ── Extended numeric cases ──

    fn scalar_integers_16() -> CaseSpec {
        CaseSpec::from_str(r#"{"signed_16":-32768,"unsigned_16":65535}"#)
    }

    fn scalar_integers_128() -> CaseSpec {
        CaseSpec::from_str(r#"{"signed_128":-170141183460469231731687303715884105728,"unsigned_128":340282366920938463463374607431768211455}"#)
            .without_roundtrip("i128/u128 serialization not supported yet")
    }

    fn scalar_integers_size() -> CaseSpec {
        CaseSpec::from_str(r#"{"signed_size":-1000,"unsigned_size":2000}"#)
    }

    // ── NonZero cases ──

    fn nonzero_integers() -> CaseSpec {
        CaseSpec::from_str(r#"{"nz_u32":42,"nz_i64":-100}"#)
    }

    // ── Borrowed string cases ──

    fn cow_str() -> CaseSpec {
        CaseSpec::from_str(r#"{"owned":"hello world","message":"borrowed"}"#)
    }
}

fn main() {
    let args = Arguments::from_args();
    let trials: Vec<Trial> = all_cases::<JsonSlice>()
        .into_iter()
        .map(|case| {
            let name = format!("{}::{}", JsonSlice::format_name(), case.id);
            let skip_reason = case.skip_reason();
            let mut trial = Trial::test(name, move || match case.run() {
                CaseOutcome::Passed => Ok(()),
                CaseOutcome::Skipped(_) => Ok(()),
                CaseOutcome::Failed(msg) => Err(Failed::from(msg)),
            });
            if skip_reason.is_some() {
                trial = trial.with_ignored_flag(true);
            }
            trial
        })
        .collect();

    libtest_mimic::run(&args, trials).exit()
}
