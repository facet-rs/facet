+++
title = "Format crates comparison"
weight = 2
+++

This document tracks feature parity across all facet format crates.

Legend:
- ✅ = Fully supported with tests
- 🟡 = Partial support or untested
- 🚫 = Not supported
- ➖ = Not applicable to this format

Note: `msgp` = `facet-msgpack`, `pcrd` = `facet-postcard` (shortened for column width)

## Overview

Note: S = Serialization, D = Deserialization

| Crate | Direction | Format Type | Parser | Showcase |
|-------|-----------|-------------|--------|----------|
| [facet-json](https://docs.rs/facet-json) | SD | Text | Event-based (custom) | 🚫 |
| [facet-yaml](https://docs.rs/facet-yaml) | SD | Text | Event-based ([saphyr-parser](https://docs.rs/saphyr-parser)) | 🚫 |
| [facet-toml](https://docs.rs/facet-toml) | SD | Text | Event-based ([toml_parser](https://docs.rs/toml_parser)) | 🚫 |
| [facet-xml](https://docs.rs/facet-xml) | SD | Text | Event-based ([quick-xml](https://docs.rs/quick-xml)) | 🚫 |
| [facet-msgpack](https://docs.rs/facet-msgpack) | SD | Binary | Event-based (custom) | 🚫 |
| [facet-postcard](https://docs.rs/facet-postcard) | SD | Binary | Event-based (custom) | 🚫 |
| [facet-asn1](https://docs.rs/facet-asn1) | S | Binary | (Custom) | 🚫 |
| [facet-xdr](https://docs.rs/facet-xdr) | S | Binary | (Custom) | 🚫 |
| [facet-args](https://docs.rs/facet-args) | D | CLI | (Custom) | [View](/showcases/args) |
| [facet-urlencoded](https://docs.rs/facet-urlencoded) | D | Text | [form_urlencoded](https://docs.rs/form_urlencoded) | 🚫 |
| [facet-csv](https://docs.rs/facet-csv) | S | Text | (Custom) | 🚫 |

## Scalar types

| Type | json | yaml | toml | xml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|------|------|------|------|-----|------|------|------|-----|------|--------|-----|
| `bool` | <span title="json_read_bool, test_bool_serialization">✅</span> | <span title="test_bool">✅</span> | <span title="test_bool">✅</span> | 🟡 | <span title="test_bool, msgpack_read_bool">✅</span> | <span title="bool_tests">✅</span> | ✅ | ✅ | <span title="test_missing_bool_is_false, test_bool_chain_simple">✅</span> | 🚫 | ✅ |
| `u8..u64` | <span title="json_read_more_types, test_integer_types_serialization">✅</span> | <span title="test_u8, test_u16, test_u32, test_u64">✅</span> | <span title="test_u8..test_u64">✅</span> | 🟡 | <span title="test_u8..test_u64">✅</span> | <span title="primitives.rs u8..u64 tests">✅</span> | ✅ | ✅ | <span title="test_arg_parse_nums">✅</span> | <span title="test_basic_urlencoded">✅</span> | ✅ |
| `i8..i64` | <span title="json_read_more_types, test_integer_types_serialization">✅</span> | <span title="test_i8, test_i16, test_i32, test_i64">✅</span> | <span title="test_i8..test_i64">✅</span> | 🟡 | <span title="test_i8..test_i64">✅</span> | <span title="primitives.rs i8..i64 tests">✅</span> | ✅ | ✅ | <span title="test_arg_parse_nums">✅</span> | 🚫 | ✅ |
| `u128/i128` | <span title="json_read_more_types">✅</span> | <span title="test_u128, test_i128, test_u128_large, test_i128_large">✅</span> | 🟡 | 🟡 | 🟡 | <span title="u128_tests, i128_tests">✅</span> | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| `f32/f64` | <span title="test_f64_serialization, json_read_more_types">✅</span> | <span title="test_f32, test_f64">✅</span> | <span title="test_f32, test_f64">✅</span> | 🟡 | 🚫 | <span title="f32_tests, f64_tests, special_floats">✅</span> | ✅ | ✅ | <span title="test_arg_parse_nums, test_inf_float_parsing">✅</span> | 🚫 | ✅ |
| `char` | 🚫 | ✅ | 🟡 | 🟡 | <span title="test_char">✅</span> | <span title="char_tests">✅</span> | 🟡 | 🟡 | 🚫 | 🚫 | ✅ |
| NonZero integers | <span title="test_nonzero_types_serialization, read_nonzero_one, write_nonzero">✅</span> | 🟡 | 🚫 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🟡 | 🟡 | ✅ |

## String types

All formats support `String`, `&str` (with best-effort borrowing), and `Cow<str>`.

## Lists, sets, and maps

| Type | json | yaml | toml | xml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|------|------|------|------|-----|------|------|------|-----|------|--------|-----|
| `Vec<T>` | <span title="json_read_vec, test_nested_arrays, test_vec_of_structs_fine">✅</span> | <span title="test_deserialize_primitive_list, test_scalar_list">✅</span> | <span title="test_scalar_list, test_nested_lists">✅</span> | <span title="simple_enum::test_enum_in_list, real_world::test_svg_simple">✅</span> | <span title="msgpack_deserialize_vec, test_nested_arrays">✅</span> | <span title="vec_tests">✅</span> | <span title="test_deserialize_octet_string, test_serialize_octet_string">✅</span> | <span title="test_serialize_file_example, test_option_is_the_same_as_vec">✅</span> | <span title="test_simplest_value_singleton_list_named">✅</span> | 🚫 | 🚫 |
| `[T; N]` (arrays) | <span title="test_simple_array, test_array_field_parsing">✅</span> | ✅ | <span title="test_fixed_size_array, test_fixed_size_array_roundtrip">✅</span> | 🟡 | ✅ | <span title="array_tests">✅</span> | ✅ | ✅ | 🟡 | 🚫 | 🚫 |
| `HashSet<T>` | <span title="test_set, test_set_with_multiple_entries">✅</span> | ✅ | 🚫 | 🟡 | 🚫 | <span title="hashset_tests">✅</span> | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 |
| `BTreeSet<T>` | ✅ | ✅ | 🚫 | 🟡 | 🚫 | <span title="btreeset_tests">✅</span> | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 |
| `HashMap<K, V>` | <span title="json_read_hashmap, test_map_with_string_keys">✅</span> | <span title="test_deserialize_string_to_string_map, test_scalar_map">✅</span> | <span title="test_scalar_map, test_struct_map">✅</span> | 🟡 | <span title="msgpack_deserialize_hashmap">✅</span> | <span title="hashmap_tests">✅</span> | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 |
| `BTreeMap<K, V>` | ✅ | ✅ | ✅ | 🟡 | 🟡 | <span title="btreemap_tests">✅</span> | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 |
| Non-string map keys | <span title="serialize_hashmap_i32_number_keys, test_hashmap_u32_u32_roundtrip">✅</span> | <span title="test_invalid_map_key">✅</span> | 🚫 | 🚫 | ✅ | <span title="test_int_key_hashmap">✅</span> | 🚫 | 🚫 | ➖ | ➖ | ➖ |

## Compound types

| Type | json | yaml | toml | xml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|------|------|------|------|-----|------|------|------|-----|------|--------|-----|
| `Option<T>` | <span title="test_from_json_with_option, test_from_json_with_nested_options">✅</span> | <span title="test_optional_scalar">✅</span> | <span title="test_option_scalar, test_nested_option">✅</span> | <span title="minimal::test_optional_present, minimal::test_optional_absent">✅</span> | <span title="test_from_msgpack_with_option, test_option_some, test_option_none">✅</span> | <span title="option_tests">✅</span> | <span title="test_deserialize_point_x, test_deserialize_point_y">🟡</span> | <span title="test_option_is_the_same_as_vec">✅</span> | <span title="test_optional_subcommand">✅</span> | 🚫 | ✅ |
| `Result<T, E>` | 🚫 | 🟡 | 🚫 | 🚫 | 🚫 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |

## Smart pointers

| Type | json | yaml | toml | xml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|------|------|------|------|-----|------|------|------|-----|------|--------|-----|
| `Box<T>` | <span title="test_deserialize_boxed_struct, test_serialize_boxed_struct, test_roundtrip_box_str">✅</span> | ✅ | 🚫 | 🟡 | 🟡 | <span title="box_tests">✅</span> | 🟡 | 🟡 | 🟡 | 🚫 | 🟡 |
| `Rc<T>` | <span title="test_roundtrip_rc_str">✅</span> | ✅ | 🚫 | 🟡 | 🟡 | <span title="rc_tests">✅</span> | 🟡 | 🟡 | 🟡 | 🚫 | 🟡 |
| `Arc<T>` | <span title="test_deserialize_struct_with_arc_field, test_roundtrip_arc_foobar, test_roundtrip_arc_str">✅</span> | <span title="test_deserialize_arc_slice_i32, test_deserialize_arc_slice_string">✅</span> | 🚫 | 🟡 | 🟡 | <span title="arc_tests">✅</span> | 🟡 | 🟡 | 🟡 | 🚫 | 🟡 |

## Attributes

| Attribute | json | yaml | toml | xml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|-----------|------|------|------|-----|------|------|------|-----|------|--------|-----|
| `opaque` | <span title="test_opaque_with_proxy_option">✅</span> | 🚫 | 🚫 | <span title="test_opaque_with_proxy_option_simple">✅</span> | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `proxy` | <span title="test_proxy_without_opaque, test_proxy_for_validation">✅</span> | 🚫 | 🚫 | <span title="test_proxy_without_opaque, test_proxy_for_validation">✅</span> | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
