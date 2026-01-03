#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
use facet_json::{JsonError, JsonParser, to_vec};
use indoc::indoc;
use libtest_mimic::{Arguments, Failed, Trial};

struct JsonSlice;

impl FormatSuite for JsonSlice {
    type Error = DeserializeError<JsonError>;

    fn format_name() -> &'static str {
        "facet-json/slice"
    }

    fn highlight_language() -> Option<&'static str> {
        Some("json")
    }

    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        T: Facet<'static> + core::fmt::Debug,
    {
        let parser = JsonParser::new(input);
        let mut de = FormatDeserializer::new_owned(parser);
        de.deserialize_root::<T>()
    }

    fn serialize<T>(value: &T) -> Option<Result<Vec<u8>, String>>
    where
        for<'facet> T: Facet<'facet>,
        T: core::fmt::Debug,
    {
        Some(to_vec(value).map_err(|e| e.to_string()))
    }

    #[cfg(feature = "tokio")]
    fn deserialize_async<T>(
        input: &[u8],
    ) -> impl std::future::Future<Output = Option<Result<T, Self::Error>>>
    where
        for<'facet> T: Facet<'facet>,
        T: core::fmt::Debug,
    {
        use facet_json::from_async_reader_tokio;
        use std::io::Cursor;

        let input = input.to_vec();
        async move {
            let reader = Cursor::new(input);
            Some(from_async_reader_tokio(reader).await)
        }
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

    fn attr_default_struct() -> CaseSpec {
        // message is missing, should use String::default() (empty string)
        CaseSpec::from_str(indoc!(
            r#"
            {
                "count": 123
            }
        "#
        ))
    }

    fn attr_default_function() -> CaseSpec {
        // magic_number is missing, should use custom_default_value() = 42
        CaseSpec::from_str(indoc!(
            r#"
            {
                "name": "hello"
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

    fn option_some() -> CaseSpec {
        // nickname has a value
        CaseSpec::from_str(indoc!(
            r#"
            {
                "name": "test",
                "nickname": "nick"
            }
        "#
        ))
    }

    fn option_null() -> CaseSpec {
        // nickname is explicitly null, should be None
        CaseSpec::from_str(indoc!(
            r#"
            {
                "name": "test",
                "nickname": null
            }
        "#
        ))
        .without_roundtrip("null serializes as missing field, not explicit null")
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

    fn attr_skip_serializing_if() -> CaseSpec {
        // optional_data is None, skip_serializing_if = Option::is_none makes it absent in output
        CaseSpec::from_str(indoc!(
            r#"
            {
                "name": "test"
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

    fn error_type_mismatch_string_to_int() -> CaseSpec {
        // String provided where integer expected
        CaseSpec::expect_error(r#"{"value":"not_a_number"}"#, "failed to parse")
    }

    fn error_type_mismatch_object_to_array() -> CaseSpec {
        // Object provided where array expected
        CaseSpec::expect_error(r#"{"items":{"wrong":"structure"}}"#, "type mismatch")
    }

    fn error_missing_required_field() -> CaseSpec {
        // Missing required field "email"
        CaseSpec::expect_error(r#"{"name":"Alice","age":30}"#, "missing field")
    }

    // ── Alias cases ──

    fn attr_alias() -> CaseSpec {
        // Input uses the alias "old_name" which should map to field "new_name"
        CaseSpec::from_str(r#"{"old_name":"value","count":5}"#)
            .without_roundtrip("alias is only for deserialization, serializes as new_name")
    }

    // ── Attribute precedence cases ──

    fn attr_rename_vs_alias_precedence() -> CaseSpec {
        // When both rename and alias are present, rename takes precedence for serialization
        CaseSpec::from_str(r#"{"officialName":"test","id":1}"#)
    }

    fn attr_rename_all_kebab() -> CaseSpec {
        CaseSpec::from_str(r#"{"first-name":"John","last-name":"Doe","user-id":42}"#)
    }

    fn attr_rename_all_screaming() -> CaseSpec {
        CaseSpec::from_str(r#"{"API_KEY":"secret-123","MAX_RETRY_COUNT":5}"#)
    }

    fn attr_rename_unicode() -> CaseSpec {
        CaseSpec::from_str(r#"{"🎉":"party"}"#)
    }

    fn attr_rename_special_chars() -> CaseSpec {
        CaseSpec::from_str(r#"{"@type":"node"}"#)
    }

    // ── Proxy cases ──

    fn proxy_container() -> CaseSpec {
        // ProxyInt deserializes from a string "42" via IntAsString proxy
        CaseSpec::from_str(r#""42""#)
    }

    fn proxy_field_level() -> CaseSpec {
        // Field-level proxy: "count" field deserializes from string "100" via proxy
        CaseSpec::from_str(r#"{"name":"test","count":"100"}"#)
    }

    fn proxy_validation_error() -> CaseSpec {
        // Proxy conversion fails with non-numeric string
        CaseSpec::expect_error(r#""not_a_number""#, "invalid digit")
    }

    fn proxy_with_option() -> CaseSpec {
        CaseSpec::from_str(r#"{"name":"test","count":"42"}"#)
    }

    fn proxy_with_enum() -> CaseSpec {
        CaseSpec::from_str(r#"{"Value":"99"}"#)
    }

    fn proxy_with_transparent() -> CaseSpec {
        CaseSpec::from_str(r#""42""#)
    }

    fn opaque_proxy() -> CaseSpec {
        // OpaqueType doesn't implement Facet, but OpaqueTypeProxy does
        // Use PartialEq comparison since reflection can't peek into opaque types
        // Roundtrip disabled: serialization of opaque types not yet supported
        CaseSpec::from_str(r#"{"value":{"inner":42}}"#)
            .with_partial_eq()
            .without_roundtrip("serialization of opaque types not yet supported")
    }

    fn opaque_proxy_option() -> CaseSpec {
        // Optional opaque field with proxy
        // Use PartialEq comparison since reflection can't peek into opaque types
        // Roundtrip disabled: serialization of opaque types not yet supported
        CaseSpec::from_str(r#"{"value":{"inner":99}}"#)
            .with_partial_eq()
            .without_roundtrip("serialization of opaque types not yet supported")
    }

    fn transparent_multilevel() -> CaseSpec {
        CaseSpec::from_str(r#"42"#)
    }

    fn transparent_option() -> CaseSpec {
        CaseSpec::from_str(r#"99"#)
    }

    fn transparent_nonzero() -> CaseSpec {
        CaseSpec::from_str(r#"42"#)
    }

    fn flatten_optional_some() -> CaseSpec {
        // TODO: flatten with Option<T> not yet fully supported
        CaseSpec::skip("flatten with Option<T> not yet implemented")
    }

    fn flatten_optional_none() -> CaseSpec {
        CaseSpec::from_str(r#"{"name":"test"}"#)
    }

    fn flatten_overlapping_fields_error() -> CaseSpec {
        // Two flattened structs both have a "shared" field - should error
        CaseSpec::expect_error(
            r#"{"field_a":"a","field_b":"b","shared":1}"#,
            "duplicate field",
        )
    }

    fn flatten_multilevel() -> CaseSpec {
        // All fields from 3 levels should be flattened to top level
        CaseSpec::from_str(r#"{"top_field":"top","mid_field":42,"deep_field":100}"#)
    }

    fn flatten_multiple_enums() -> CaseSpec {
        // Two different enums (auth + transport) flattened into same struct
        CaseSpec::from_str(
            r#"{"name":"service","Password":{"password":"secret"},"Tcp":{"port":8080}}"#,
        )
        .without_roundtrip("serialization of flattened enums not yet supported")
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

    fn tuple_nested() -> CaseSpec {
        CaseSpec::from_str(r#"{"outer":[[1,2],["test",true]]}"#)
    }

    fn tuple_empty() -> CaseSpec {
        CaseSpec::from_str(r#"{"name":"test","empty":[]}"#)
            .without_roundtrip("empty tuple serialization format mismatch")
    }

    fn tuple_single_element() -> CaseSpec {
        CaseSpec::from_str(r#"{"name":"test","single":[42]}"#)
    }

    fn tuple_struct_variant() -> CaseSpec {
        CaseSpec::from_str(r#"{"Pair":["test",42]}"#)
    }

    fn tuple_newtype_variant() -> CaseSpec {
        CaseSpec::from_str(r#"{"Some":99}"#)
    }

    // ── Enum variant cases ──

    fn enum_unit_variant() -> CaseSpec {
        CaseSpec::from_str(r#""Active""#)
    }

    fn numeric_enum() -> CaseSpec {
        CaseSpec::from_str(r#"1"#)
    }

    fn signed_numeric_enum() -> CaseSpec {
        CaseSpec::from_str(r#"-1"#)
    }

    fn inferred_numeric_enum() -> CaseSpec {
        CaseSpec::from_str(r#""0""#)
    }

    fn enum_untagged() -> CaseSpec {
        CaseSpec::from_str(r#"{"x":10,"y":20}"#)
    }

    fn enum_variant_rename() -> CaseSpec {
        // Variant "Active" is renamed to "enabled" in the input
        CaseSpec::from_str(r#""enabled""#)
    }

    fn untagged_with_null() -> CaseSpec {
        CaseSpec::from_str(r#"null"#)
            .without_roundtrip("unit variant serializes to variant name, not null")
    }

    fn untagged_newtype_variant() -> CaseSpec {
        CaseSpec::from_str(r#""test""#)
    }

    fn untagged_as_field() -> CaseSpec {
        CaseSpec::from_str(r#"{"name":"test","value":42}"#)
    }

    fn untagged_unit_only() -> CaseSpec {
        // Untagged enum with only unit variants, deserialized from string "Alpha"
        CaseSpec::from_str(r#""Alpha""#)
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
            .without_roundtrip("i128/u128 serialize as strings, not native JSON numbers")
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

    // ── Bytes/binary data cases ──

    fn bytes_vec_u8() -> CaseSpec {
        CaseSpec::from_str(r#"{"data":[0,128,255,42]}"#)
    }

    // ── Fixed-size array cases ──

    fn array_fixed_size() -> CaseSpec {
        CaseSpec::from_str(r#"{"values":[1,2,3]}"#)
    }

    // ── Unknown field handling cases ──

    fn skip_unknown_fields() -> CaseSpec {
        // Input has extra "unknown" field which should be silently skipped
        CaseSpec::from_str(r#"{"unknown":"ignored","known":"value"}"#)
            .without_roundtrip("unknown field is not preserved")
    }

    // ── String escape cases ──

    fn string_escapes() -> CaseSpec {
        // JSON escape sequences: \n, \t, \", \\
        CaseSpec::from_str(r#"{"text":"line1\nline2\ttab\"quote\\backslash"}"#)
    }

    // ── Unit type cases ──

    fn unit_struct() -> CaseSpec {
        // Unit struct serializes as empty object in JSON
        CaseSpec::from_str(r#"{}"#)
    }

    // ── Newtype cases ──

    fn newtype_u64() -> CaseSpec {
        CaseSpec::from_str(r#"{"value":42}"#)
    }

    fn newtype_string() -> CaseSpec {
        CaseSpec::from_str(r#"{"value":"hello"}"#)
    }

    // ── Char cases ──

    fn char_scalar() -> CaseSpec {
        CaseSpec::from_str(r#"{"letter":"A","emoji":"🦀"}"#)
            .without_roundtrip("char serialization not yet supported")
    }

    // ── HashSet cases ──

    fn hashset() -> CaseSpec {
        CaseSpec::from_str(r#"{"items":["alpha","beta"]}"#)
    }

    // ── Nested collection cases ──

    fn vec_nested() -> CaseSpec {
        CaseSpec::from_str(r#"{"matrix":[[1,2],[3,4,5]]}"#)
    }

    // ── Third-party type cases ──

    fn uuid() -> CaseSpec {
        // UUID in canonical hyphenated format
        CaseSpec::from_str(r#"{"id":"550e8400-e29b-41d4-a716-446655440000"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn ulid() -> CaseSpec {
        // ULID in standard Crockford Base32 format
        CaseSpec::from_str(r#"{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn camino_path() -> CaseSpec {
        CaseSpec::from_str(r#"{"path":"/home/user/documents"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn ordered_float() -> CaseSpec {
        CaseSpec::from_str(r#"{"value":1.23456}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn rust_decimal() -> CaseSpec {
        CaseSpec::from_str(r#"{"amount":"24.99"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    // ── Scientific notation floats ──

    fn scalar_floats_scientific() -> CaseSpec {
        CaseSpec::from_str(r#"{"large":1.23e10,"small":-4.56e-7,"positive_exp":5e3}"#)
    }

    // ── Extended escape sequences ──

    fn string_escapes_extended() -> CaseSpec {
        CaseSpec::from_str(
            r#"{"backspace":"hello\u0008world","formfeed":"page\u000Cbreak","carriage_return":"line\rreturn","control_char":"\u0001"}"#,
        )
    }

    // ── Unsized smart pointer cases ──

    fn box_str() -> CaseSpec {
        CaseSpec::from_str(r#"{"inner":"hello world"}"#)
    }

    fn arc_str() -> CaseSpec {
        CaseSpec::from_str(r#"{"inner":"hello world"}"#)
    }

    fn rc_str() -> CaseSpec {
        CaseSpec::from_str(r#"{"inner":"hello world"}"#)
    }

    fn arc_slice() -> CaseSpec {
        CaseSpec::from_str(r#"{"inner":[1,2,3,4]}"#)
    }

    // ── Extended NonZero cases ──

    fn nonzero_integers_extended() -> CaseSpec {
        CaseSpec::from_str(
            r#"{"nz_u8":255,"nz_i8":-128,"nz_u16":65535,"nz_i16":-32768,"nz_u128":1,"nz_i128":-1,"nz_usize":1000,"nz_isize":-500}"#,
        )
        .without_roundtrip("i128/u128 serialize as strings, not native JSON numbers")
    }

    // ── DateTime type cases ──

    fn time_offset_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"{"created_at":"2023-01-15T12:34:56Z"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn jiff_timestamp() -> CaseSpec {
        CaseSpec::from_str(r#"{"created_at":"2023-12-31T11:30:00Z"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn jiff_civil_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"{"created_at":"2024-06-19T15:22:45"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_datetime_utc() -> CaseSpec {
        CaseSpec::from_str(r#"{"created_at":"2023-01-15T12:34:56Z"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_naive_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"{"created_at":"2023-01-15T12:34:56"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_naive_date() -> CaseSpec {
        CaseSpec::from_str(r#"{"birth_date":"2023-01-15"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_naive_time() -> CaseSpec {
        CaseSpec::from_str(r#"{"alarm_time":"12:34:56"}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_in_vec() -> CaseSpec {
        CaseSpec::from_str(r#"{"timestamps":["2023-01-01T00:00:00Z","2023-06-15T12:30:00Z"]}"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    // ── Bytes crate cases ──

    fn bytes_bytes() -> CaseSpec {
        CaseSpec::from_str(r#"{"data":[1,2,3,4,255]}"#)
    }

    fn bytes_bytes_mut() -> CaseSpec {
        CaseSpec::from_str(r#"{"data":[1,2,3,4,255]}"#)
    }

    // ── String optimization crate cases ──

    fn bytestring() -> CaseSpec {
        CaseSpec::from_str(r#"{"value":"hello world"}"#)
    }

    fn compact_string() -> CaseSpec {
        CaseSpec::from_str(r#"{"value":"hello world"}"#)
    }

    fn smartstring() -> CaseSpec {
        CaseSpec::from_str(r#"{"value":"hello world"}"#)
    }

    fn smol_str() -> CaseSpec {
        CaseSpec::from_str(r#"{"value":"hello world"}"#)
    }

    // ── Dynamic value cases ──
    // NOTE: facet_value::Value uses DynamicValue def which requires specialized handling
    // in the deserializer. The format deserializer doesn't support this yet.

    fn value_null() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported in format deserializer")
    }

    fn value_bool() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported in format deserializer")
    }

    fn value_integer() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported in format deserializer")
    }

    fn value_float() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported in format deserializer")
    }

    fn value_string() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported in format deserializer")
    }

    fn value_array() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported in format deserializer")
    }

    fn value_object() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported in format deserializer")
    }
}

fn main() {
    use std::sync::Arc;

    let args = Arguments::from_args();
    let cases: Vec<Arc<_>> = all_cases::<JsonSlice>().into_iter().map(Arc::new).collect();

    let mut trials: Vec<Trial> = Vec::new();

    // Sync tests
    for case in &cases {
        let name = format!("{}::{}", JsonSlice::format_name(), case.id);
        let skip_reason = case.skip_reason();
        let case = Arc::clone(case);
        let mut trial = Trial::test(name, move || match case.run() {
            CaseOutcome::Passed => Ok(()),
            CaseOutcome::Skipped(_) => Ok(()),
            CaseOutcome::Failed(msg) => Err(Failed::from(msg)),
        });
        if skip_reason.is_some() {
            trial = trial.with_ignored_flag(true);
        }
        trials.push(trial);
    }

    // Async tests (only when tokio feature is enabled)
    #[cfg(feature = "tokio")]
    for case in &cases {
        let name = format!("{}/async::{}", JsonSlice::format_name(), case.id);
        let skip_reason = case.skip_reason();
        let case = Arc::clone(case);
        let mut trial = Trial::test(name, move || match case.run_async() {
            CaseOutcome::Passed => Ok(()),
            CaseOutcome::Skipped(_) => Ok(()),
            CaseOutcome::Failed(msg) => Err(Failed::from(msg)),
        });
        if skip_reason.is_some() {
            trial = trial.with_ignored_flag(true);
        }
        trials.push(trial);
    }

    libtest_mimic::run(&args, trials).exit()
}
