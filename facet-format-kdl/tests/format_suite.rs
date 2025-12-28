#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_format_kdl::{KdlError, KdlParser, to_vec};
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
use indoc::indoc;
use libtest_mimic::{Arguments, Failed, Trial};
use std::sync::Arc;

struct KdlSlice;

impl FormatSuite for KdlSlice {
    type Error = DeserializeError<KdlError>;

    fn format_name() -> &'static str {
        "facet-format-kdl/slice"
    }

    fn highlight_language() -> Option<&'static str> {
        Some("kdl")
    }

    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        T: Facet<'static> + core::fmt::Debug,
    {
        let input_str = std::str::from_utf8(input)
            .map_err(|e| DeserializeError::Parser(KdlError::ParseError(e.to_string())))?;
        let parser = KdlParser::new(input_str);
        let mut de = FormatDeserializer::new_owned(parser);
        de.deserialize()
    }

    fn serialize<T>(value: &T) -> Option<Result<Vec<u8>, String>>
    where
        for<'facet> T: Facet<'facet>,
        T: core::fmt::Debug,
    {
        Some(to_vec(value).map_err(|e| format!("{:?}", e)))
    }

    // ── Struct cases ──
    // Note: format_suite types don't have KDL attributes, so we use child nodes
    // which emit FieldLocationHint::Child. The deserializer matches by field name.

    fn struct_single_field() -> CaseSpec {
        // Child node "name" with argument "facet"
        CaseSpec::from_str(indoc!(
            r#"
            record {
                name "facet"
            }
        "#
        ))
    }

    fn struct_nested() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            parent {
                id 42
                child {
                    code "alpha"
                    active #true
                }
                tags {
                    item "core"
                    item "json"
                }
            }
        "#
        ))
    }

    // ── Sequence cases ──

    fn sequence_numbers() -> CaseSpec {
        // KDL doesn't have native arrays - use repeated child nodes
        CaseSpec::from_str(indoc!(
            r#"
            numbers {
                value 1
                value 2
                value 3
            }
        "#
        ))
    }

    fn sequence_mixed_scalars() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            mixed {
                entry -1
                entry 4.625
                entry #null
                entry #true
            }
        "#
        ))
    }

    // ── Enum cases ──

    fn enum_complex() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            enum {
                Label {
                    name "facet"
                    level 7
                }
            }
        "#
        ))
    }

    fn enum_internally_tagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            shape {
                type "Circle"
                radius 5.0
            }
        "#
        ))
    }

    fn enum_adjacently_tagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            value {
                t "Message"
                c "hello"
            }
        "#
        ))
    }

    fn enum_unit_variant() -> CaseSpec {
        // KDL root nodes emit StructStart, not scalar - can't match unit variant directly
        CaseSpec::skip("KDL root nodes can't express bare unit variant selection")
    }

    fn enum_untagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            value {
                x 10
                y 20
            }
        "#
        ))
    }

    fn enum_variant_rename() -> CaseSpec {
        // KDL root nodes emit StructStart, not scalar - can't match variant directly
        CaseSpec::skip("KDL root nodes can't express bare unit variant selection")
    }

    // ── Attribute cases ──

    fn attr_rename_field() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                userName "alice"
                age 30
            }
        "#
        ))
    }

    fn attr_rename_all_camel() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                firstName "Jane"
                lastName "Doe"
                isActive #true
            }
        "#
        ))
    }

    fn attr_default_field() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                required "present"
            }
        "#
        ))
    }

    fn attr_default_struct() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                count 123
            }
        "#
        ))
        .without_roundtrip("empty string serializes differently")
    }

    fn attr_default_function() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                name "hello"
            }
        "#
        ))
    }

    fn attr_skip_serializing() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                visible "shown"
            }
        "#
        ))
    }

    fn attr_skip_serializing_if() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                name "test"
            }
        "#
        ))
    }

    fn attr_skip() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                visible "data"
            }
        "#
        ))
    }

    fn attr_alias() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                old_name "value"
                count 5
            }
        "#
        ))
        .without_roundtrip("alias is only for deserialization")
    }

    fn attr_rename_vs_alias_precedence() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                officialName "test"
                id 1
            }
        "#
        ))
    }

    fn attr_rename_all_kebab() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                first-name "John"
                last-name "Doe"
                user-id 42
            }
        "#
        ))
    }

    fn attr_rename_all_screaming() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                API_KEY "secret-123"
                MAX_RETRY_COUNT 5
            }
        "#
        ))
    }

    fn attr_rename_unicode() -> CaseSpec {
        // KDL supports unicode but may need quoting
        CaseSpec::skip("Unicode field names need special handling in KDL")
    }

    fn attr_rename_special_chars() -> CaseSpec {
        // Special chars in identifiers need quoting
        CaseSpec::skip("Special char field names need quoting in KDL")
    }

    // ── Option cases ──

    fn option_none() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                name "test"
            }
        "#
        ))
    }

    fn option_some() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                name "test"
                nickname "nick"
            }
        "#
        ))
    }

    fn option_null() -> CaseSpec {
        // KDL has #null - but the deserializer needs to handle it
        CaseSpec::skip("KDL #null handling for Option fields not yet implemented")
    }

    // ── Flatten cases ──

    fn struct_flatten() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                name "point"
                x 10
                y 20
            }
        "#
        ))
    }

    fn flatten_optional_some() -> CaseSpec {
        CaseSpec::skip("flatten with Option<T> not yet implemented")
    }

    fn flatten_optional_none() -> CaseSpec {
        CaseSpec::from_str(r#"record { name "test" }"#)
    }

    fn flatten_overlapping_fields_error() -> CaseSpec {
        CaseSpec::expect_error(
            r#"record { field_a "a"; field_b "b"; shared 1 }"#,
            "duplicate field",
        )
    }

    fn flatten_multilevel() -> CaseSpec {
        CaseSpec::skip("multilevel nested flatten not yet implemented")
    }

    fn flatten_multiple_enums() -> CaseSpec {
        CaseSpec::skip("multiple flattened enums not yet implemented")
    }

    // ── Transparent cases ──

    fn transparent_newtype() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            record {
                id 42
                name "alice"
            }
        "#
        ))
    }

    fn transparent_multilevel() -> CaseSpec {
        // KDL requires nodes with names - can't express bare scalars like JSON's `42`
        // A node `value 42` creates StructStart which doesn't match transparent expectation
        CaseSpec::skip("KDL requires node names; can't express bare scalars for transparent types")
    }

    fn transparent_option() -> CaseSpec {
        // Same issue as transparent_multilevel
        CaseSpec::skip("KDL requires node names; can't express bare scalars for transparent types")
    }

    fn transparent_nonzero() -> CaseSpec {
        // Same issue as transparent_multilevel
        CaseSpec::skip("KDL requires node names; can't express bare scalars for transparent types")
    }

    // ── Error cases ──

    fn deny_unknown_fields() -> CaseSpec {
        CaseSpec::expect_error(
            r#"record { foo "abc"; bar 42; baz #true }"#,
            "unknown field",
        )
    }

    fn error_type_mismatch_string_to_int() -> CaseSpec {
        CaseSpec::expect_error(r#"record { value "not_a_number" }"#, "Failed to parse")
    }

    fn error_type_mismatch_object_to_array() -> CaseSpec {
        CaseSpec::skip("KDL nodes are ambiguous like XML elements")
    }

    fn error_missing_required_field() -> CaseSpec {
        CaseSpec::expect_error(r#"record { name "Alice"; age 30 }"#, "missing field")
    }

    // ── Scalar cases ──

    fn scalar_bool() -> CaseSpec {
        CaseSpec::from_str(r#"record { yes #true; no #false }"#)
    }

    fn scalar_integers() -> CaseSpec {
        CaseSpec::from_str(
            r#"record { signed_8 -128; unsigned_8 255; signed_32 -2147483648; unsigned_32 4294967295; signed_64 -9223372036854775808; unsigned_64 18446744073709551615 }"#,
        )
    }

    fn scalar_integers_16() -> CaseSpec {
        CaseSpec::from_str(r#"record { signed_16 -32768; unsigned_16 65535 }"#)
    }

    fn scalar_integers_128() -> CaseSpec {
        // KDL's integer literals overflow at i128 boundaries, so use strings and coerce
        CaseSpec::from_str(
            r#"record { signed_128 "-170141183460469231731687303715884105728"; unsigned_128 "340282366920938463463374607431768211455" }"#,
        )
        .without_roundtrip("serializer outputs numeric literals which may differ")
    }

    fn scalar_integers_size() -> CaseSpec {
        CaseSpec::from_str(r#"record { signed_size -1000; unsigned_size 2000 }"#)
    }

    fn scalar_floats() -> CaseSpec {
        CaseSpec::from_str(r#"record { float_32 1.5; float_64 2.25 }"#)
    }

    fn scalar_floats_scientific() -> CaseSpec {
        CaseSpec::from_str(r#"record { large 1.23e10; small -4.56e-7; positive_exp 5e3 }"#)
    }

    fn char_scalar() -> CaseSpec {
        CaseSpec::from_str(r#"record { letter "A"; emoji "🦀" }"#)
            .without_roundtrip("char serialization not yet supported")
    }

    fn nonzero_integers() -> CaseSpec {
        CaseSpec::from_str(r#"record { nz_u32 42; nz_i64 -100 }"#)
    }

    fn nonzero_integers_extended() -> CaseSpec {
        CaseSpec::from_str(
            r#"record { nz_u8 255; nz_i8 -128; nz_u16 65535; nz_i16 -32768; nz_u128 1; nz_i128 -1; nz_usize 1000; nz_isize -500 }"#,
        )
    }

    // ── String cases ──

    fn cow_str() -> CaseSpec {
        CaseSpec::from_str(r#"record { owned "hello world"; message "borrowed" }"#)
    }

    fn string_escapes() -> CaseSpec {
        // KDL uses standard escape sequences
        CaseSpec::from_str(r#"record { text "line1\nline2\ttab\"quote\\backslash" }"#)
    }

    fn string_escapes_extended() -> CaseSpec {
        CaseSpec::from_str(
            r#"record { backspace "hello\bworld"; formfeed "page\fbreak"; carriage_return "line\rreturn"; control_char "\u{0001}" }"#,
        )
    }

    // ── Collection cases ──

    fn map_string_keys() -> CaseSpec {
        CaseSpec::from_str(r#"record { data { alpha 1; beta 2 } }"#)
    }

    fn tuple_simple() -> CaseSpec {
        CaseSpec::from_str(r#"record { triple { item "hello"; item 42; item #true } }"#)
    }

    fn tuple_nested() -> CaseSpec {
        CaseSpec::from_str(
            r#"record { outer { item { item 1; item 2 }; item { item "test"; item #true } } }"#,
        )
    }

    fn tuple_empty() -> CaseSpec {
        CaseSpec::skip("empty tuple not supported in KDL")
    }

    fn tuple_single_element() -> CaseSpec {
        CaseSpec::skip("single-element tuple not supported in KDL")
    }

    fn tuple_struct_variant() -> CaseSpec {
        CaseSpec::from_str(r#"value { Pair { item "test"; item 42 } }"#)
    }

    fn tuple_newtype_variant() -> CaseSpec {
        CaseSpec::from_str(r#"value { Some 99 }"#)
    }

    fn set_btree() -> CaseSpec {
        CaseSpec::from_str(r#"record { items { item "alpha"; item "beta"; item "gamma" } }"#)
    }

    fn hashset() -> CaseSpec {
        CaseSpec::from_str(r#"record { items { item "alpha"; item "beta" } }"#)
    }

    fn vec_nested() -> CaseSpec {
        // Nested Vec uses "item" for both levels - serializer outputs consistent naming
        CaseSpec::from_str(
            r#"record { matrix { item { item 1; item 2 }; item { item 3; item 4; item 5 } } }"#,
        )
    }

    fn array_fixed_size() -> CaseSpec {
        CaseSpec::from_str(r#"record { values { value 1; value 2; value 3 } }"#)
    }

    fn bytes_vec_u8() -> CaseSpec {
        CaseSpec::from_str(r#"record { data { value 0; value 128; value 255; value 42 } }"#)
    }

    // ── Pointer cases ──

    fn box_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"record { inner 42 }"#)
    }

    fn arc_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"record { inner 42 }"#)
    }

    fn rc_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"record { inner 42 }"#)
    }

    fn box_str() -> CaseSpec {
        CaseSpec::from_str(r#"record { inner "hello world" }"#)
    }

    fn arc_str() -> CaseSpec {
        CaseSpec::from_str(r#"record { inner "hello world" }"#)
    }

    fn rc_str() -> CaseSpec {
        CaseSpec::from_str(r#"record { inner "hello world" }"#)
    }

    fn arc_slice() -> CaseSpec {
        CaseSpec::from_str(r#"record { inner { item 1; item 2; item 3; item 4 } }"#)
    }

    // ── Newtype cases ──

    fn newtype_u64() -> CaseSpec {
        CaseSpec::from_str(r#"record { value 42 }"#)
    }

    fn newtype_string() -> CaseSpec {
        CaseSpec::from_str(r#"record { value "hello" }"#)
    }

    // ── Unit cases ──

    fn unit_struct() -> CaseSpec {
        CaseSpec::from_str(r#"UnitStruct"#)
    }

    // ── Unknown field handling ──

    fn skip_unknown_fields() -> CaseSpec {
        CaseSpec::from_str(r#"record { unknown "ignored"; known "value" }"#)
            .without_roundtrip("unknown field is not preserved")
    }

    // ── Untagged enum cases ──

    fn untagged_with_null() -> CaseSpec {
        CaseSpec::skip("KDL empty nodes don't map to unit variants")
    }

    fn untagged_newtype_variant() -> CaseSpec {
        // KDL requires node names - root node `value "test"` emits StructStart
        CaseSpec::skip("KDL requires node names; can't express bare scalars for newtype variants")
    }

    fn untagged_as_field() -> CaseSpec {
        CaseSpec::skip("numeric matching not yet supported")
    }

    fn untagged_unit_only() -> CaseSpec {
        // KDL requires node names - root node `value "Alpha"` emits StructStart
        CaseSpec::skip("KDL requires node names; can't express bare scalars for unit variants")
    }

    // ── Proxy cases ──

    fn proxy_container() -> CaseSpec {
        // KDL requires node names - root node emits StructStart for proxy containers
        CaseSpec::skip("KDL requires node names; can't express bare scalars for proxy containers")
    }

    fn proxy_field_level() -> CaseSpec {
        CaseSpec::from_str(r#"record { name "test"; count "100" }"#)
    }

    fn proxy_validation_error() -> CaseSpec {
        // KDL root nodes emit StructStart, but proxy expects scalar directly
        CaseSpec::skip("KDL root nodes can't express bare scalars for proxy validation")
    }

    fn proxy_with_option() -> CaseSpec {
        CaseSpec::from_str(r#"record { name "test"; count "42" }"#)
    }

    fn proxy_with_enum() -> CaseSpec {
        CaseSpec::from_str(r#"value { Value "99" }"#)
    }

    fn proxy_with_transparent() -> CaseSpec {
        // KDL requires node names - can't express bare scalars for transparent+proxy
        CaseSpec::skip("KDL requires node names; can't express bare scalars for transparent proxy")
    }

    fn opaque_proxy() -> CaseSpec {
        CaseSpec::from_str(r#"record { value { inner 42 } }"#)
            .with_partial_eq()
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn opaque_proxy_option() -> CaseSpec {
        CaseSpec::from_str(r#"record { value { inner 99 } }"#)
            .with_partial_eq()
            .without_roundtrip("opaque type serialization not yet supported")
    }

    // ── Third-party type cases ──

    fn uuid() -> CaseSpec {
        CaseSpec::from_str(r#"record { id "550e8400-e29b-41d4-a716-446655440000" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn ulid() -> CaseSpec {
        CaseSpec::from_str(r#"record { id "01ARZ3NDEKTSV4RRFFQ69G5FAV" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn camino_path() -> CaseSpec {
        CaseSpec::from_str(r#"record { path "/home/user/documents" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn ordered_float() -> CaseSpec {
        CaseSpec::from_str(r#"record { value 1.23456 }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn time_offset_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"record { created_at "2023-01-15T12:34:56Z" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn jiff_timestamp() -> CaseSpec {
        CaseSpec::from_str(r#"record { created_at "2023-12-31T11:30:00Z" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn jiff_civil_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"record { created_at "2024-06-19T15:22:45" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_datetime_utc() -> CaseSpec {
        CaseSpec::from_str(r#"record { created_at "2023-01-15T12:34:56Z" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_naive_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"record { created_at "2023-01-15T12:34:56" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_naive_date() -> CaseSpec {
        CaseSpec::from_str(r#"record { birth_date "2023-01-15" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_naive_time() -> CaseSpec {
        CaseSpec::from_str(r#"record { alarm_time "12:34:56" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn chrono_in_vec() -> CaseSpec {
        CaseSpec::from_str(
            r#"record { timestamps { item "2023-01-01T00:00:00Z"; item "2023-06-15T12:30:00Z" } }"#,
        )
        .without_roundtrip("opaque type serialization not yet supported")
    }

    fn bytes_bytes() -> CaseSpec {
        CaseSpec::from_str(r#"record { data { item 1; item 2; item 3; item 4; item 255 } }"#)
    }

    fn bytes_bytes_mut() -> CaseSpec {
        CaseSpec::from_str(r#"record { data { item 1; item 2; item 3; item 4; item 255 } }"#)
    }

    fn bytestring() -> CaseSpec {
        CaseSpec::from_str(r#"record { value "hello world" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn compact_string() -> CaseSpec {
        CaseSpec::from_str(r#"record { value "hello world" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    fn smartstring() -> CaseSpec {
        CaseSpec::from_str(r#"record { value "hello world" }"#)
            .without_roundtrip("opaque type serialization not yet supported")
    }

    // ── Dynamic value cases ──

    fn value_null() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported")
    }

    fn value_bool() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported")
    }

    fn value_integer() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported")
    }

    fn value_float() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported")
    }

    fn value_string() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported")
    }

    fn value_array() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported")
    }

    fn value_object() -> CaseSpec {
        CaseSpec::skip("DynamicValue not yet supported")
    }

    fn numeric_enum() -> CaseSpec {
        CaseSpec::skip("Numeric enum not yet supported")
    }

    fn signed_numeric_enum() -> CaseSpec {
        CaseSpec::skip("Numeric enum not yet supported")
    }

    fn inferred_numeric_enum() -> CaseSpec {
        CaseSpec::skip("Numeric enum not yet supported")
    }
}

fn main() {
    let args = Arguments::from_args();

    let trials: Vec<Trial> = all_cases::<KdlSlice>()
        .into_iter()
        .map(|case| {
            let case = Arc::new(case);
            let name = format!("{}::{}", KdlSlice::format_name(), case.id);
            let skip_reason = case.skip_reason();
            let case_clone = Arc::clone(&case);
            let mut trial = Trial::test(name, move || match case_clone.run() {
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
