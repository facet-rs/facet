#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
use facet_lua::{LuaParser, to_vec};
use libtest_mimic::{Arguments, Failed, Trial};

struct LuaSlice;

impl FormatSuite for LuaSlice {
    type Error = DeserializeError;

    fn format_name() -> &'static str {
        "facet-lua/slice"
    }

    fn highlight_language() -> Option<&'static str> {
        Some("lua")
    }

    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        T: Facet<'static> + core::fmt::Debug,
    {
        let s = core::str::from_utf8(input).map_err(|e| {
            let mut context = [0u8; 16];
            let context_len = e.valid_up_to().min(16);
            context[..context_len].copy_from_slice(&input[..context_len]);
            facet_format::DeserializeErrorKind::InvalidUtf8 {
                context,
                context_len: context_len as u8,
            }
            .with_span(facet_reflect::Span::new(e.valid_up_to(), 1))
        })?;
        let mut parser = LuaParser::new(s);
        let mut de = FormatDeserializer::new_owned(&mut parser);
        de.deserialize_root::<T>()
    }

    fn serialize<T>(value: &T) -> Option<Result<Vec<u8>, String>>
    where
        for<'facet> T: Facet<'facet>,
        T: core::fmt::Debug,
    {
        Some(to_vec(value).map_err(|e| e.to_string()))
    }

    // ── Core cases ──

    fn struct_single_field() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "facet"}"#)
    }

    fn sequence_numbers() -> CaseSpec {
        CaseSpec::from_str("{1, 2, 3}")
    }

    fn sequence_mixed_scalars() -> CaseSpec {
        CaseSpec::from_str("{-1, 4.625, nil, true}")
    }

    fn struct_nested() -> CaseSpec {
        CaseSpec::from_str(
            r#"{id = 42, child = {code = "alpha", active = true}, tags = {"core", "json"}}"#,
        )
    }

    fn enum_complex() -> CaseSpec {
        CaseSpec::from_str(r#"{Label = {name = "facet", level = 7}}"#)
    }

    // ── Attribute cases ──

    fn attr_rename_field() -> CaseSpec {
        CaseSpec::from_str(r#"{userName = "alice", age = 30}"#)
    }

    fn attr_rename_all_camel() -> CaseSpec {
        CaseSpec::from_str(r#"{firstName = "Jane", lastName = "Doe", isActive = true}"#)
    }

    fn attr_default_field() -> CaseSpec {
        CaseSpec::from_str(r#"{required = "present"}"#)
    }

    fn attr_default_struct() -> CaseSpec {
        CaseSpec::from_str(r#"{count = 123}"#)
    }

    fn attr_default_function() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "hello"}"#)
    }

    fn option_none() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test"}"#)
    }

    fn option_some() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test", nickname = "nick"}"#)
    }

    fn option_null() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test", nickname = nil}"#)
            .without_roundtrip("nil serializes as missing field, not explicit nil")
    }

    fn attr_skip_serializing() -> CaseSpec {
        CaseSpec::from_str(r#"{visible = "shown"}"#)
    }

    fn attr_skip_serializing_if() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test"}"#)
    }

    fn attr_skip() -> CaseSpec {
        CaseSpec::from_str(r#"{visible = "data"}"#)
    }

    // ── Enum tagging cases ──

    fn enum_internally_tagged() -> CaseSpec {
        CaseSpec::from_str(r#"{type = "Circle", radius = 5.0}"#)
    }

    fn enum_adjacently_tagged() -> CaseSpec {
        CaseSpec::from_str(r#"{t = "Message", c = "hello"}"#)
    }

    // ── Advanced cases ──

    fn struct_flatten() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "point", x = 10, y = 20}"#)
    }

    fn transparent_newtype() -> CaseSpec {
        CaseSpec::from_str(r#"{id = 42, name = "alice"}"#)
    }

    // ── Error cases ──

    fn deny_unknown_fields() -> CaseSpec {
        CaseSpec::expect_error(r#"{foo = "abc", bar = 42, baz = true}"#, "unknown field")
    }

    fn error_type_mismatch_string_to_int() -> CaseSpec {
        CaseSpec::expect_error(r#"{value = "not_a_number"}"#, "failed to parse")
    }

    fn error_type_mismatch_object_to_array() -> CaseSpec {
        CaseSpec::expect_error(
            r#"{items = {wrong = "structure"}}"#,
            "got object, expected array",
        )
    }

    fn error_missing_required_field() -> CaseSpec {
        CaseSpec::expect_error(r#"{name = "Alice", age = 30}"#, "missing field")
    }

    // ── Alias cases ──

    fn attr_alias() -> CaseSpec {
        CaseSpec::from_str(r#"{old_name = "value", count = 5}"#)
            .without_roundtrip("alias is only for deserialization, serializes as new_name")
    }

    fn attr_rename_vs_alias_precedence() -> CaseSpec {
        CaseSpec::from_str(r#"{officialName = "test", id = 1}"#)
    }

    fn attr_rename_all_kebab() -> CaseSpec {
        CaseSpec::from_str(r#"{["first-name"] = "John", ["last-name"] = "Doe", ["user-id"] = 42}"#)
    }

    fn attr_rename_all_screaming() -> CaseSpec {
        CaseSpec::from_str(r#"{API_KEY = "secret-123", MAX_RETRY_COUNT = 5}"#)
    }

    fn attr_rename_unicode() -> CaseSpec {
        // Unicode keys use ["key"] bracket syntax in Lua
        CaseSpec::from_str(r#"{["🎉"] = "party"}"#)
    }

    fn attr_rename_special_chars() -> CaseSpec {
        CaseSpec::from_str(r#"{["@type"] = "node"}"#)
    }

    // ── Proxy cases ──

    fn proxy_container() -> CaseSpec {
        CaseSpec::from_str(r#""42""#)
    }

    fn proxy_field_level() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test", count = "100"}"#)
    }

    fn proxy_validation_error() -> CaseSpec {
        CaseSpec::expect_error(r#""not_a_number""#, "invalid digit")
    }

    fn proxy_with_option() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test", count = "42"}"#)
    }

    fn proxy_with_enum() -> CaseSpec {
        CaseSpec::from_str(r#"{Value = "99"}"#)
    }

    fn proxy_with_transparent() -> CaseSpec {
        CaseSpec::from_str(r#""42""#)
    }

    fn opaque_proxy() -> CaseSpec {
        CaseSpec::from_str(r#"{value = {inner = 42}}"#).with_partial_eq()
    }

    fn opaque_proxy_option() -> CaseSpec {
        CaseSpec::from_str(r#"{value = {inner = 99}}"#).with_partial_eq()
    }

    fn transparent_multilevel() -> CaseSpec {
        CaseSpec::from_str("42")
    }

    fn transparent_option() -> CaseSpec {
        CaseSpec::from_str("99")
    }

    fn transparent_nonzero() -> CaseSpec {
        CaseSpec::from_str("42")
    }

    fn flatten_optional_some() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test", version = 1, author = "alice"}"#)
    }

    fn flatten_optional_none() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test"}"#)
    }

    fn flatten_overlapping_fields_error() -> CaseSpec {
        CaseSpec::expect_error(
            r#"{field_a = "a", field_b = "b", shared = 1}"#,
            "Duplicate field",
        )
    }

    fn flatten_multilevel() -> CaseSpec {
        CaseSpec::from_str(r#"{top_field = "top", mid_field = 42, deep_field = 100}"#)
    }

    fn flatten_multiple_enums() -> CaseSpec {
        CaseSpec::from_str(
            r#"{name = "service", Password = {password = "secret"}, Tcp = {port = 8080}}"#,
        )
        .without_roundtrip("serialization of flattened enums not yet supported")
    }

    // ── Scalar cases ──

    fn scalar_bool() -> CaseSpec {
        CaseSpec::from_str(r#"{yes = true, no = false}"#)
    }

    fn scalar_integers() -> CaseSpec {
        CaseSpec::from_str(
            r#"{signed_8 = -128, unsigned_8 = 255, signed_32 = -2147483648, unsigned_32 = 4294967295, signed_64 = -9223372036854775808, unsigned_64 = 18446744073709551615}"#,
        )
    }

    fn scalar_floats() -> CaseSpec {
        CaseSpec::from_str(r#"{float_32 = 1.5, float_64 = 2.25}"#)
    }

    fn scalar_floats_scientific() -> CaseSpec {
        CaseSpec::from_str(r#"{large = 1.23e10, small = -4.56e-7, positive_exp = 5e3}"#)
    }

    // ── Network type cases ──

    fn net_ip_addr_v4() -> CaseSpec {
        CaseSpec::from_str(r#"{addr = "192.168.1.1"}"#)
    }

    fn net_ip_addr_v6() -> CaseSpec {
        CaseSpec::from_str(r#"{addr = "2001:db8::1"}"#)
    }

    fn net_ipv4_addr() -> CaseSpec {
        CaseSpec::from_str(r#"{addr = "127.0.0.1"}"#)
    }

    fn net_ipv6_addr() -> CaseSpec {
        CaseSpec::from_str(r#"{addr = "::1"}"#)
    }

    fn net_socket_addr_v4() -> CaseSpec {
        CaseSpec::from_str(r#"{addr = "192.168.1.1:8080"}"#)
    }

    fn net_socket_addr_v6() -> CaseSpec {
        CaseSpec::from_str(r#"{addr = "[2001:db8::1]:443"}"#)
    }

    fn net_socket_addr_v4_explicit() -> CaseSpec {
        CaseSpec::from_str(r#"{addr = "10.0.0.1:3000"}"#)
    }

    fn net_socket_addr_v6_explicit() -> CaseSpec {
        CaseSpec::from_str(r#"{addr = "[fe80::1]:9000"}"#)
    }

    // ── Collection cases ──

    fn map_string_keys() -> CaseSpec {
        CaseSpec::from_str(r#"{data = {alpha = 1, beta = 2}}"#)
    }

    fn tuple_simple() -> CaseSpec {
        CaseSpec::from_str(r#"{triple = {"hello", 42, true}}"#)
    }

    fn tuple_nested() -> CaseSpec {
        CaseSpec::from_str(r#"{outer = {{1, 2}, {"test", true}}}"#)
    }

    fn tuple_empty() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test", empty = {}}"#)
            .without_roundtrip("empty tuple serialization format mismatch")
    }

    fn tuple_single_element() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test", single = {42}}"#)
    }

    fn tuple_struct_variant() -> CaseSpec {
        CaseSpec::from_str(r#"{Pair = {"test", 42}}"#)
    }

    fn tuple_newtype_variant() -> CaseSpec {
        CaseSpec::from_str(r#"{Some = 99}"#)
    }

    // ── Enum variant cases ──

    fn enum_unit_variant() -> CaseSpec {
        CaseSpec::from_str(r#""Active""#)
    }

    fn numeric_enum() -> CaseSpec {
        CaseSpec::from_str("1")
    }

    fn signed_numeric_enum() -> CaseSpec {
        CaseSpec::from_str("-1")
    }

    fn inferred_numeric_enum() -> CaseSpec {
        CaseSpec::from_str(r#""0""#)
    }

    fn enum_untagged() -> CaseSpec {
        CaseSpec::from_str(r#"{x = 10, y = 20}"#)
    }

    fn enum_variant_rename() -> CaseSpec {
        CaseSpec::from_str(r#""enabled""#)
    }

    fn untagged_with_null() -> CaseSpec {
        CaseSpec::from_str("nil")
            .without_roundtrip("unit variant serializes to variant name, not nil")
    }

    fn untagged_newtype_variant() -> CaseSpec {
        CaseSpec::from_str(r#""test""#)
    }

    fn untagged_as_field() -> CaseSpec {
        CaseSpec::from_str(r#"{name = "test", value = 42}"#)
    }

    fn untagged_unit_only() -> CaseSpec {
        CaseSpec::from_str(r#""Alpha""#)
    }

    // ── Smart pointer cases ──

    fn box_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"{inner = 42}"#)
    }

    fn arc_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"{inner = 42}"#)
    }

    fn rc_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"{inner = 42}"#)
    }

    fn box_str() -> CaseSpec {
        CaseSpec::from_str(r#"{inner = "hello world"}"#)
    }

    fn arc_str() -> CaseSpec {
        CaseSpec::from_str(r#"{inner = "hello world"}"#)
    }

    fn rc_str() -> CaseSpec {
        CaseSpec::from_str(r#"{inner = "hello world"}"#)
    }

    fn arc_slice() -> CaseSpec {
        CaseSpec::from_str(r#"{inner = {1, 2, 3, 4}}"#)
    }

    // ── Set cases ──

    fn set_btree() -> CaseSpec {
        CaseSpec::from_str(r#"{items = {"alpha", "beta", "gamma"}}"#)
    }

    // ── Extended numeric cases ──

    fn scalar_integers_16() -> CaseSpec {
        CaseSpec::from_str(r#"{signed_16 = -32768, unsigned_16 = 65535}"#)
    }

    fn scalar_integers_128() -> CaseSpec {
        CaseSpec::from_str(
            r#"{signed_128 = -170141183460469231731687303715884105728, unsigned_128 = 340282366920938463463374607431768211455}"#,
        )
        .without_roundtrip("i128/u128 serialize as strings, not native Lua numbers")
    }

    fn scalar_integers_size() -> CaseSpec {
        CaseSpec::from_str(r#"{signed_size = -1000, unsigned_size = 2000}"#)
    }

    // ── NonZero cases ──

    fn nonzero_integers() -> CaseSpec {
        CaseSpec::from_str(r#"{nz_u32 = 42, nz_i64 = -100}"#)
    }

    fn nonzero_integers_extended() -> CaseSpec {
        CaseSpec::from_str(
            r#"{nz_u8 = 255, nz_i8 = -128, nz_u16 = 65535, nz_i16 = -32768, nz_u128 = 1, nz_i128 = -1, nz_usize = 1000, nz_isize = -500}"#,
        )
        .without_roundtrip("i128/u128 serialize as strings, not native Lua numbers")
    }

    // ── Borrowed string cases ──

    fn cow_str() -> CaseSpec {
        CaseSpec::from_str(r#"{owned = "hello world", message = "borrowed"}"#)
    }

    // ── Bytes/binary data cases ──

    fn bytes_vec_u8() -> CaseSpec {
        CaseSpec::from_str(r#"{data = {0, 128, 255, 42}}"#)
    }

    // ── Fixed-size array cases ──

    fn array_fixed_size() -> CaseSpec {
        CaseSpec::from_str(r#"{values = {1, 2, 3}}"#)
    }

    // ── Unknown field handling cases ──

    fn skip_unknown_fields() -> CaseSpec {
        CaseSpec::from_str(r#"{unknown = "ignored", known = "value"}"#)
            .without_roundtrip("unknown field is not preserved")
    }

    // ── String escape cases ──

    fn string_escapes() -> CaseSpec {
        CaseSpec::from_str(r#"{text = "line1\nline2\ttab\"quote\\backslash"}"#)
    }

    fn string_escapes_extended() -> CaseSpec {
        // Lua uses \ddd decimal escapes for control chars
        CaseSpec::from_str(
            r#"{backspace = "hello\8world", formfeed = "page\12break", carriage_return = "line\rreturn", control_char = "\1"}"#,
        )
    }

    // ── Unit type cases ──

    fn unit_struct() -> CaseSpec {
        CaseSpec::from_str(r#"{}"#)
    }

    // ── Newtype cases ──

    fn newtype_u64() -> CaseSpec {
        CaseSpec::from_str(r#"{value = 42}"#)
    }

    fn newtype_string() -> CaseSpec {
        CaseSpec::from_str(r#"{value = "hello"}"#)
    }

    // ── Char cases ──

    fn char_scalar() -> CaseSpec {
        CaseSpec::from_str(r#"{letter = "A", emoji = "🦀"}"#)
    }

    // ── HashSet cases ──

    fn hashset() -> CaseSpec {
        CaseSpec::from_str(r#"{items = {"alpha", "beta"}}"#)
    }

    // ── Nested collection cases ──

    fn vec_nested() -> CaseSpec {
        CaseSpec::from_str(r#"{matrix = {{1, 2}, {3, 4, 5}}}"#)
    }

    // ── Duration case ──

    fn std_duration() -> CaseSpec {
        CaseSpec::from_str(r#"{duration = {3600, 500000000}}"#)
    }
}

fn main() {
    use std::sync::Arc;

    let args = Arguments::from_args();
    let cases: Vec<Arc<_>> = all_cases::<LuaSlice>().into_iter().map(Arc::new).collect();

    let mut trials: Vec<Trial> = Vec::new();

    for case in &cases {
        let name = format!("{}::{}", LuaSlice::format_name(), case.id);
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

    libtest_mimic::run(&args, trials).exit()
}
