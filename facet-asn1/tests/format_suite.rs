#![forbid(unsafe_code)]

use facet::Facet;
use facet_asn1::{Asn1Error, Asn1Parser, to_vec};
use facet_format::DeserializeError;
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
use libtest_mimic::{Arguments, Failed, Trial};

struct Asn1Slice;

impl FormatSuite for Asn1Slice {
    type Error = DeserializeError<Asn1Error>;

    fn format_name() -> &'static str {
        "facet-asn1/slice"
    }

    fn highlight_language() -> Option<&'static str> {
        None // Binary format, no syntax highlighting
    }

    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        T: Facet<'static> + core::fmt::Debug,
    {
        use facet_format::FormatDeserializer;
        let parser = Asn1Parser::new(input);
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

    // ASN.1 is a positional binary format - test cases need binary input,
    // not JSON strings. Most suite cases use JSON string input which won't work.
    // We skip cases that require text-based input and only enable those that
    // can work with binary roundtrip testing.

    fn struct_single_field() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn sequence_numbers() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn sequence_mixed_scalars() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn struct_nested() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn enum_complex() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn attr_rename_field() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn attr_rename_all_camel() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn attr_default_field() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn attr_default_struct() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn attr_default_function() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn option_none() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn option_some() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn option_null() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn attr_skip_serializing() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn attr_skip_serializing_if() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn attr_skip() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn enum_internally_tagged() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn enum_adjacently_tagged() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn struct_flatten() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn transparent_newtype() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn deny_unknown_fields() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn error_type_mismatch_string_to_int() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn error_type_mismatch_object_to_array() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn error_missing_required_field() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn attr_alias() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn attr_rename_vs_alias_precedence() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn attr_rename_all_kebab() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn attr_rename_all_screaming() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn attr_rename_unicode() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn attr_rename_special_chars() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, field names are not in wire format")
    }

    fn proxy_container() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn proxy_field_level() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn proxy_validation_error() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn proxy_with_option() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn proxy_with_enum() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn proxy_with_transparent() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn opaque_proxy() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn opaque_proxy_option() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn transparent_multilevel() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn transparent_option() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn transparent_nonzero() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn flatten_optional_some() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn flatten_optional_none() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn flatten_overlapping_fields_error() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn flatten_multilevel() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn flatten_multiple_enums() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn scalar_bool() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn scalar_integers() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn scalar_floats() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn map_string_keys() -> CaseSpec {
        CaseSpec::skip("ASN.1 doesn't have native map support")
    }

    fn tuple_simple() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn tuple_nested() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn tuple_empty() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn tuple_single_element() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn tuple_struct_variant() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn tuple_newtype_variant() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn enum_unit_variant() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn numeric_enum() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn signed_numeric_enum() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn inferred_numeric_enum() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn enum_untagged() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn enum_variant_rename() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn untagged_with_null() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn untagged_newtype_variant() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn untagged_as_field() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn untagged_unit_only() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn box_wrapper() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn arc_wrapper() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn rc_wrapper() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn set_btree() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn scalar_integers_16() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn scalar_integers_128() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn scalar_integers_size() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn nonzero_integers() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn cow_str() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn bytes_vec_u8() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn array_fixed_size() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn skip_unknown_fields() -> CaseSpec {
        CaseSpec::skip("ASN.1 is positional, doesn't have field names to skip")
    }

    fn string_escapes() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, no string escape sequences")
    }

    fn unit_struct() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn newtype_u64() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn newtype_string() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn char_scalar() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn hashset() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn vec_nested() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    // Third-party types - all need binary input, not JSON strings
    fn uuid() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn ulid() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn camino_path() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn ordered_float() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn rust_decimal() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn scalar_floats_scientific() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, no scientific notation")
    }

    fn string_escapes_extended() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, no string escape sequences")
    }

    fn box_str() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn arc_str() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn rc_str() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn arc_slice() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn nonzero_integers_extended() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn time_offset_datetime() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn jiff_timestamp() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn jiff_civil_datetime() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn chrono_datetime_utc() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn chrono_naive_datetime() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn chrono_naive_date() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn chrono_naive_time() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn chrono_in_vec() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn chrono_duration() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn chrono_duration_negative() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn bytes_bytes() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn bytes_bytes_mut() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn bytestring() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn compact_string() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn smartstring() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn smol_str() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn iddqd_id_hash_map() -> CaseSpec {
        // IdHashMap serializes as array of values (Set semantics)
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn value_null() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, DynamicValue not supported")
    }

    fn value_bool() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, DynamicValue not supported")
    }

    fn value_integer() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, DynamicValue not supported")
    }

    fn value_float() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, DynamicValue not supported")
    }

    fn value_string() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, DynamicValue not supported")
    }

    fn value_array() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, DynamicValue not supported")
    }

    fn value_object() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, DynamicValue not supported")
    }

    // ── Network type cases ──

    fn net_ip_addr_v4() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn net_ip_addr_v6() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn net_ipv4_addr() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn net_ipv6_addr() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn net_socket_addr_v4() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn net_socket_addr_v6() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn net_socket_addr_v4_explicit() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }

    fn net_socket_addr_v6_explicit() -> CaseSpec {
        CaseSpec::skip("ASN.1 is a binary format, requires binary input not JSON strings")
    }
}

fn main() {
    use std::sync::Arc;

    let args = Arguments::from_args();
    let cases: Vec<Arc<_>> = all_cases::<Asn1Slice>().into_iter().map(Arc::new).collect();

    let mut trials: Vec<Trial> = Vec::new();

    for case in &cases {
        let name = format!("{}::{}", Asn1Slice::format_name(), case.id);
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
