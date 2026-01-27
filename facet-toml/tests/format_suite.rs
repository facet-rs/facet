#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
use facet_toml::{TomlError, TomlParser, to_string};
use indoc::indoc;
use libtest_mimic::{Arguments, Failed, Trial};
use std::sync::Arc;

struct TomlSlice;

impl FormatSuite for TomlSlice {
    type Error = DeserializeError;

    fn format_name() -> &'static str {
        "facet-toml/slice"
    }

    fn highlight_language() -> Option<&'static str> {
        Some("toml")
    }

    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        T: Facet<'static> + core::fmt::Debug,
    {
        let input_str = std::str::from_utf8(input).expect("input should be valid UTF-8");
        let parser = TomlParser::new(input_str).map_err(DeserializeError::parser)?;
        let mut de = FormatDeserializer::new_owned(parser);
        de.deserialize_deferred::<T>()
    }

    fn serialize<T>(value: &T) -> Option<Result<Vec<u8>, String>>
    where
        for<'facet> T: Facet<'facet>,
        T: core::fmt::Debug,
    {
        Some(
            to_string(value)
                .map(|s| s.into_bytes())
                .map_err(|e| e.to_string()),
        )
    }

    fn struct_single_field() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            name = "facet"
        "#
        ))
    }

    fn sequence_numbers() -> CaseSpec {
        // TOML requires arrays to be inside a field or be inline
        // The suite expects a bare array, which TOML cannot represent at the root level
        CaseSpec::skip("TOML cannot represent a bare array at root level")
    }

    fn sequence_mixed_scalars() -> CaseSpec {
        // TOML requires homogeneous arrays (pre-1.0) and no null
        CaseSpec::skip("TOML has no null and requires homogeneous arrays")
    }

    fn struct_nested() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            id = 42
            tags = ["core", "json"]

            [child]
            code = "alpha"
            active = true
        "#
        ))
    }

    fn enum_complex() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            [Label]
            name = "facet"
            level = 7
        "#
        ))
    }

    // -- Attribute cases --

    fn attr_rename_field() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            userName = "alice"
            age = 30
        "#
        ))
    }

    fn attr_rename_all_camel() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            firstName = "Jane"
            lastName = "Doe"
            isActive = true
        "#
        ))
    }

    fn attr_default_field() -> CaseSpec {
        // optional_count is missing, should default to 0
        CaseSpec::from_str(indoc!(
            r#"
            required = "present"
        "#
        ))
    }

    fn attr_default_struct() -> CaseSpec {
        // message is missing, should use String::default() (empty string)
        // TOML requires deferred mode to handle missing fields with defaults
        CaseSpec::skip("struct-level defaults require deferred deserialization mode")
    }

    fn attr_default_function() -> CaseSpec {
        // magic_number is missing, should use custom_default_value() = 42
        CaseSpec::from_str(indoc!(
            r#"
            name = "hello"
        "#
        ))
    }

    fn option_none() -> CaseSpec {
        // nickname is missing, should be None
        CaseSpec::from_str(indoc!(
            r#"
            name = "test"
        "#
        ))
        .without_roundtrip("TOML serializer cannot serialize Option::None as a value")
    }

    fn option_some() -> CaseSpec {
        // nickname has a value
        CaseSpec::from_str(indoc!(
            r#"
            name = "test"
            nickname = "nick"
        "#
        ))
    }

    fn option_null() -> CaseSpec {
        // TOML has no null literal
        CaseSpec::skip("TOML has no null literal")
    }

    fn attr_skip_serializing() -> CaseSpec {
        // hidden field not in input (will use default), not serialized on roundtrip
        CaseSpec::from_str(indoc!(
            r#"
            visible = "shown"
        "#
        ))
    }

    fn attr_skip_serializing_if() -> CaseSpec {
        // optional_data is None, skip_serializing_if = Option::is_none makes it absent in output
        CaseSpec::from_str(indoc!(
            r#"
            name = "test"
        "#
        ))
    }

    fn attr_skip() -> CaseSpec {
        // internal field is completely ignored - not read from input, not written on output
        CaseSpec::from_str(indoc!(
            r#"
            visible = "data"
        "#
        ))
    }

    // -- Enum tagging cases --

    fn enum_internally_tagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            type = "Circle"
            radius = 5.0
        "#
        ))
    }

    fn enum_adjacently_tagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            t = "Message"
            c = "hello"
        "#
        ))
    }

    // -- Advanced cases --

    fn struct_flatten() -> CaseSpec {
        // x and y are flattened into the outer element
        CaseSpec::from_str(indoc!(
            r#"
            name = "point"
            x = 10
            y = 20
        "#
        ))
    }

    fn transparent_newtype() -> CaseSpec {
        // UserId(42) serializes as just 42, not a nested element
        CaseSpec::from_str(indoc!(
            r#"
            id = 42
            name = "alice"
        "#
        ))
    }

    // -- Error cases --

    fn deny_unknown_fields() -> CaseSpec {
        // Input has extra field "baz" which should trigger an error
        CaseSpec::expect_error("foo = \"abc\"\nbar = 42\nbaz = true", "unknown field")
    }

    fn error_type_mismatch_string_to_int() -> CaseSpec {
        // String provided where integer expected
        CaseSpec::expect_error("value = \"not_a_number\"", "failed to parse")
    }

    fn error_type_mismatch_object_to_array() -> CaseSpec {
        // Object (nested struct) provided where array expected
        CaseSpec::expect_error("[items]\nkey = \"value\"", "type mismatch")
    }

    fn error_missing_required_field() -> CaseSpec {
        // Missing required field "email"
        CaseSpec::expect_error("name = \"Alice\"\nage = 30", "was not initialized")
    }

    // -- Alias cases --

    fn attr_alias() -> CaseSpec {
        // Input uses the alias "old_name" which should map to field "new_name"
        CaseSpec::from_str("old_name = \"value\"\ncount = 5")
            .without_roundtrip("alias is only for deserialization, serializes as new_name")
    }

    // -- Attribute precedence cases --

    fn attr_rename_vs_alias_precedence() -> CaseSpec {
        // When both rename and alias are present, rename takes precedence for serialization
        CaseSpec::from_str("officialName = \"test\"\nid = 1")
    }

    fn attr_rename_all_kebab() -> CaseSpec {
        CaseSpec::from_str("first-name = \"John\"\nlast-name = \"Doe\"\nuser-id = 42")
    }

    fn attr_rename_all_screaming() -> CaseSpec {
        CaseSpec::from_str("API_KEY = \"secret-123\"\nMAX_RETRY_COUNT = 5")
    }

    fn attr_rename_unicode() -> CaseSpec {
        // TOML bare keys have restricted charset (alphanumeric, dash, underscore)
        // Need quoted keys for unicode
        CaseSpec::skip("TOML bare keys cannot contain emoji - would need quoted keys")
    }

    fn attr_rename_special_chars() -> CaseSpec {
        // TOML bare keys have restricted charset
        CaseSpec::skip("TOML bare keys cannot contain @ - would need quoted keys")
    }

    // -- Proxy cases --

    fn proxy_container() -> CaseSpec {
        // ProxyInt deserializes from a string "42" via IntAsString proxy
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn proxy_field_level() -> CaseSpec {
        // Field-level proxy: "count" field deserializes from string "100" via proxy
        CaseSpec::from_str("name = \"test\"\ncount = \"100\"")
    }

    fn proxy_validation_error() -> CaseSpec {
        // Proxy conversion fails with non-numeric string
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn proxy_with_option() -> CaseSpec {
        CaseSpec::from_str("name = \"test\"\ncount = \"42\"")
    }

    fn proxy_with_enum() -> CaseSpec {
        // The suite expects an enum with a newtype variant, represented differently in TOML
        CaseSpec::skip("enum variant with proxy not supported in TOML format")
    }

    fn proxy_with_transparent() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn opaque_proxy() -> CaseSpec {
        // OpaqueType doesn't implement Facet, but OpaqueTypeProxy does
        CaseSpec::from_str("[value]\ninner = 42").with_partial_eq()
    }

    fn opaque_proxy_option() -> CaseSpec {
        // Optional opaque field with proxy
        CaseSpec::from_str("[value]\ninner = 99").with_partial_eq()
    }

    fn transparent_multilevel() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn transparent_option() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn transparent_nonzero() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn flatten_optional_some() -> CaseSpec {
        CaseSpec::from_str("name = \"test\"\nversion = 1\nauthor = \"alice\"")
    }

    fn flatten_optional_none() -> CaseSpec {
        CaseSpec::from_str("name = \"test\"")
    }

    fn flatten_overlapping_fields_error() -> CaseSpec {
        // Two flattened structs both have a "shared" field - should error
        CaseSpec::expect_error(
            "field_a = \"a\"\nfield_b = \"b\"\nshared = 1",
            "Duplicate field",
        )
    }

    fn flatten_multilevel() -> CaseSpec {
        CaseSpec::from_str("top_field = \"top\"\nmid_field = 42\ndeep_field = 100")
    }

    fn flatten_multiple_enums() -> CaseSpec {
        // Multiple flattened enums require solver support that isn't working properly
        CaseSpec::skip("multiple flattened enums solver not supported")
    }

    // -- Scalar cases --

    fn scalar_bool() -> CaseSpec {
        CaseSpec::from_str("yes = true\nno = false")
    }

    fn scalar_integers() -> CaseSpec {
        // TOML integers are i64, so the suite's expected u64::MAX cannot be represented
        CaseSpec::skip("TOML integers are signed 64-bit, cannot represent u64::MAX")
    }

    fn scalar_floats() -> CaseSpec {
        CaseSpec::from_str("float_32 = 1.5\nfloat_64 = 2.25")
    }

    // -- Collection cases --

    fn map_string_keys() -> CaseSpec {
        CaseSpec::from_str("[data]\nalpha = 1\nbeta = 2")
    }

    fn tuple_simple() -> CaseSpec {
        CaseSpec::from_str("triple = [\"hello\", 42, true]")
    }

    fn tuple_nested() -> CaseSpec {
        // Nested tuples in TOML arrays
        CaseSpec::skip("nested tuple serialization format differs")
    }

    fn tuple_empty() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            name = "test"
            empty = []
        "#
        ))
        .without_roundtrip("empty tuple serialization format mismatch")
    }

    fn tuple_single_element() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            name = "test"
            single = [42]
        "#
        ))
    }

    fn tuple_struct_variant() -> CaseSpec {
        CaseSpec::from_str("Pair = [\"test\", 42]")
    }

    fn tuple_newtype_variant() -> CaseSpec {
        CaseSpec::from_str("Some = 99")
    }

    // -- Enum variant cases --

    fn enum_unit_variant() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn enum_untagged() -> CaseSpec {
        CaseSpec::from_str("x = 10\ny = 20")
    }

    fn enum_variant_rename() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn untagged_with_null() -> CaseSpec {
        // TOML has no null
        CaseSpec::skip("TOML has no null literal")
    }

    fn untagged_newtype_variant() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn untagged_as_field() -> CaseSpec {
        CaseSpec::from_str("name = \"test\"\nvalue = 42")
    }

    fn untagged_unit_only() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    // -- Smart pointer cases --

    fn box_wrapper() -> CaseSpec {
        CaseSpec::from_str("inner = 42")
    }

    fn arc_wrapper() -> CaseSpec {
        CaseSpec::from_str("inner = 42")
    }

    fn rc_wrapper() -> CaseSpec {
        CaseSpec::from_str("inner = 42")
    }

    // -- Set cases --

    fn set_btree() -> CaseSpec {
        CaseSpec::from_str("items = [\"alpha\", \"beta\", \"gamma\"]")
    }

    // -- Extended numeric cases --

    fn scalar_integers_16() -> CaseSpec {
        CaseSpec::from_str("signed_16 = -32768\nunsigned_16 = 65535")
    }

    fn scalar_integers_128() -> CaseSpec {
        // TOML integers are 64-bit signed
        CaseSpec::skip("TOML integers are 64-bit signed, cannot represent 128-bit values")
    }

    fn scalar_integers_size() -> CaseSpec {
        CaseSpec::from_str("signed_size = -1000\nunsigned_size = 2000")
    }

    // -- NonZero cases --

    fn nonzero_integers() -> CaseSpec {
        CaseSpec::from_str("nz_u32 = 42\nnz_i64 = -100")
    }

    // -- Borrowed string cases --

    fn cow_str() -> CaseSpec {
        CaseSpec::from_str("owned = \"hello world\"\nmessage = \"borrowed\"")
    }

    // -- Bytes/binary data cases --

    fn bytes_vec_u8() -> CaseSpec {
        CaseSpec::from_str("data = [0, 128, 255, 42]")
    }

    // -- Fixed-size array cases --

    fn array_fixed_size() -> CaseSpec {
        CaseSpec::from_str("values = [1, 2, 3]")
    }

    // -- Unknown field handling cases --

    fn skip_unknown_fields() -> CaseSpec {
        // Input has extra "unknown" field which should be silently skipped
        CaseSpec::from_str("unknown = \"ignored\"\nknown = \"value\"")
            .without_roundtrip("unknown field is not preserved")
    }

    // -- String escape cases --

    fn string_escapes() -> CaseSpec {
        // TOML escape sequences in basic strings
        CaseSpec::from_str("text = \"line1\\nline2\\ttab\\\"quote\\\\backslash\"")
    }

    // -- Unit type cases --

    fn unit_struct() -> CaseSpec {
        // Unit struct serializes as empty table in TOML
        CaseSpec::from_str("")
    }

    // -- Newtype cases --

    fn newtype_u64() -> CaseSpec {
        CaseSpec::from_str("value = 42")
    }

    fn newtype_string() -> CaseSpec {
        CaseSpec::from_str("value = \"hello\"")
    }

    // -- Char cases --

    fn char_scalar() -> CaseSpec {
        CaseSpec::from_str("letter = \"A\"\nemoji = \"\u{1F980}\"")
    }

    // -- HashSet cases --

    fn hashset() -> CaseSpec {
        CaseSpec::from_str("items = [\"alpha\", \"beta\"]")
    }

    // -- Nested collection cases --

    fn vec_nested() -> CaseSpec {
        // Nested Vec in TOML
        CaseSpec::skip("nested Vec serialization format differs")
    }

    // -- Third-party type cases --

    fn uuid() -> CaseSpec {
        // UUID in canonical hyphenated format
        CaseSpec::from_str("id = \"550e8400-e29b-41d4-a716-446655440000\"")
    }

    fn ulid() -> CaseSpec {
        // ULID in standard Crockford Base32 format
        CaseSpec::from_str("id = \"01ARZ3NDEKTSV4RRFFQ69G5FAV\"")
    }

    fn camino_path() -> CaseSpec {
        CaseSpec::from_str("path = \"/home/user/documents\"")
    }

    fn ordered_float() -> CaseSpec {
        CaseSpec::from_str("value = 1.23456")
    }

    fn rust_decimal() -> CaseSpec {
        CaseSpec::from_str("amount = \"24.99\"")
    }

    // -- Scientific notation floats --

    fn scalar_floats_scientific() -> CaseSpec {
        CaseSpec::from_str("large = 1.23e10\nsmall = -4.56e-7\npositive_exp = 5e3")
    }

    // -- Extended escape sequences --

    fn string_escapes_extended() -> CaseSpec {
        // TOML uses escape sequences for control characters
        CaseSpec::from_str(
            "backspace = \"hello\\bworld\"\nformfeed = \"page\\fbreak\"\ncarriage_return = \"line\\rreturn\"\ncontrol_char = \"\\u0001\"",
        )
    }

    // -- Unsized smart pointer cases --

    fn box_str() -> CaseSpec {
        CaseSpec::from_str("inner = \"hello world\"")
    }

    fn arc_str() -> CaseSpec {
        CaseSpec::from_str("inner = \"hello world\"")
    }

    fn rc_str() -> CaseSpec {
        CaseSpec::from_str("inner = \"hello world\"")
    }

    fn arc_slice() -> CaseSpec {
        CaseSpec::from_str("inner = [1, 2, 3, 4]")
    }

    // -- Yoke cases --

    #[cfg(feature = "yoke")]
    fn yoke_cow_str() -> CaseSpec {
        CaseSpec::from_str(r#"value = "hello yoke""#).with_partial_eq()
    }

    #[cfg(feature = "yoke")]
    fn yoke_custom() -> CaseSpec {
        CaseSpec::from_str(r#"value = "hello|yoke""#).with_partial_eq()
    }

    // -- Extended NonZero cases --

    fn nonzero_integers_extended() -> CaseSpec {
        CaseSpec::from_str(
            "nz_u8 = 255\nnz_i8 = -128\nnz_u16 = 65535\nnz_i16 = -32768\nnz_u128 = 1\nnz_i128 = -1\nnz_usize = 1000\nnz_isize = -500",
        )
        .without_roundtrip("i128/u128 may serialize as strings")
    }

    // -- DateTime type cases --

    fn time_offset_datetime() -> CaseSpec {
        CaseSpec::from_str("created_at = 2023-01-15T12:34:56Z")
    }

    fn jiff_timestamp() -> CaseSpec {
        CaseSpec::from_str("created_at = 2023-12-31T11:30:00Z")
    }

    fn jiff_civil_datetime() -> CaseSpec {
        CaseSpec::from_str("created_at = 2024-06-19T15:22:45")
    }

    fn jiff_civil_date() -> CaseSpec {
        CaseSpec::from_str("date = 2024-06-19")
    }

    fn jiff_civil_time() -> CaseSpec {
        CaseSpec::from_str("time = 15:22:45")
    }

    fn chrono_datetime_utc() -> CaseSpec {
        CaseSpec::from_str("created_at = 2023-01-15T12:34:56Z")
    }

    fn chrono_naive_datetime() -> CaseSpec {
        CaseSpec::from_str("created_at = 2023-01-15T12:34:56")
    }

    fn chrono_naive_date() -> CaseSpec {
        CaseSpec::from_str("birth_date = 2023-01-15")
    }

    fn chrono_naive_time() -> CaseSpec {
        CaseSpec::from_str("alarm_time = 12:34:56")
    }

    fn chrono_in_vec() -> CaseSpec {
        CaseSpec::from_str("timestamps = [2023-01-01T00:00:00Z, 2023-06-15T12:30:00Z]")
    }

    fn chrono_duration() -> CaseSpec {
        CaseSpec::from_str("duration = [3600, 500000000]")
    }

    fn chrono_duration_negative() -> CaseSpec {
        CaseSpec::from_str("duration = [-90, -250000000]")
    }

    // -- Standard library time cases --

    fn std_duration() -> CaseSpec {
        CaseSpec::from_str("duration = [3600, 500000000]")
    }

    // -- Bytes crate cases --

    fn bytes_bytes() -> CaseSpec {
        CaseSpec::from_str("data = [1, 2, 3, 4, 255]")
    }

    fn bytes_bytes_mut() -> CaseSpec {
        CaseSpec::from_str("data = [1, 2, 3, 4, 255]")
    }

    // -- String optimization crate cases --

    fn bytestring() -> CaseSpec {
        CaseSpec::from_str("value = \"hello world\"")
    }

    fn compact_string() -> CaseSpec {
        CaseSpec::from_str("value = \"hello world\"")
    }

    fn smartstring() -> CaseSpec {
        CaseSpec::from_str("value = \"hello world\"")
    }

    fn smol_str() -> CaseSpec {
        CaseSpec::from_str("value = \"hello world\"")
    }

    fn iddqd_id_hash_map() -> CaseSpec {
        // IdHashMap serializes as array of values (Set semantics)
        // Single element ensures deterministic order for roundtrip
        CaseSpec::from_str(indoc!(
            r#"
            [[items]]
            id = 1
            name = "Alice"
        "#
        ))
        .without_roundtrip("TOML array-of-tables format differs from serialized output")
    }

    fn iddqd_id_ord_map() -> CaseSpec {
        // IdOrdMap serializes as array of values (Set semantics), ordered by key
        CaseSpec::from_str(indoc!(
            r#"
            [[items]]
            id = 1
            name = "Alice"
        "#
        ))
        .without_roundtrip("TOML array-of-tables format differs from serialized output")
    }

    fn iddqd_bi_hash_map() -> CaseSpec {
        // BiHashMap serializes as array of values (Set semantics)
        // Single element ensures deterministic order for roundtrip
        CaseSpec::from_str(indoc!(
            r#"
            [[items]]
            id = 1
            code = "A001"
            name = "Alice"
        "#
        ))
        .without_roundtrip("TOML array-of-tables format differs from serialized output")
    }

    fn iddqd_tri_hash_map() -> CaseSpec {
        // TriHashMap serializes as array of values (Set semantics)
        // Single element ensures deterministic order for roundtrip
        CaseSpec::from_str(indoc!(
            r#"
            [[items]]
            id = 1
            code = "A001"
            email = "alice@example.com"
            name = "Alice"
        "#
        ))
        .without_roundtrip("TOML array-of-tables format differs from serialized output")
    }

    // -- Dynamic value cases --

    fn value_null() -> CaseSpec {
        // TOML has no null
        CaseSpec::skip("TOML has no null literal")
    }

    fn value_bool() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn value_integer() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn value_float() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn value_string() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn value_array() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn value_object() -> CaseSpec {
        CaseSpec::from_str("name = \"test\"\ncount = 42")
    }

    fn numeric_enum() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn signed_numeric_enum() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    fn inferred_numeric_enum() -> CaseSpec {
        // TOML cannot have bare values at root level
        CaseSpec::skip("TOML cannot have bare values at root level")
    }

    // ── Network type cases ──

    fn net_ip_addr_v4() -> CaseSpec {
        CaseSpec::from_str("addr = \"192.168.1.1\"")
    }

    fn net_ip_addr_v6() -> CaseSpec {
        CaseSpec::from_str("addr = \"2001:db8::1\"")
    }

    fn net_ipv4_addr() -> CaseSpec {
        CaseSpec::from_str("addr = \"127.0.0.1\"")
    }

    fn net_ipv6_addr() -> CaseSpec {
        CaseSpec::from_str("addr = \"::1\"")
    }

    fn net_socket_addr_v4() -> CaseSpec {
        CaseSpec::from_str("addr = \"192.168.1.1:8080\"")
    }

    fn net_socket_addr_v6() -> CaseSpec {
        CaseSpec::from_str("addr = \"[2001:db8::1]:443\"")
    }

    fn net_socket_addr_v4_explicit() -> CaseSpec {
        CaseSpec::from_str("addr = \"10.0.0.1:3000\"")
    }

    fn net_socket_addr_v6_explicit() -> CaseSpec {
        CaseSpec::from_str("addr = \"[fe80::1]:9000\"")
    }
}

fn main() {
    let args = Arguments::from_args();

    let trials: Vec<Trial> = all_cases::<TomlSlice>()
        .into_iter()
        .map(|case| {
            let case = Arc::new(case);
            let name = format!("{}::{}", TomlSlice::format_name(), case.id);
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
