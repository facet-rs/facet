#![forbid(unsafe_code)]

use arborium::Highlighter;
use core::fmt::{Debug, Display};
use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

use facet::Facet;
use facet_assert::assert_same;
use facet_pretty::{FacetPretty, PrettyPrinter};
use indoc::formatdoc;

/// Trait every format variant implements to participate in the suite.
///
/// Each method returning a [`CaseSpec`] corresponds to a canonical test case.
/// When the suite adds a new case, the trait sprouts another required method,
/// forcing every format crate to acknowledge and implement it.
///
/// The [`FormatSuite::deserialize`] hook is intentionally generic over every `T: Facet` – in
/// the end state it will invoke the shared `FormatDeserializer` to produce a
/// typed value, not just raw events.
pub trait FormatSuite {
    /// Parser/deserializer specific error type.
    type Error: Debug + Display;

    /// Human-readable name for diagnostics.
    fn format_name() -> &'static str;

    /// Optional syntax highlighter language name (Arborium).
    fn highlight_language() -> Option<&'static str> {
        None
    }

    /// Attempt to deserialize `input` into the requested Facet type.
    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        for<'facet> T: Facet<'facet>,
        T: Debug;

    /// Optional serialization hook used for round-trip testing.
    ///
    /// If implemented (returns `Some`), the suite will:
    /// 1) deserialize the canonical input into `T`
    /// 2) serialize that value back into the format
    /// 3) deserialize again into `T`
    /// 4) `assert_same!` that the round-tripped value matches the first one.
    ///
    /// Returning `None` disables round-trip checks for the format.
    fn serialize<T>(value: &T) -> Option<Result<Vec<u8>, String>>
    where
        for<'facet> T: Facet<'facet>,
        T: Debug,
    {
        let _ = value;
        None
    }

    /// Optional async deserialization hook for streaming formats.
    ///
    /// If implemented (returns `Some`), the suite will run async variants
    /// of each test case using this method.
    ///
    /// Returning `None` indicates the format doesn't support async deserialization.
    #[cfg(feature = "tokio")]
    fn deserialize_async<T>(
        input: &[u8],
    ) -> impl std::future::Future<Output = Option<Result<T, Self::Error>>>
    where
        for<'facet> T: Facet<'facet>,
        T: Debug,
    {
        let _ = input;
        async { None }
    }

    /// Case: simple object with a single string field.
    fn struct_single_field() -> CaseSpec;
    /// Case: homogeneous sequence of unsigned integers.
    fn sequence_numbers() -> CaseSpec;
    /// Case: heterogeneous scalar sequence represented as an untagged enum.
    fn sequence_mixed_scalars() -> CaseSpec;
    /// Case: nested struct with child object and tags.
    fn struct_nested() -> CaseSpec;
    /// Case: enum with multiple variant styles.
    fn enum_complex() -> CaseSpec;

    // ── Attribute tests ──

    /// Case: field with `#[facet(rename = "...")]` attribute.
    fn attr_rename_field() -> CaseSpec;
    /// Case: container with `#[facet(rename_all = "camelCase")]` attribute.
    fn attr_rename_all_camel() -> CaseSpec;
    /// Case: field with `#[facet(default)]` attribute.
    fn attr_default_field() -> CaseSpec;
    /// Case: struct-level `#[facet(default)]` allowing all fields to be missing.
    fn attr_default_struct() -> CaseSpec;
    /// Case: field with `#[facet(default = expr)]` using a custom default expression.
    fn attr_default_function() -> CaseSpec;
    /// Case: `Option<T>` field with `None` value (missing in input).
    fn option_none() -> CaseSpec;
    /// Case: `Option<T>` field with `Some` value.
    fn option_some() -> CaseSpec;
    /// Case: `Option<T>` field with explicit `null` value.
    fn option_null() -> CaseSpec;
    /// Case: `#[facet(skip_serializing)]` field.
    fn attr_skip_serializing() -> CaseSpec;
    /// Case: `#[facet(skip_serializing_if = predicate)]` field.
    fn attr_skip_serializing_if() -> CaseSpec;
    /// Case: `#[facet(skip)]` field (skipped for both ser and de).
    fn attr_skip() -> CaseSpec;

    // ── Enum tagging tests ──

    /// Case: internally tagged enum `#[facet(tag = "type")]`.
    fn enum_internally_tagged() -> CaseSpec;
    /// Case: adjacently tagged enum `#[facet(tag = "t", content = "c")]`.
    fn enum_adjacently_tagged() -> CaseSpec;

    // ── Advanced tests ──

    /// Case: flattened struct `#[facet(flatten)]`.
    fn struct_flatten() -> CaseSpec;
    /// Case: transparent newtype `#[facet(transparent)]`.
    fn transparent_newtype() -> CaseSpec;

    // ── Flatten variation tests ──

    /// Case: flattened field is `Option<T>` with `Some` value.
    fn flatten_optional_some() -> CaseSpec;
    /// Case: flattened field is `Option<T>` with `None` value.
    fn flatten_optional_none() -> CaseSpec;
    /// Case: two flattened structs with overlapping field names (error).
    fn flatten_overlapping_fields_error() -> CaseSpec;
    /// Case: three levels of nested flatten (A -> B -> C, all flattened).
    fn flatten_multilevel() -> CaseSpec;
    /// Case: two different enums flattened into same struct.
    fn flatten_multiple_enums() -> CaseSpec;

    // ── Error cases ──

    /// Case: `#[facet(deny_unknown_fields)]` rejects unknown fields.
    fn deny_unknown_fields() -> CaseSpec;
    /// Case: type mismatch - string provided where integer expected.
    fn error_type_mismatch_string_to_int() -> CaseSpec;
    /// Case: structure mismatch - object provided where array expected.
    fn error_type_mismatch_object_to_array() -> CaseSpec;
    /// Case: missing required field error.
    fn error_missing_required_field() -> CaseSpec;

    // ── Alias tests ──

    /// Case: field with `#[facet(alias = "...")]` accepts alternative name.
    fn attr_alias() -> CaseSpec;

    // ── Attribute precedence tests ──

    /// Case: field with both rename and alias - rename takes precedence.
    fn attr_rename_vs_alias_precedence() -> CaseSpec;
    /// Case: struct with `#[facet(rename_all = "kebab-case")]`.
    fn attr_rename_all_kebab() -> CaseSpec;
    /// Case: struct with `#[facet(rename_all = "SCREAMING_SNAKE_CASE")]`.
    fn attr_rename_all_screaming() -> CaseSpec;
    /// Case: field with unicode (emoji) rename `#[facet(rename = "🎉")]`.
    fn attr_rename_unicode() -> CaseSpec;
    /// Case: field with special symbol chars in rename `#[facet(rename = "@type")]`.
    fn attr_rename_special_chars() -> CaseSpec;

    // ── Proxy tests ──

    /// Case: container-level `#[facet(proxy = ...)]` for custom serialization.
    fn proxy_container() -> CaseSpec;
    /// Case: field-level `#[facet(proxy = ...)]` on individual field.
    fn proxy_field_level() -> CaseSpec;
    /// Case: proxy conversion error handling (validation failure).
    fn proxy_validation_error() -> CaseSpec;
    /// Case: proxy wrapping `Option<T>`.
    fn proxy_with_option() -> CaseSpec;
    /// Case: proxy on enum variants.
    fn proxy_with_enum() -> CaseSpec;
    /// Case: interaction between proxy and transparent.
    fn proxy_with_transparent() -> CaseSpec;

    /// Case: `#[facet(opaque, proxy = ...)]` where target type doesn't implement Facet.
    fn opaque_proxy() -> CaseSpec;

    /// Case: `#[facet(opaque, proxy = ...)]` on `Option<OpaqueType>`.
    fn opaque_proxy_option() -> CaseSpec;

    // ── Transparent tests ──

    /// Case: transparent wrapping another transparent type (multilevel).
    fn transparent_multilevel() -> CaseSpec;
    /// Case: transparent wrapping `Option<T>`.
    fn transparent_option() -> CaseSpec;
    /// Case: transparent wrapping NonZero types.
    fn transparent_nonzero() -> CaseSpec;

    // ── Scalar tests ──

    /// Case: boolean scalar value.
    fn scalar_bool() -> CaseSpec;
    /// Case: various integer types.
    fn scalar_integers() -> CaseSpec;
    /// Case: floating point types.
    fn scalar_floats() -> CaseSpec;
    /// Case: floating point with scientific notation.
    fn scalar_floats_scientific() -> CaseSpec;

    // ── Network type tests ──

    /// Case: `std::net::IpAddr` (IPv4 variant).
    #[cfg(feature = "net")]
    fn net_ip_addr_v4() -> CaseSpec;
    /// Case: `std::net::IpAddr` (IPv6 variant).
    #[cfg(feature = "net")]
    fn net_ip_addr_v6() -> CaseSpec;
    /// Case: `std::net::Ipv4Addr`.
    #[cfg(feature = "net")]
    fn net_ipv4_addr() -> CaseSpec;
    /// Case: `std::net::Ipv6Addr`.
    #[cfg(feature = "net")]
    fn net_ipv6_addr() -> CaseSpec;
    /// Case: `std::net::SocketAddr` (IPv4 variant).
    #[cfg(feature = "net")]
    fn net_socket_addr_v4() -> CaseSpec;
    /// Case: `std::net::SocketAddr` (IPv6 variant).
    #[cfg(feature = "net")]
    fn net_socket_addr_v6() -> CaseSpec;
    /// Case: `std::net::SocketAddrV4`.
    #[cfg(feature = "net")]
    fn net_socket_addr_v4_explicit() -> CaseSpec;
    /// Case: `std::net::SocketAddrV6`.
    #[cfg(feature = "net")]
    fn net_socket_addr_v6_explicit() -> CaseSpec;

    // ── Collection tests ──

    /// Case: `HashMap<String, T>`.
    fn map_string_keys() -> CaseSpec;
    /// Case: tuple types.
    fn tuple_simple() -> CaseSpec;
    /// Case: nested tuple types.
    fn tuple_nested() -> CaseSpec;
    /// Case: empty tuple `()` as a field.
    fn tuple_empty() -> CaseSpec;
    /// Case: single-element tuple `(T,)` as a field.
    fn tuple_single_element() -> CaseSpec;
    /// Case: enum with tuple variant `Variant(T, U)`.
    fn tuple_struct_variant() -> CaseSpec;
    /// Case: enum with newtype variant `Variant(T)`.
    fn tuple_newtype_variant() -> CaseSpec;

    // ── Enum variant tests ──

    /// Case: unit enum variant.
    fn enum_unit_variant() -> CaseSpec;
    /// Case: numeric enum.
    fn numeric_enum() -> CaseSpec;
    /// Case: signed numeric enum.
    fn signed_numeric_enum() -> CaseSpec;
    /// Case: numeric enum.
    fn inferred_numeric_enum() -> CaseSpec;
    /// Case: untagged enum.
    fn enum_untagged() -> CaseSpec;
    /// Case: enum with renamed variants `#[facet(rename = "...")]`.
    fn enum_variant_rename() -> CaseSpec;

    // ── Untagged enum variation tests ──

    /// Case: untagged enum with unit variant matching null.
    fn untagged_with_null() -> CaseSpec;
    /// Case: untagged enum with newtype variants (discrimination test).
    fn untagged_newtype_variant() -> CaseSpec;
    /// Case: untagged enum as struct field (nesting test).
    fn untagged_as_field() -> CaseSpec;

    /// Case: untagged enum with only unit variants (dataless enum).
    fn untagged_unit_only() -> CaseSpec;

    // ── Smart pointer tests ──

    /// Case: `Box<T>` smart pointer.
    fn box_wrapper() -> CaseSpec;
    /// Case: `Arc<T>` smart pointer.
    fn arc_wrapper() -> CaseSpec;
    /// Case: `Rc<T>` smart pointer.
    fn rc_wrapper() -> CaseSpec;
    /// Case: `Box<str>` unsized smart pointer.
    fn box_str() -> CaseSpec;
    /// Case: `Arc<str>` unsized smart pointer.
    fn arc_str() -> CaseSpec;
    /// Case: `Rc<str>` unsized smart pointer.
    fn rc_str() -> CaseSpec;
    /// Case: `Arc<[T]>` unsized slice smart pointer.
    fn arc_slice() -> CaseSpec;

    // ── Yoke tests ──

    /// Case: `Yoke<Cow<'static, str>, Arc<str>>` zero-copy smart pointer.
    #[cfg(feature = "yoke")]
    fn yoke_cow_str() -> CaseSpec;

    /// Case: Custom type deriving Yokeable and using #[facet(try_from_ref)].
    #[cfg(feature = "yoke")]
    fn yoke_custom() -> CaseSpec;

    // ── Set tests ──

    /// Case: `BTreeSet<T>`.
    fn set_btree() -> CaseSpec;

    // ── Extended numeric tests ──

    /// Case: i16, u16 integers.
    fn scalar_integers_16() -> CaseSpec;
    /// Case: i128, u128 integers.
    fn scalar_integers_128() -> CaseSpec;
    /// Case: isize, usize integers.
    fn scalar_integers_size() -> CaseSpec;

    // ── NonZero tests ──

    /// Case: NonZero integer types.
    fn nonzero_integers() -> CaseSpec;
    /// Case: Extended NonZero integer types (8, 16, 128, size).
    fn nonzero_integers_extended() -> CaseSpec;

    // ── Borrowed string tests ──

    /// Case: Cow<'static, str> field.
    fn cow_str() -> CaseSpec;

    // ── Newtype tests ──

    /// Case: newtype wrapper around u64.
    fn newtype_u64() -> CaseSpec;
    /// Case: newtype wrapper around String.
    fn newtype_string() -> CaseSpec;

    // ── Char tests ──

    /// Case: char scalar type.
    fn char_scalar() -> CaseSpec;

    // ── HashSet tests ──

    /// Case: `HashSet<T>`.
    fn hashset() -> CaseSpec;

    // ── Nested collection tests ──

    /// Case: nested `Vec<Vec<T>>`.
    fn vec_nested() -> CaseSpec;

    // ── Bytes/binary data tests ──

    /// Case: `Vec<u8>` binary data as array of numbers.
    fn bytes_vec_u8() -> CaseSpec;

    // ── Fixed-size array tests ──

    /// Case: `[T; N]` fixed-size array.
    fn array_fixed_size() -> CaseSpec;

    // ── Unknown field handling tests ──

    /// Case: unknown fields are silently skipped by default.
    fn skip_unknown_fields() -> CaseSpec;

    // ── String escape tests ──

    /// Case: string with escape sequences (\n, \t, \", \\).
    fn string_escapes() -> CaseSpec;
    /// Case: string with extended escape sequences (\b, \f, \r, \u0001).
    fn string_escapes_extended() -> CaseSpec;

    // ── Unit type tests ──

    /// Case: unit struct (zero-sized type).
    fn unit_struct() -> CaseSpec;

    // ── Third-party type tests ──

    /// Case: uuid::Uuid type.
    #[cfg(feature = "uuid")]
    fn uuid() -> CaseSpec;

    /// Case: ulid::Ulid type.
    #[cfg(feature = "ulid")]
    fn ulid() -> CaseSpec;

    /// Case: camino::Utf8PathBuf type.
    #[cfg(feature = "camino")]
    fn camino_path() -> CaseSpec;

    /// Case: ordered_float::OrderedFloat type.
    #[cfg(feature = "ordered-float")]
    fn ordered_float() -> CaseSpec;

    /// Case: rust_decimal::Decimal type.
    #[cfg(feature = "rust_decimal")]
    fn rust_decimal() -> CaseSpec;

    /// Case: time::OffsetDateTime type.
    #[cfg(feature = "time")]
    fn time_offset_datetime() -> CaseSpec;

    /// Case: jiff::Timestamp type.
    #[cfg(feature = "jiff02")]
    fn jiff_timestamp() -> CaseSpec;

    /// Case: jiff::civil::DateTime type.
    #[cfg(feature = "jiff02")]
    fn jiff_civil_datetime() -> CaseSpec;

    /// Case: `chrono::DateTime<Utc>` type.
    #[cfg(feature = "chrono")]
    fn chrono_datetime_utc() -> CaseSpec;

    /// Case: chrono::NaiveDateTime type.
    #[cfg(feature = "chrono")]
    fn chrono_naive_datetime() -> CaseSpec;

    /// Case: chrono::NaiveDate type.
    #[cfg(feature = "chrono")]
    fn chrono_naive_date() -> CaseSpec;

    /// Case: chrono::NaiveTime type.
    #[cfg(feature = "chrono")]
    fn chrono_naive_time() -> CaseSpec;

    /// Case: `Vec<chrono::DateTime<Utc>>` - chrono in collections.
    #[cfg(feature = "chrono")]
    fn chrono_in_vec() -> CaseSpec;

    /// Case: `chrono::Duration` type - serializes as (secs, nanos) tuple.
    #[cfg(feature = "chrono")]
    fn chrono_duration() -> CaseSpec;

    /// Case: `chrono::Duration` with negative value - tests signed (secs, nanos) handling.
    #[cfg(feature = "chrono")]
    fn chrono_duration_negative() -> CaseSpec;

    // ── Standard library time types ──

    /// Case: `core::time::Duration` type - serializes as (secs, nanos) tuple.
    fn std_duration() -> CaseSpec;

    // ── Bytes crate tests ──

    /// Case: `bytes::Bytes` type.
    #[cfg(feature = "bytes")]
    fn bytes_bytes() -> CaseSpec;

    /// Case: `bytes::BytesMut` type.
    #[cfg(feature = "bytes")]
    fn bytes_bytes_mut() -> CaseSpec;

    /// Case: `bytestring::ByteString` type.
    #[cfg(feature = "bytestring")]
    fn bytestring() -> CaseSpec;

    /// Case: `compact_str::CompactString` type.
    #[cfg(feature = "compact_str")]
    fn compact_string() -> CaseSpec;

    /// Case: `smartstring::SmartString` type.
    #[cfg(feature = "smartstring")]
    fn smartstring() -> CaseSpec;

    /// Case: `smol_str::SmolStr` type.
    #[cfg(feature = "smol_str")]
    fn smol_str() -> CaseSpec;

    /// Case: `iddqd::IdHashMap` type.
    #[cfg(feature = "iddqd")]
    fn iddqd_id_hash_map() -> CaseSpec;

    /// Case: `iddqd::IdOrdMap` type.
    #[cfg(feature = "iddqd")]
    fn iddqd_id_ord_map() -> CaseSpec;

    /// Case: `iddqd::BiHashMap` type.
    #[cfg(feature = "iddqd")]
    fn iddqd_bi_hash_map() -> CaseSpec;

    /// Case: `iddqd::TriHashMap` type.
    #[cfg(feature = "iddqd")]
    fn iddqd_tri_hash_map() -> CaseSpec;

    // ── Dynamic value tests ──

    /// Case: `facet_value::Value` dynamic type - null.
    #[cfg(feature = "facet-value")]
    fn value_null() -> CaseSpec;

    /// Case: `facet_value::Value` dynamic type - bool.
    #[cfg(feature = "facet-value")]
    fn value_bool() -> CaseSpec;

    /// Case: `facet_value::Value` dynamic type - integer.
    #[cfg(feature = "facet-value")]
    fn value_integer() -> CaseSpec;

    /// Case: `facet_value::Value` dynamic type - float.
    #[cfg(feature = "facet-value")]
    fn value_float() -> CaseSpec;

    /// Case: `facet_value::Value` dynamic type - string.
    #[cfg(feature = "facet-value")]
    fn value_string() -> CaseSpec;

    /// Case: `facet_value::Value` dynamic type - array.
    #[cfg(feature = "facet-value")]
    fn value_array() -> CaseSpec;

    /// Case: `facet_value::Value` dynamic type - object.
    #[cfg(feature = "facet-value")]
    fn value_object() -> CaseSpec;
}

/// Execute suite cases; kept for convenience, but formats should register each
/// case individually via [`all_cases`].
pub fn run_suite<S: FormatSuite + 'static>() {
    for case in all_cases::<S>() {
        match case.run() {
            CaseOutcome::Passed => {}
            CaseOutcome::Skipped(reason) => {
                eprintln!(
                    "facet-format-suite: skipping {} for {} ({reason})",
                    case.id,
                    S::format_name()
                );
            }
            CaseOutcome::Failed(msg) => {
                panic!(
                    "facet-format-suite case {} ({}) failed: {msg}",
                    case.id, case.description
                );
            }
        }
    }
}

/// Enumerate every canonical case with its typed descriptor.
pub fn all_cases<S: FormatSuite + 'static>() -> Vec<SuiteCase> {
    vec![
        // Core cases
        SuiteCase::new::<S, StructSingleField>(&CASE_STRUCT_SINGLE_FIELD, S::struct_single_field),
        SuiteCase::new::<S, Vec<u64>>(&CASE_SEQUENCE_NUMBERS, S::sequence_numbers),
        SuiteCase::new::<S, Vec<MixedScalar>>(
            &CASE_SEQUENCE_MIXED_SCALARS,
            S::sequence_mixed_scalars,
        ),
        SuiteCase::new::<S, NestedParent>(&CASE_STRUCT_NESTED, S::struct_nested),
        SuiteCase::new::<S, ComplexEnum>(&CASE_ENUM_COMPLEX, S::enum_complex),
        // Attribute cases
        SuiteCase::new::<S, RenamedField>(&CASE_ATTR_RENAME_FIELD, S::attr_rename_field),
        SuiteCase::new::<S, CamelCaseStruct>(&CASE_ATTR_RENAME_ALL_CAMEL, S::attr_rename_all_camel),
        SuiteCase::new::<S, WithDefault>(&CASE_ATTR_DEFAULT_FIELD, S::attr_default_field),
        SuiteCase::new::<S, StructLevelDefault>(&CASE_ATTR_DEFAULT_STRUCT, S::attr_default_struct),
        SuiteCase::new::<S, WithDefaultFunction>(
            &CASE_ATTR_DEFAULT_FUNCTION,
            S::attr_default_function,
        ),
        SuiteCase::new::<S, WithOption>(&CASE_OPTION_NONE, S::option_none),
        SuiteCase::new::<S, WithOption>(&CASE_OPTION_SOME, S::option_some),
        SuiteCase::new::<S, WithOption>(&CASE_OPTION_NULL, S::option_null),
        SuiteCase::new::<S, WithSkipSerializing>(
            &CASE_ATTR_SKIP_SERIALIZING,
            S::attr_skip_serializing,
        ),
        SuiteCase::new::<S, WithSkipSerializingIf>(
            &CASE_ATTR_SKIP_SERIALIZING_IF,
            S::attr_skip_serializing_if,
        ),
        SuiteCase::new::<S, WithSkip>(&CASE_ATTR_SKIP, S::attr_skip),
        // Enum tagging cases
        SuiteCase::new::<S, InternallyTagged>(
            &CASE_ENUM_INTERNALLY_TAGGED,
            S::enum_internally_tagged,
        ),
        SuiteCase::new::<S, AdjacentlyTagged>(
            &CASE_ENUM_ADJACENTLY_TAGGED,
            S::enum_adjacently_tagged,
        ),
        // Advanced cases
        SuiteCase::new::<S, FlattenOuter>(&CASE_STRUCT_FLATTEN, S::struct_flatten),
        SuiteCase::new::<S, UserRecord>(&CASE_TRANSPARENT_NEWTYPE, S::transparent_newtype),
        // Flatten variation cases
        SuiteCase::new::<S, FlattenOptionalSome>(
            &CASE_FLATTEN_OPTIONAL_SOME,
            S::flatten_optional_some,
        ),
        SuiteCase::new::<S, FlattenOptionalNone>(
            &CASE_FLATTEN_OPTIONAL_NONE,
            S::flatten_optional_none,
        ),
        SuiteCase::new::<S, FlattenOverlapping>(
            &CASE_FLATTEN_OVERLAPPING_FIELDS_ERROR,
            S::flatten_overlapping_fields_error,
        ),
        SuiteCase::new::<S, FlattenLevel1>(&CASE_FLATTEN_MULTILEVEL, S::flatten_multilevel),
        SuiteCase::new::<S, FlattenMultipleEnums>(
            &CASE_FLATTEN_MULTIPLE_ENUMS,
            S::flatten_multiple_enums,
        ),
        // Error cases
        SuiteCase::new::<S, DenyUnknownStruct>(&CASE_DENY_UNKNOWN_FIELDS, S::deny_unknown_fields),
        SuiteCase::new::<S, ExpectsInteger>(
            &CASE_ERROR_TYPE_MISMATCH_STRING_TO_INT,
            S::error_type_mismatch_string_to_int,
        ),
        SuiteCase::new::<S, ExpectsArray>(
            &CASE_ERROR_TYPE_MISMATCH_OBJECT_TO_ARRAY,
            S::error_type_mismatch_object_to_array,
        ),
        SuiteCase::new::<S, RequiresAllFields>(
            &CASE_ERROR_MISSING_REQUIRED_FIELD,
            S::error_missing_required_field,
        ),
        // Alias cases
        SuiteCase::new::<S, WithAlias>(&CASE_ATTR_ALIAS, S::attr_alias),
        // Attribute precedence cases
        SuiteCase::new::<S, RenameVsAlias>(
            &CASE_ATTR_RENAME_VS_ALIAS,
            S::attr_rename_vs_alias_precedence,
        ),
        SuiteCase::new::<S, RenameAllKebab>(&CASE_ATTR_RENAME_ALL_KEBAB, S::attr_rename_all_kebab),
        SuiteCase::new::<S, RenameAllScreaming>(
            &CASE_ATTR_RENAME_ALL_SCREAMING,
            S::attr_rename_all_screaming,
        ),
        SuiteCase::new::<S, RenameUnicode>(&CASE_ATTR_RENAME_UNICODE, S::attr_rename_unicode),
        SuiteCase::new::<S, RenameSpecialChars>(
            &CASE_ATTR_RENAME_SPECIAL_CHARS,
            S::attr_rename_special_chars,
        ),
        // Proxy cases
        SuiteCase::new::<S, ProxyInt>(&CASE_PROXY_CONTAINER, S::proxy_container),
        SuiteCase::new::<S, ProxyFieldLevel>(&CASE_PROXY_FIELD_LEVEL, S::proxy_field_level),
        SuiteCase::new::<S, ProxyInt>(&CASE_PROXY_VALIDATION_ERROR, S::proxy_validation_error),
        SuiteCase::new::<S, ProxyWithOption>(&CASE_PROXY_WITH_OPTION, S::proxy_with_option),
        SuiteCase::new::<S, ProxyEnum>(&CASE_PROXY_WITH_ENUM, S::proxy_with_enum),
        SuiteCase::new::<S, TransparentProxy>(
            &CASE_PROXY_WITH_TRANSPARENT,
            S::proxy_with_transparent,
        ),
        SuiteCase::new::<S, OpaqueProxyWrapper>(&CASE_OPAQUE_PROXY, S::opaque_proxy),
        SuiteCase::new::<S, OpaqueProxyOptionWrapper>(
            &CASE_OPAQUE_PROXY_OPTION,
            S::opaque_proxy_option,
        ),
        // Transparent cases
        SuiteCase::new::<S, OuterTransparent>(
            &CASE_TRANSPARENT_MULTILEVEL,
            S::transparent_multilevel,
        ),
        SuiteCase::new::<S, TransparentOption>(&CASE_TRANSPARENT_OPTION, S::transparent_option),
        SuiteCase::new::<S, TransparentNonZero>(&CASE_TRANSPARENT_NONZERO, S::transparent_nonzero),
        // Scalar cases
        SuiteCase::new::<S, BoolWrapper>(&CASE_SCALAR_BOOL, S::scalar_bool),
        SuiteCase::new::<S, IntegerTypes>(&CASE_SCALAR_INTEGERS, S::scalar_integers),
        SuiteCase::new::<S, FloatTypes>(&CASE_SCALAR_FLOATS, S::scalar_floats),
        SuiteCase::new::<S, FloatTypesScientific>(
            &CASE_SCALAR_FLOATS_SCIENTIFIC,
            S::scalar_floats_scientific,
        ),
        // Network type cases
        #[cfg(feature = "net")]
        SuiteCase::new::<S, IpAddrV4Wrapper>(&CASE_NET_IP_ADDR_V4, S::net_ip_addr_v4),
        #[cfg(feature = "net")]
        SuiteCase::new::<S, IpAddrV6Wrapper>(&CASE_NET_IP_ADDR_V6, S::net_ip_addr_v6),
        #[cfg(feature = "net")]
        SuiteCase::new::<S, Ipv4AddrWrapper>(&CASE_NET_IPV4_ADDR, S::net_ipv4_addr),
        #[cfg(feature = "net")]
        SuiteCase::new::<S, Ipv6AddrWrapper>(&CASE_NET_IPV6_ADDR, S::net_ipv6_addr),
        #[cfg(feature = "net")]
        SuiteCase::new::<S, SocketAddrV4Wrapper>(&CASE_NET_SOCKET_ADDR_V4, S::net_socket_addr_v4),
        #[cfg(feature = "net")]
        SuiteCase::new::<S, SocketAddrV6Wrapper>(&CASE_NET_SOCKET_ADDR_V6, S::net_socket_addr_v6),
        #[cfg(feature = "net")]
        SuiteCase::new::<S, SocketAddrV4ExplicitWrapper>(
            &CASE_NET_SOCKET_ADDR_V4_EXPLICIT,
            S::net_socket_addr_v4_explicit,
        ),
        #[cfg(feature = "net")]
        SuiteCase::new::<S, SocketAddrV6ExplicitWrapper>(
            &CASE_NET_SOCKET_ADDR_V6_EXPLICIT,
            S::net_socket_addr_v6_explicit,
        ),
        // Collection cases
        SuiteCase::new::<S, MapWrapper>(&CASE_MAP_STRING_KEYS, S::map_string_keys),
        SuiteCase::new::<S, TupleWrapper>(&CASE_TUPLE_SIMPLE, S::tuple_simple),
        SuiteCase::new::<S, NestedTupleWrapper>(&CASE_TUPLE_NESTED, S::tuple_nested),
        SuiteCase::new::<S, EmptyTupleWrapper>(&CASE_TUPLE_EMPTY, S::tuple_empty),
        SuiteCase::new::<S, SingleElementTupleWrapper>(
            &CASE_TUPLE_SINGLE_ELEMENT,
            S::tuple_single_element,
        ),
        SuiteCase::new::<S, TupleVariantEnum>(&CASE_TUPLE_STRUCT_VARIANT, S::tuple_struct_variant),
        SuiteCase::new::<S, NewtypeVariantEnum>(
            &CASE_TUPLE_NEWTYPE_VARIANT,
            S::tuple_newtype_variant,
        ),
        // Enum variant cases
        SuiteCase::new::<S, UnitVariantEnum>(&CASE_ENUM_UNIT_VARIANT, S::enum_unit_variant),
        SuiteCase::new::<S, NumericEnum>(&CASE_NUMERIC_ENUM, S::numeric_enum),
        SuiteCase::new::<S, UntaggedEnum>(&CASE_ENUM_UNTAGGED, S::enum_untagged),
        SuiteCase::new::<S, EnumVariantRename>(&CASE_ENUM_VARIANT_RENAME, S::enum_variant_rename),
        // Numeric enum variation cases
        SuiteCase::new::<S, SignedNumericEnum>(&CASE_SIGNED_NUMERIC_ENUM, S::signed_numeric_enum),
        SuiteCase::new::<S, NumericEnum>(&CASE_INFERRED_NUMERIC_ENUM, S::inferred_numeric_enum),
        // Untagged enum variation cases
        SuiteCase::new::<S, UntaggedWithNull>(&CASE_UNTAGGED_WITH_NULL, S::untagged_with_null),
        SuiteCase::new::<S, UntaggedNewtype>(
            &CASE_UNTAGGED_NEWTYPE_VARIANT,
            S::untagged_newtype_variant,
        ),
        SuiteCase::new::<S, UntaggedAsField>(&CASE_UNTAGGED_AS_FIELD, S::untagged_as_field),
        SuiteCase::new::<S, UntaggedUnitOnly>(&CASE_UNTAGGED_UNIT_ONLY, S::untagged_unit_only),
        // Smart pointer cases
        SuiteCase::new::<S, BoxWrapper>(&CASE_BOX_WRAPPER, S::box_wrapper),
        SuiteCase::new::<S, ArcWrapper>(&CASE_ARC_WRAPPER, S::arc_wrapper),
        SuiteCase::new::<S, RcWrapper>(&CASE_RC_WRAPPER, S::rc_wrapper),
        SuiteCase::new::<S, BoxStrWrapper>(&CASE_BOX_STR, S::box_str),
        SuiteCase::new::<S, ArcStrWrapper>(&CASE_ARC_STR, S::arc_str),
        SuiteCase::new::<S, RcStrWrapper>(&CASE_RC_STR, S::rc_str),
        SuiteCase::new::<S, ArcSliceWrapper>(&CASE_ARC_SLICE, S::arc_slice),
        // Yoke cases
        #[cfg(feature = "yoke")]
        SuiteCase::new::<S, YokeWrapper>(&CASE_YOKE_COW_STR, S::yoke_cow_str),
        #[cfg(feature = "yoke")]
        SuiteCase::new::<S, YokingWrapper>(&CASE_YOKE_CUSTOM, S::yoke_custom),
        // Set cases
        SuiteCase::new::<S, SetWrapper>(&CASE_SET_BTREE, S::set_btree),
        // Extended numeric cases
        SuiteCase::new::<S, IntegerTypes16>(&CASE_SCALAR_INTEGERS_16, S::scalar_integers_16),
        SuiteCase::new::<S, IntegerTypes128>(&CASE_SCALAR_INTEGERS_128, S::scalar_integers_128),
        SuiteCase::new::<S, IntegerTypesSize>(&CASE_SCALAR_INTEGERS_SIZE, S::scalar_integers_size),
        // NonZero cases
        SuiteCase::new::<S, NonZeroTypes>(&CASE_NONZERO_INTEGERS, S::nonzero_integers),
        SuiteCase::new::<S, NonZeroTypesExtended>(
            &CASE_NONZERO_INTEGERS_EXTENDED,
            S::nonzero_integers_extended,
        ),
        // Borrowed string cases
        SuiteCase::new::<S, CowStrWrapper>(&CASE_COW_STR, S::cow_str),
        // Bytes/binary data cases
        SuiteCase::new::<S, BytesWrapper>(&CASE_BYTES_VEC_U8, S::bytes_vec_u8),
        // Fixed-size array cases
        SuiteCase::new::<S, ArrayWrapper>(&CASE_ARRAY_FIXED_SIZE, S::array_fixed_size),
        // Unknown field handling cases
        SuiteCase::new::<S, SkipUnknownStruct>(&CASE_SKIP_UNKNOWN_FIELDS, S::skip_unknown_fields),
        // String escape cases
        SuiteCase::new::<S, StringEscapes>(&CASE_STRING_ESCAPES, S::string_escapes),
        SuiteCase::new::<S, StringEscapesExtended>(
            &CASE_STRING_ESCAPES_EXTENDED,
            S::string_escapes_extended,
        ),
        // Unit type cases
        SuiteCase::new::<S, UnitStruct>(&CASE_UNIT_STRUCT, S::unit_struct),
        // Newtype cases
        SuiteCase::new::<S, NewtypeU64Wrapper>(&CASE_NEWTYPE_U64, S::newtype_u64),
        SuiteCase::new::<S, NewtypeStringWrapper>(&CASE_NEWTYPE_STRING, S::newtype_string),
        // Char cases
        SuiteCase::new::<S, CharWrapper>(&CASE_CHAR_SCALAR, S::char_scalar),
        // HashSet cases
        SuiteCase::new::<S, HashSetWrapper>(&CASE_HASHSET, S::hashset),
        // Nested collection cases
        SuiteCase::new::<S, NestedVecWrapper>(&CASE_VEC_NESTED, S::vec_nested),
        // Third-party type cases
        #[cfg(feature = "uuid")]
        SuiteCase::new::<S, UuidWrapper>(&CASE_UUID, S::uuid),
        #[cfg(feature = "ulid")]
        SuiteCase::new::<S, UlidWrapper>(&CASE_ULID, S::ulid),
        #[cfg(feature = "camino")]
        SuiteCase::new::<S, CaminoWrapper>(&CASE_CAMINO_PATH, S::camino_path),
        #[cfg(feature = "ordered-float")]
        SuiteCase::new::<S, OrderedFloatWrapper>(&CASE_ORDERED_FLOAT, S::ordered_float),
        #[cfg(feature = "rust_decimal")]
        SuiteCase::new::<S, RustDecimalWrapper>(&CASE_RUST_DECIMAL, S::rust_decimal),
        #[cfg(feature = "time")]
        SuiteCase::new::<S, TimeOffsetDateTimeWrapper>(
            &CASE_TIME_OFFSET_DATETIME,
            S::time_offset_datetime,
        ),
        #[cfg(feature = "jiff02")]
        SuiteCase::new::<S, JiffTimestampWrapper>(&CASE_JIFF_TIMESTAMP, S::jiff_timestamp),
        #[cfg(feature = "jiff02")]
        SuiteCase::new::<S, JiffCivilDateTimeWrapper>(
            &CASE_JIFF_CIVIL_DATETIME,
            S::jiff_civil_datetime,
        ),
        #[cfg(feature = "chrono")]
        SuiteCase::new::<S, ChronoDateTimeUtcWrapper>(
            &CASE_CHRONO_DATETIME_UTC,
            S::chrono_datetime_utc,
        ),
        #[cfg(feature = "chrono")]
        SuiteCase::new::<S, ChronoNaiveDateTimeWrapper>(
            &CASE_CHRONO_NAIVE_DATETIME,
            S::chrono_naive_datetime,
        ),
        #[cfg(feature = "chrono")]
        SuiteCase::new::<S, ChronoNaiveDateWrapper>(&CASE_CHRONO_NAIVE_DATE, S::chrono_naive_date),
        #[cfg(feature = "chrono")]
        SuiteCase::new::<S, ChronoNaiveTimeWrapper>(&CASE_CHRONO_NAIVE_TIME, S::chrono_naive_time),
        #[cfg(feature = "chrono")]
        SuiteCase::new::<S, ChronoInVecWrapper>(&CASE_CHRONO_IN_VEC, S::chrono_in_vec),
        #[cfg(feature = "chrono")]
        SuiteCase::new::<S, ChronoDurationWrapper>(&CASE_CHRONO_DURATION, S::chrono_duration),
        #[cfg(feature = "chrono")]
        SuiteCase::new::<S, ChronoDurationNegativeWrapper>(
            &CASE_CHRONO_DURATION_NEGATIVE,
            S::chrono_duration_negative,
        ),
        // Standard library time cases
        SuiteCase::new::<S, StdDurationWrapper>(&CASE_STD_DURATION, S::std_duration),
        // Bytes crate cases
        #[cfg(feature = "bytes")]
        SuiteCase::new::<S, BytesBytesWrapper>(&CASE_BYTES_BYTES, S::bytes_bytes),
        #[cfg(feature = "bytes")]
        SuiteCase::new::<S, BytesBytesMutWrapper>(&CASE_BYTES_BYTES_MUT, S::bytes_bytes_mut),
        // String optimization crate cases
        #[cfg(feature = "bytestring")]
        SuiteCase::new::<S, ByteStringWrapper>(&CASE_BYTESTRING, S::bytestring),
        #[cfg(feature = "compact_str")]
        SuiteCase::new::<S, CompactStringWrapper>(&CASE_COMPACT_STRING, S::compact_string),
        #[cfg(feature = "smartstring")]
        SuiteCase::new::<S, SmartStringWrapper>(&CASE_SMARTSTRING, S::smartstring),
        #[cfg(feature = "smol_str")]
        SuiteCase::new::<S, SmolStrWrapper>(&CASE_SMOL_STR, S::smol_str),
        #[cfg(feature = "iddqd")]
        SuiteCase::new::<S, IddqdIdHashMapWrapper>(&CASE_IDDQD_ID_HASH_MAP, S::iddqd_id_hash_map),
        #[cfg(feature = "iddqd")]
        SuiteCase::new::<S, IddqdIdOrdMapWrapper>(&CASE_IDDQD_ID_ORD_MAP, S::iddqd_id_ord_map),
        #[cfg(feature = "iddqd")]
        SuiteCase::new::<S, IddqdBiHashMapWrapper>(&CASE_IDDQD_BI_HASH_MAP, S::iddqd_bi_hash_map),
        #[cfg(feature = "iddqd")]
        SuiteCase::new::<S, IddqdTriHashMapWrapper>(&CASE_IDDQD_TRI_HASH_MAP, S::iddqd_tri_hash_map),
        // Dynamic value cases
        #[cfg(feature = "facet-value")]
        SuiteCase::new::<S, facet_value::Value>(&CASE_VALUE_NULL, S::value_null),
        #[cfg(feature = "facet-value")]
        SuiteCase::new::<S, facet_value::Value>(&CASE_VALUE_BOOL, S::value_bool),
        #[cfg(feature = "facet-value")]
        SuiteCase::new::<S, facet_value::Value>(&CASE_VALUE_INTEGER, S::value_integer),
        #[cfg(feature = "facet-value")]
        SuiteCase::new::<S, facet_value::Value>(&CASE_VALUE_FLOAT, S::value_float),
        #[cfg(feature = "facet-value")]
        SuiteCase::new::<S, facet_value::Value>(&CASE_VALUE_STRING, S::value_string),
        #[cfg(feature = "facet-value")]
        SuiteCase::new::<S, facet_value::Value>(&CASE_VALUE_ARRAY, S::value_array),
        #[cfg(feature = "facet-value")]
        SuiteCase::new::<S, facet_value::Value>(&CASE_VALUE_OBJECT, S::value_object),
    ]
}

/// How to compare the deserialized value against the expected value.
#[derive(Debug, Clone, Copy, Default)]
pub enum CompareMode {
    /// Use `assert_same!` (reflection-based comparison) - default for most types.
    #[default]
    Reflection,
    /// Use `assert_eq!` (PartialEq comparison) - required for types containing opaque fields.
    PartialEq,
}

/// Specification returned by each trait method.
#[derive(Debug, Clone)]
pub struct CaseSpec {
    payload: CasePayload,
    note: Option<&'static str>,
    roundtrip: RoundtripSpec,
    compare_mode: CompareMode,
}

impl CaseSpec {
    /// Provide raw bytes for the case input.
    pub const fn from_bytes(input: &'static [u8]) -> Self {
        Self {
            payload: CasePayload::Input(input),
            note: None,
            roundtrip: RoundtripSpec::Enabled,
            compare_mode: CompareMode::Reflection,
        }
    }

    /// Convenience for UTF-8 inputs.
    #[allow(clippy::should_implement_trait)]
    pub const fn from_str(input: &'static str) -> Self {
        Self::from_bytes(input.as_bytes())
    }

    /// Mark the case as skipped for this format, documenting the reason.
    pub const fn skip(reason: &'static str) -> Self {
        Self {
            payload: CasePayload::Skip { reason },
            note: None,
            roundtrip: RoundtripSpec::Enabled,
            compare_mode: CompareMode::Reflection,
        }
    }

    /// Attach an optional note for diagnostics.
    pub const fn with_note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }

    /// Disable round-trip checks for this case, documenting the reason.
    pub const fn without_roundtrip(mut self, reason: &'static str) -> Self {
        self.roundtrip = RoundtripSpec::Disabled { reason };
        self
    }

    /// Use PartialEq comparison instead of reflection-based comparison.
    /// Required for types containing opaque fields that can't be compared via reflection.
    pub const fn with_partial_eq(mut self) -> Self {
        self.compare_mode = CompareMode::PartialEq;
        self
    }

    /// Expect deserialization to fail with an error containing the given substring.
    pub const fn expect_error(input: &'static str, error_contains: &'static str) -> Self {
        Self {
            payload: CasePayload::ExpectError {
                input: input.as_bytes(),
                error_contains,
            },
            note: None,
            roundtrip: RoundtripSpec::Disabled {
                reason: "error case",
            },
            compare_mode: CompareMode::Reflection,
        }
    }

    /// Provide dynamically-generated bytes for the case input.
    ///
    /// Use this for binary formats where input is generated at runtime
    /// (e.g., from a reference implementation like rmp-serde for MsgPack).
    pub const fn from_bytes_vec(input: Vec<u8>) -> Self {
        Self {
            payload: CasePayload::DynamicInput(input),
            note: None,
            roundtrip: RoundtripSpec::Enabled,
            compare_mode: CompareMode::Reflection,
        }
    }
}

#[derive(Debug, Clone)]
enum CasePayload {
    Input(&'static [u8]),
    /// Dynamic input: bytes generated at runtime (e.g., from a reference implementation).
    DynamicInput(Vec<u8>),
    Skip {
        reason: &'static str,
    },
    /// Expect deserialization to fail with an error containing the given substring.
    ExpectError {
        input: &'static [u8],
        error_contains: &'static str,
    },
}

#[derive(Debug, Clone)]
enum RoundtripSpec {
    Enabled,
    Disabled { reason: &'static str },
}

struct CaseDescriptor<T> {
    id: &'static str,
    description: &'static str,
    expected: fn() -> T,
}

#[derive(Debug)]
pub enum CaseOutcome {
    Passed,
    Skipped(&'static str),
    Failed(String),
}

pub struct SuiteCase {
    pub id: &'static str,
    pub description: &'static str,
    skip_reason: Option<&'static str>,
    runner: Box<dyn Fn() -> CaseOutcome + Send + Sync + 'static>,
    #[cfg(feature = "tokio")]
    async_runner: Box<dyn Fn() -> CaseOutcome + Send + Sync + 'static>,
}

impl SuiteCase {
    #[cfg(not(feature = "tokio"))]
    fn new<S, T>(desc: &'static CaseDescriptor<T>, provider: fn() -> CaseSpec) -> Self
    where
        S: FormatSuite,
        for<'facet> T: Facet<'facet>,
        T: Debug + PartialEq + 'static,
    {
        let spec = provider();
        let skip_reason = match spec.payload {
            CasePayload::Skip { reason } => Some(reason),
            _ => None,
        };
        let runner_spec = spec.clone();
        let runner = move || execute_case::<S, T>(desc, runner_spec.clone());

        Self {
            id: desc.id,
            description: desc.description,
            skip_reason,
            runner: Box::new(runner),
        }
    }

    #[cfg(feature = "tokio")]
    fn new<S, T>(desc: &'static CaseDescriptor<T>, provider: fn() -> CaseSpec) -> Self
    where
        S: FormatSuite + 'static,
        for<'facet> T: Facet<'facet>,
        T: Debug + PartialEq + 'static,
    {
        let spec = provider();
        let skip_reason = match spec.payload {
            CasePayload::Skip { reason } => Some(reason),
            _ => None,
        };
        let runner_spec = spec.clone();
        let runner = move || execute_case::<S, T>(desc, runner_spec.clone());

        #[cfg(feature = "tokio")]
        let async_runner = {
            let async_spec = spec.clone();
            Box::new(move || {
                let spec = async_spec.clone();
                // Use current_thread runtime to avoid Send requirements
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create tokio runtime");
                rt.block_on(execute_case_async::<S, T>(desc, spec))
            }) as Box<dyn Fn() -> CaseOutcome + Send + Sync + 'static>
        };

        Self {
            id: desc.id,
            description: desc.description,
            skip_reason,
            runner: Box::new(runner),
            #[cfg(feature = "tokio")]
            async_runner,
        }
    }

    pub fn run(&self) -> CaseOutcome {
        (self.runner)()
    }

    /// Run the async version of this test case.
    /// Uses a current-thread tokio runtime internally.
    #[cfg(feature = "tokio")]
    pub fn run_async(&self) -> CaseOutcome {
        (self.async_runner)()
    }

    pub const fn skip_reason(&self) -> Option<&'static str> {
        self.skip_reason
    }
}

fn execute_case<S, T>(desc: &'static CaseDescriptor<T>, spec: CaseSpec) -> CaseOutcome
where
    S: FormatSuite,
    for<'facet> T: Facet<'facet>,
    T: Debug + PartialEq,
{
    let note = spec.note;
    let compare_mode = spec.compare_mode;
    let roundtrip_disabled_reason = match spec.roundtrip {
        RoundtripSpec::Enabled => None,
        RoundtripSpec::Disabled { reason } => Some(reason),
    };
    let highlight_language = S::highlight_language();
    match spec.payload {
        CasePayload::Skip { reason } => CaseOutcome::Skipped(reason),
        CasePayload::Input(input) => {
            let expected = (desc.expected)();
            tracing::info!("=================== Deserializing");
            let actual = match S::deserialize::<T>(input) {
                Ok(value) => value,
                Err(err) => return CaseOutcome::Failed(err.to_string()),
            };

            tracing::info!("=================== Emitting showcase");
            emit_case_showcase::<S, T>(
                desc,
                note,
                roundtrip_disabled_reason,
                input,
                highlight_language,
                &actual,
            );

            tracing::info!("=================== Comparing deserialized value");
            // Compare deserialized value against expected
            let first_assert = panic::catch_unwind(AssertUnwindSafe(|| match compare_mode {
                CompareMode::Reflection => {
                    assert_same!(
                        actual,
                        expected,
                        "facet-format-suite {} ({}) produced unexpected value",
                        desc.id,
                        desc.description
                    );
                }
                CompareMode::PartialEq => {
                    assert_eq!(
                        actual, expected,
                        "facet-format-suite {} ({}) produced unexpected value",
                        desc.id, desc.description
                    );
                }
            }));
            if let Err(payload) = first_assert {
                return CaseOutcome::Failed(format_panic(payload));
            }

            if roundtrip_disabled_reason.is_some() {
                return CaseOutcome::Passed;
            }

            tracing::info!("=================== Serializing");
            let Some(serialized) = S::serialize(&actual) else {
                return CaseOutcome::Passed;
            };

            let serialized = match serialized {
                Ok(bytes) => bytes,
                Err(msg) => {
                    return CaseOutcome::Failed(format!(
                        "facet-format-suite {} ({}) serialization failed: {msg}",
                        desc.id, desc.description
                    ));
                }
            };

            if let Ok(serialized) = std::str::from_utf8(&serialized) {
                tracing::info!(%serialized, "=================== Serialized result");
            }

            let roundtripped = match S::deserialize::<T>(&serialized) {
                Ok(value) => value,
                Err(err) => {
                    return CaseOutcome::Failed(format!(
                        "facet-format-suite {} ({}) round-trip deserialize failed: {err}",
                        desc.id, desc.description
                    ));
                }
            };

            // Compare round-tripped value against original
            match panic::catch_unwind(AssertUnwindSafe(|| match compare_mode {
                CompareMode::Reflection => {
                    assert_same!(
                        roundtripped,
                        actual,
                        "facet-format-suite {} ({}) round-trip mismatch",
                        desc.id,
                        desc.description
                    );
                }
                CompareMode::PartialEq => {
                    assert_eq!(
                        roundtripped, actual,
                        "facet-format-suite {} ({}) round-trip mismatch",
                        desc.id, desc.description
                    );
                }
            })) {
                Ok(_) => CaseOutcome::Passed,
                Err(payload) => CaseOutcome::Failed(format_panic(payload)),
            }
        }
        CasePayload::DynamicInput(ref input) => {
            let expected = (desc.expected)();
            let actual = match S::deserialize::<T>(input) {
                Ok(value) => value,
                Err(err) => return CaseOutcome::Failed(err.to_string()),
            };

            emit_case_showcase_dynamic::<S, T>(
                desc,
                note,
                roundtrip_disabled_reason,
                input,
                &actual,
            );

            // Compare deserialized value against expected
            let first_assert = panic::catch_unwind(AssertUnwindSafe(|| match compare_mode {
                CompareMode::Reflection => {
                    assert_same!(
                        actual,
                        expected,
                        "facet-format-suite {} ({}) produced unexpected value",
                        desc.id,
                        desc.description
                    );
                }
                CompareMode::PartialEq => {
                    assert_eq!(
                        actual, expected,
                        "facet-format-suite {} ({}) produced unexpected value",
                        desc.id, desc.description
                    );
                }
            }));
            if let Err(payload) = first_assert {
                return CaseOutcome::Failed(format_panic(payload));
            }

            if roundtrip_disabled_reason.is_some() {
                return CaseOutcome::Passed;
            }

            let Some(serialized) = S::serialize(&actual) else {
                return CaseOutcome::Passed;
            };

            let serialized = match serialized {
                Ok(bytes) => bytes,
                Err(msg) => {
                    return CaseOutcome::Failed(format!(
                        "facet-format-suite {} ({}) serialization failed: {msg}",
                        desc.id, desc.description
                    ));
                }
            };

            let roundtripped = match S::deserialize::<T>(&serialized) {
                Ok(value) => value,
                Err(err) => {
                    return CaseOutcome::Failed(format!(
                        "facet-format-suite {} ({}) round-trip deserialize failed: {err}",
                        desc.id, desc.description
                    ));
                }
            };

            // Compare round-tripped value against original
            match panic::catch_unwind(AssertUnwindSafe(|| match compare_mode {
                CompareMode::Reflection => {
                    assert_same!(
                        roundtripped,
                        actual,
                        "facet-format-suite {} ({}) round-trip mismatch",
                        desc.id,
                        desc.description
                    );
                }
                CompareMode::PartialEq => {
                    assert_eq!(
                        roundtripped, actual,
                        "facet-format-suite {} ({}) round-trip mismatch",
                        desc.id, desc.description
                    );
                }
            })) {
                Ok(_) => CaseOutcome::Passed,
                Err(payload) => CaseOutcome::Failed(format_panic(payload)),
            }
        }
        CasePayload::ExpectError {
            input,
            error_contains,
        } => {
            emit_error_case_showcase::<S>(
                desc.id,
                desc.description,
                note,
                input,
                highlight_language,
                error_contains,
            );

            match S::deserialize::<T>(input) {
                Ok(_) => CaseOutcome::Failed(format!(
                    "facet-format-suite {} ({}) expected error containing '{}' but deserialization succeeded",
                    desc.id, desc.description, error_contains
                )),
                Err(err) => {
                    let err_str = err.to_string();
                    if err_str.contains(error_contains) {
                        CaseOutcome::Passed
                    } else {
                        CaseOutcome::Failed(format!(
                            "facet-format-suite {} ({}) expected error containing '{}' but got: {}",
                            desc.id, desc.description, error_contains, err_str
                        ))
                    }
                }
            }
        }
    }
}

fn format_panic(payload: Box<dyn Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        msg.to_string()
    } else if let Some(msg) = payload.downcast_ref::<String>() {
        msg.clone()
    } else {
        "panic with non-string payload".into()
    }
}

#[cfg(feature = "tokio")]
async fn execute_case_async<S, T>(desc: &'static CaseDescriptor<T>, spec: CaseSpec) -> CaseOutcome
where
    S: FormatSuite,
    for<'facet> T: Facet<'facet>,
    T: Debug + PartialEq,
{
    let compare_mode = spec.compare_mode;
    match spec.payload {
        CasePayload::Skip { reason } => CaseOutcome::Skipped(reason),
        CasePayload::Input(input) => {
            // Try async deserialization
            let result = S::deserialize_async::<T>(input).await;
            let actual = match result {
                None => {
                    // Format doesn't support async - skip this test
                    return CaseOutcome::Skipped("async deserialization not supported");
                }
                Some(Ok(value)) => value,
                Some(Err(err)) => return CaseOutcome::Failed(err.to_string()),
            };

            let expected = (desc.expected)();

            // Compare deserialized value against expected
            let first_assert = panic::catch_unwind(AssertUnwindSafe(|| match compare_mode {
                CompareMode::Reflection => {
                    assert_same!(
                        actual,
                        expected,
                        "facet-format-suite {} ({}) async produced unexpected value",
                        desc.id,
                        desc.description
                    );
                }
                CompareMode::PartialEq => {
                    assert_eq!(
                        actual, expected,
                        "facet-format-suite {} ({}) async produced unexpected value",
                        desc.id, desc.description
                    );
                }
            }));
            if let Err(payload) = first_assert {
                return CaseOutcome::Failed(format_panic(payload));
            }

            // Note: We skip round-trip testing for async - it's tested by sync version
            CaseOutcome::Passed
        }
        CasePayload::DynamicInput(ref input) => {
            // Try async deserialization with dynamic input
            let result = S::deserialize_async::<T>(input).await;
            let actual = match result {
                None => {
                    // Format doesn't support async - skip this test
                    return CaseOutcome::Skipped("async deserialization not supported");
                }
                Some(Ok(value)) => value,
                Some(Err(err)) => return CaseOutcome::Failed(err.to_string()),
            };

            let expected = (desc.expected)();

            // Compare deserialized value against expected
            let first_assert = panic::catch_unwind(AssertUnwindSafe(|| match compare_mode {
                CompareMode::Reflection => {
                    assert_same!(
                        actual,
                        expected,
                        "facet-format-suite {} ({}) async produced unexpected value",
                        desc.id,
                        desc.description
                    );
                }
                CompareMode::PartialEq => {
                    assert_eq!(
                        actual, expected,
                        "facet-format-suite {} ({}) async produced unexpected value",
                        desc.id, desc.description
                    );
                }
            }));
            if let Err(payload) = first_assert {
                return CaseOutcome::Failed(format_panic(payload));
            }

            // Note: We skip round-trip testing for async - it's tested by sync version
            CaseOutcome::Passed
        }
        CasePayload::ExpectError {
            input,
            error_contains,
        } => {
            let result = S::deserialize_async::<T>(input).await;
            match result {
                None => CaseOutcome::Skipped("async deserialization not supported"),
                Some(Ok(_)) => CaseOutcome::Failed(format!(
                    "facet-format-suite {} ({}) async expected error containing '{}' but deserialization succeeded",
                    desc.id, desc.description, error_contains
                )),
                Some(Err(err)) => {
                    let err_str = err.to_string();
                    if err_str.contains(error_contains) {
                        CaseOutcome::Passed
                    } else {
                        CaseOutcome::Failed(format!(
                            "facet-format-suite {} ({}) async expected error containing '{}' but got: {}",
                            desc.id, desc.description, error_contains, err_str
                        ))
                    }
                }
            }
        }
    }
}

const CASE_STRUCT_SINGLE_FIELD: CaseDescriptor<StructSingleField> = CaseDescriptor {
    id: "struct::single_field",
    description: "single-field object parsed into StructSingleField",
    expected: || StructSingleField {
        name: "facet".into(),
    },
};

const CASE_SEQUENCE_NUMBERS: CaseDescriptor<Vec<u64>> = CaseDescriptor {
    id: "sequence::numbers",
    description: "array of unsigned integers parsed into Vec<u64>",
    expected: || vec![1, 2, 3],
};

const CASE_SEQUENCE_MIXED_SCALARS: CaseDescriptor<Vec<MixedScalar>> = CaseDescriptor {
    id: "sequence::mixed_scalars",
    description: "array of heterogeneous scalars parsed into Vec<MixedScalar>",
    expected: || {
        vec![
            MixedScalar::Signed(-1),
            MixedScalar::Float(4.625),
            MixedScalar::Null,
            MixedScalar::Bool(true),
        ]
    },
};

const CASE_STRUCT_NESTED: CaseDescriptor<NestedParent> = CaseDescriptor {
    id: "struct::nested",
    description: "struct containing nested child and tag list",
    expected: || NestedParent {
        id: 42,
        child: NestedChild {
            code: "alpha".into(),
            active: true,
        },
        tags: vec!["core".into(), "json".into()],
    },
};

const CASE_ENUM_COMPLEX: CaseDescriptor<ComplexEnum> = CaseDescriptor {
    id: "enum::complex",
    description: "enum with unit, tuple, and struct variants",
    expected: || ComplexEnum::Label {
        name: "facet".into(),
        level: 7,
    },
};

// ── Attribute test case descriptors ──

const CASE_ATTR_RENAME_FIELD: CaseDescriptor<RenamedField> = CaseDescriptor {
    id: "attr::rename_field",
    description: "field with #[facet(rename = \"userName\")]",
    expected: || RenamedField {
        user_name: "alice".into(),
        age: 30,
    },
};

const CASE_ATTR_RENAME_ALL_CAMEL: CaseDescriptor<CamelCaseStruct> = CaseDescriptor {
    id: "attr::rename_all_camel",
    description: "struct with #[facet(rename_all = \"camelCase\")]",
    expected: || CamelCaseStruct {
        first_name: "Jane".into(),
        last_name: "Doe".into(),
        is_active: true,
    },
};

const CASE_ATTR_DEFAULT_FIELD: CaseDescriptor<WithDefault> = CaseDescriptor {
    id: "attr::default_field",
    description: "field with #[facet(default)] missing from input",
    expected: || WithDefault {
        required: "present".into(),
        optional_count: 0, // default value
    },
};

const CASE_ATTR_DEFAULT_STRUCT: CaseDescriptor<StructLevelDefault> = CaseDescriptor {
    id: "attr::default_struct",
    description: "struct-level #[facet(default)] with missing field uses Default",
    expected: || StructLevelDefault {
        count: 123,
        message: String::new(), // Default::default() for String
    },
};

const CASE_ATTR_DEFAULT_FUNCTION: CaseDescriptor<WithDefaultFunction> = CaseDescriptor {
    id: "attr::default_function",
    description: "field with #[facet(default = expr)] uses custom default",
    expected: || WithDefaultFunction {
        magic_number: 42, // from custom_default_value()
        name: "hello".into(),
    },
};

const CASE_OPTION_NONE: CaseDescriptor<WithOption> = CaseDescriptor {
    id: "option::none",
    description: "Option<T> field missing from input becomes None",
    expected: || WithOption {
        name: "test".into(),
        nickname: None,
    },
};

const CASE_OPTION_SOME: CaseDescriptor<WithOption> = CaseDescriptor {
    id: "option::some",
    description: "Option<T> field with Some value",
    expected: || WithOption {
        name: "test".into(),
        nickname: Some("nick".into()),
    },
};

const CASE_OPTION_NULL: CaseDescriptor<WithOption> = CaseDescriptor {
    id: "option::null",
    description: "Option<T> field with explicit null becomes None",
    expected: || WithOption {
        name: "test".into(),
        nickname: None,
    },
};

const CASE_ATTR_SKIP_SERIALIZING: CaseDescriptor<WithSkipSerializing> = CaseDescriptor {
    id: "attr::skip_serializing",
    description: "field with #[facet(skip_serializing)] not in output",
    expected: || WithSkipSerializing {
        visible: "shown".into(),
        hidden: String::new(), // default, not in input
    },
};

const CASE_ATTR_SKIP_SERIALIZING_IF: CaseDescriptor<WithSkipSerializingIf> = CaseDescriptor {
    id: "attr::skip_serializing_if",
    description: "field with #[facet(skip_serializing_if = pred)] skipped when pred is true",
    expected: || WithSkipSerializingIf {
        name: "test".into(),
        optional_data: None, // is_none returns true, so field is skipped on serialize
    },
};

const CASE_ATTR_SKIP: CaseDescriptor<WithSkip> = CaseDescriptor {
    id: "attr::skip",
    description: "field with #[facet(skip)] ignored for both ser and de",
    expected: || WithSkip {
        visible: "data".into(),
        internal: 0, // always uses default (u32::default())
    },
};

// ── Enum tagging case descriptors ──

const CASE_ENUM_INTERNALLY_TAGGED: CaseDescriptor<InternallyTagged> = CaseDescriptor {
    id: "enum::internally_tagged",
    description: "internally tagged enum with #[facet(tag = \"type\")]",
    expected: || InternallyTagged::Circle { radius: 5.0 },
};

const CASE_ENUM_ADJACENTLY_TAGGED: CaseDescriptor<AdjacentlyTagged> = CaseDescriptor {
    id: "enum::adjacently_tagged",
    description: "adjacently tagged enum with #[facet(tag = \"t\", content = \"c\")]",
    expected: || AdjacentlyTagged::Message("hello".into()),
};

// ── Advanced case descriptors ──

const CASE_STRUCT_FLATTEN: CaseDescriptor<FlattenOuter> = CaseDescriptor {
    id: "struct::flatten",
    description: "struct with #[facet(flatten)] flattening inner fields",
    expected: || FlattenOuter {
        name: "point".into(),
        coords: FlattenInner { x: 10, y: 20 },
    },
};

const CASE_TRANSPARENT_NEWTYPE: CaseDescriptor<UserRecord> = CaseDescriptor {
    id: "attr::transparent",
    description: "struct containing #[facet(transparent)] newtype",
    expected: || UserRecord {
        id: UserId(42),
        name: "alice".into(),
    },
};

// ── Flatten variation case descriptors ──

const CASE_FLATTEN_OPTIONAL_SOME: CaseDescriptor<FlattenOptionalSome> = CaseDescriptor {
    id: "flatten::optional_some",
    description: "flattened field is Option<T> with Some value",
    expected: || FlattenOptionalSome {
        name: "test".into(),
        metadata: Some(FlattenMetadata {
            version: 1,
            author: "alice".into(),
        }),
    },
};

const CASE_FLATTEN_OPTIONAL_NONE: CaseDescriptor<FlattenOptionalNone> = CaseDescriptor {
    id: "flatten::optional_none",
    description: "flattened field is Option<T> with None value",
    expected: || FlattenOptionalNone {
        name: "test".into(),
        metadata: None,
    },
};

const CASE_FLATTEN_OVERLAPPING_FIELDS_ERROR: CaseDescriptor<FlattenOverlapping> = CaseDescriptor {
    id: "flatten::overlapping_fields_error",
    description: "two flattened structs with overlapping field names (error)",
    expected: || FlattenOverlapping {
        part_a: FlattenPartA {
            field_a: "a".into(),
            shared: 1,
        },
        part_b: FlattenPartB {
            field_b: "b".into(),
            shared: 2,
        },
    },
};

const CASE_FLATTEN_MULTILEVEL: CaseDescriptor<FlattenLevel1> = CaseDescriptor {
    id: "flatten::multilevel",
    description: "three levels of nested flatten (A -> B -> C, all flattened)",
    expected: || FlattenLevel1 {
        top_field: "top".into(),
        level2: FlattenLevel2 {
            mid_field: 42,
            level3: FlattenLevel3 { deep_field: 100 },
        },
    },
};

const CASE_FLATTEN_MULTIPLE_ENUMS: CaseDescriptor<FlattenMultipleEnums> = CaseDescriptor {
    id: "flatten::multiple_enums",
    description: "two different enums flattened into same struct",
    expected: || FlattenMultipleEnums {
        name: "service".into(),
        auth: FlattenAuthMethod::Password(FlattenAuthPassword {
            password: "secret".into(),
        }),
        transport: FlattenTransport::Tcp(FlattenTransportTcp { port: 8080 }),
    },
};

// ── Error case descriptors ──

const CASE_DENY_UNKNOWN_FIELDS: CaseDescriptor<DenyUnknownStruct> = CaseDescriptor {
    id: "error::deny_unknown_fields",
    description: "#[facet(deny_unknown_fields)] rejects input with extra fields",
    expected: || DenyUnknownStruct {
        foo: "abc".into(),
        bar: 42,
    },
};

const CASE_ERROR_TYPE_MISMATCH_STRING_TO_INT: CaseDescriptor<ExpectsInteger> = CaseDescriptor {
    id: "error::type_mismatch_string_to_int",
    description: "type mismatch - string provided where integer expected",
    expected: || ExpectsInteger { value: 42 },
};

const CASE_ERROR_TYPE_MISMATCH_OBJECT_TO_ARRAY: CaseDescriptor<ExpectsArray> = CaseDescriptor {
    id: "error::type_mismatch_object_to_array",
    description: "structure mismatch - object provided where array expected",
    expected: || ExpectsArray {
        items: vec![1, 2, 3],
    },
};

const CASE_ERROR_MISSING_REQUIRED_FIELD: CaseDescriptor<RequiresAllFields> = CaseDescriptor {
    id: "error::missing_required_field",
    description: "missing required field error",
    expected: || RequiresAllFields {
        name: "Alice".into(),
        age: 30,
        email: "alice@example.com".into(),
    },
};

// ── Alias case descriptors ──

const CASE_ATTR_ALIAS: CaseDescriptor<WithAlias> = CaseDescriptor {
    id: "attr::alias",
    description: "field with #[facet(alias = \"old_name\")] accepts alternative name",
    expected: || WithAlias {
        new_name: "value".into(),
        count: 5,
    },
};

// ── Attribute precedence case descriptors ──

const CASE_ATTR_RENAME_VS_ALIAS: CaseDescriptor<RenameVsAlias> = CaseDescriptor {
    id: "attr::rename_vs_alias_precedence",
    description: "when both rename and alias present, rename takes precedence",
    expected: || RenameVsAlias {
        field: "test".into(),
        id: 1,
    },
};

const CASE_ATTR_RENAME_ALL_KEBAB: CaseDescriptor<RenameAllKebab> = CaseDescriptor {
    id: "attr::rename_all_kebab",
    description: "struct with #[facet(rename_all = \"kebab-case\")]",
    expected: || RenameAllKebab {
        first_name: "John".into(),
        last_name: "Doe".into(),
        user_id: 42,
    },
};

const CASE_ATTR_RENAME_ALL_SCREAMING: CaseDescriptor<RenameAllScreaming> = CaseDescriptor {
    id: "attr::rename_all_screaming",
    description: "struct with #[facet(rename_all = \"SCREAMING_SNAKE_CASE\")]",
    expected: || RenameAllScreaming {
        api_key: "secret-123".into(),
        max_retry_count: 5,
    },
};

const CASE_ATTR_RENAME_UNICODE: CaseDescriptor<RenameUnicode> = CaseDescriptor {
    id: "attr::rename_unicode",
    description: "field with unicode (emoji) rename #[facet(rename = \"🎉\")]",
    expected: || RenameUnicode {
        celebration: "party".into(),
    },
};

const CASE_ATTR_RENAME_SPECIAL_CHARS: CaseDescriptor<RenameSpecialChars> = CaseDescriptor {
    id: "attr::rename_special_chars",
    description: "field with special chars rename #[facet(rename = \"@type\")]",
    expected: || RenameSpecialChars {
        type_field: "node".into(),
    },
};

// ── Proxy case descriptors ──

const CASE_PROXY_CONTAINER: CaseDescriptor<ProxyInt> = CaseDescriptor {
    id: "proxy::container",
    description: "container-level #[facet(proxy = IntAsString)] deserializes int from string",
    expected: || ProxyInt { value: 42 },
};

const CASE_PROXY_FIELD_LEVEL: CaseDescriptor<ProxyFieldLevel> = CaseDescriptor {
    id: "proxy::field_level",
    description: "field-level #[facet(proxy = IntAsString)] on individual field",
    expected: || ProxyFieldLevel {
        name: "test".into(),
        count: 100,
    },
};

const CASE_PROXY_VALIDATION_ERROR: CaseDescriptor<ProxyInt> = CaseDescriptor {
    id: "proxy::validation_error",
    description: "proxy conversion error when validation fails (expects error)",
    expected: || ProxyInt { value: 0 }, // Not used for error cases
};

const CASE_PROXY_WITH_OPTION: CaseDescriptor<ProxyWithOption> = CaseDescriptor {
    id: "proxy::with_option",
    description: "proxy wrapping Option<T>",
    expected: || ProxyWithOption {
        name: "test".into(),
        count: Some(42),
    },
};

const CASE_PROXY_WITH_ENUM: CaseDescriptor<ProxyEnum> = CaseDescriptor {
    id: "proxy::with_enum",
    description: "proxy on enum variant",
    expected: || ProxyEnum::Value(99),
};

const CASE_PROXY_WITH_TRANSPARENT: CaseDescriptor<TransparentProxy> = CaseDescriptor {
    id: "proxy::with_transparent",
    description: "transparent wrapper with proxy",
    expected: || TransparentProxy(42),
};

const CASE_OPAQUE_PROXY: CaseDescriptor<OpaqueProxyWrapper> = CaseDescriptor {
    id: "proxy::opaque",
    description: "#[facet(opaque, proxy = ...)] where target type doesn't implement Facet",
    expected: || OpaqueProxyWrapper {
        value: OpaqueType { inner: 42 },
    },
};

const CASE_OPAQUE_PROXY_OPTION: CaseDescriptor<OpaqueProxyOptionWrapper> = CaseDescriptor {
    id: "proxy::opaque_option",
    description: "#[facet(opaque, proxy = ...)] on Option<OpaqueType>",
    expected: || OpaqueProxyOptionWrapper {
        value: Some(OpaqueType { inner: 99 }),
    },
};

// ── Transparent case descriptors ──

const CASE_TRANSPARENT_MULTILEVEL: CaseDescriptor<OuterTransparent> = CaseDescriptor {
    id: "transparent::multilevel",
    description: "transparent wrapping another transparent type",
    expected: || OuterTransparent(InnerTransparent(42)),
};

const CASE_TRANSPARENT_OPTION: CaseDescriptor<TransparentOption> = CaseDescriptor {
    id: "transparent::option",
    description: "transparent wrapping Option<T>",
    expected: || TransparentOption(Some(99)),
};

const CASE_TRANSPARENT_NONZERO: CaseDescriptor<TransparentNonZero> = CaseDescriptor {
    id: "transparent::nonzero",
    description: "transparent wrapping NonZero type",
    expected: || TransparentNonZero(std::num::NonZeroU32::new(42).unwrap()),
};

// ── Scalar case descriptors ──

const CASE_SCALAR_BOOL: CaseDescriptor<BoolWrapper> = CaseDescriptor {
    id: "scalar::bool",
    description: "boolean scalar values",
    expected: || BoolWrapper {
        yes: true,
        no: false,
    },
};

const CASE_SCALAR_INTEGERS: CaseDescriptor<IntegerTypes> = CaseDescriptor {
    id: "scalar::integers",
    description: "various integer types (i8, u8, i32, u32, i64, u64)",
    expected: || IntegerTypes {
        signed_8: -128,
        unsigned_8: 255,
        signed_32: -2_147_483_648,
        unsigned_32: 4_294_967_295,
        signed_64: -9_223_372_036_854_775_808,
        unsigned_64: 18_446_744_073_709_551_615,
    },
};

const CASE_SCALAR_FLOATS: CaseDescriptor<FloatTypes> = CaseDescriptor {
    id: "scalar::floats",
    description: "floating point types (f32, f64)",
    expected: || FloatTypes {
        float_32: 1.5,
        float_64: 2.25,
    },
};

const CASE_SCALAR_FLOATS_SCIENTIFIC: CaseDescriptor<FloatTypesScientific> = CaseDescriptor {
    id: "scalar::floats_scientific",
    description: "floating point with scientific notation (1.23e10, -4.56e-7)",
    expected: || FloatTypesScientific {
        large: 1.23e10,
        small: -4.56e-7,
        positive_exp: 5e3,
    },
};

// ── Network type case descriptors ──

#[cfg(feature = "net")]
const CASE_NET_IP_ADDR_V4: CaseDescriptor<IpAddrV4Wrapper> = CaseDescriptor {
    id: "net::ip_addr_v4",
    description: "std::net::IpAddr (IPv4 variant)",
    expected: || IpAddrV4Wrapper {
        addr: std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 1)),
    },
};

#[cfg(feature = "net")]
const CASE_NET_IP_ADDR_V6: CaseDescriptor<IpAddrV6Wrapper> = CaseDescriptor {
    id: "net::ip_addr_v6",
    description: "std::net::IpAddr (IPv6 variant)",
    expected: || IpAddrV6Wrapper {
        addr: std::net::IpAddr::V6(std::net::Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
    },
};

#[cfg(feature = "net")]
const CASE_NET_IPV4_ADDR: CaseDescriptor<Ipv4AddrWrapper> = CaseDescriptor {
    id: "net::ipv4_addr",
    description: "std::net::Ipv4Addr",
    expected: || Ipv4AddrWrapper {
        addr: std::net::Ipv4Addr::new(127, 0, 0, 1),
    },
};

#[cfg(feature = "net")]
const CASE_NET_IPV6_ADDR: CaseDescriptor<Ipv6AddrWrapper> = CaseDescriptor {
    id: "net::ipv6_addr",
    description: "std::net::Ipv6Addr",
    expected: || Ipv6AddrWrapper {
        addr: std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1),
    },
};

#[cfg(feature = "net")]
const CASE_NET_SOCKET_ADDR_V4: CaseDescriptor<SocketAddrV4Wrapper> = CaseDescriptor {
    id: "net::socket_addr_v4",
    description: "std::net::SocketAddr (IPv4 variant)",
    expected: || SocketAddrV4Wrapper {
        addr: std::net::SocketAddr::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 1)),
            8080,
        ),
    },
};

#[cfg(feature = "net")]
const CASE_NET_SOCKET_ADDR_V6: CaseDescriptor<SocketAddrV6Wrapper> = CaseDescriptor {
    id: "net::socket_addr_v6",
    description: "std::net::SocketAddr (IPv6 variant)",
    expected: || SocketAddrV6Wrapper {
        addr: std::net::SocketAddr::new(
            std::net::IpAddr::V6(std::net::Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            443,
        ),
    },
};

#[cfg(feature = "net")]
const CASE_NET_SOCKET_ADDR_V4_EXPLICIT: CaseDescriptor<SocketAddrV4ExplicitWrapper> =
    CaseDescriptor {
        id: "net::socket_addr_v4_explicit",
        description: "std::net::SocketAddrV4 (explicit type)",
        expected: || SocketAddrV4ExplicitWrapper {
            addr: std::net::SocketAddrV4::new(std::net::Ipv4Addr::new(10, 0, 0, 1), 3000),
        },
    };

#[cfg(feature = "net")]
const CASE_NET_SOCKET_ADDR_V6_EXPLICIT: CaseDescriptor<SocketAddrV6ExplicitWrapper> =
    CaseDescriptor {
        id: "net::socket_addr_v6_explicit",
        description: "std::net::SocketAddrV6 (explicit type)",
        expected: || SocketAddrV6ExplicitWrapper {
            addr: std::net::SocketAddrV6::new(
                std::net::Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1),
                9000,
                0,
                0,
            ),
        },
    };

// ── Collection case descriptors ──

const CASE_MAP_STRING_KEYS: CaseDescriptor<MapWrapper> = CaseDescriptor {
    id: "collection::map",
    description: "BTreeMap<String, i32> with string keys",
    expected: || {
        let mut map = std::collections::BTreeMap::new();
        map.insert("alpha".into(), 1);
        map.insert("beta".into(), 2);
        MapWrapper { data: map }
    },
};

const CASE_TUPLE_SIMPLE: CaseDescriptor<TupleWrapper> = CaseDescriptor {
    id: "collection::tuple",
    description: "tuple (String, i32, bool)",
    expected: || TupleWrapper {
        triple: ("hello".into(), 42, true),
    },
};

const CASE_TUPLE_NESTED: CaseDescriptor<NestedTupleWrapper> = CaseDescriptor {
    id: "collection::tuple_nested",
    description: "nested tuple ((i32, i32), (String, bool))",
    expected: || NestedTupleWrapper {
        outer: ((1, 2), ("test".into(), true)),
    },
};

const CASE_TUPLE_EMPTY: CaseDescriptor<EmptyTupleWrapper> = CaseDescriptor {
    id: "collection::tuple_empty",
    description: "empty tuple () as a field",
    expected: || EmptyTupleWrapper {
        name: "test".into(),
        empty: (),
    },
};

const CASE_TUPLE_SINGLE_ELEMENT: CaseDescriptor<SingleElementTupleWrapper> = CaseDescriptor {
    id: "collection::tuple_single_element",
    description: "single-element tuple (T,) as a field",
    expected: || SingleElementTupleWrapper {
        name: "test".into(),
        single: (42,),
    },
};

const CASE_TUPLE_STRUCT_VARIANT: CaseDescriptor<TupleVariantEnum> = CaseDescriptor {
    id: "collection::tuple_struct_variant",
    description: "enum with tuple variant Variant(T, U)",
    expected: || TupleVariantEnum::Pair("test".into(), 42),
};

const CASE_TUPLE_NEWTYPE_VARIANT: CaseDescriptor<NewtypeVariantEnum> = CaseDescriptor {
    id: "collection::tuple_newtype_variant",
    description: "enum with newtype variant Variant(T)",
    expected: || NewtypeVariantEnum::Some(99),
};

// ── Enum variant case descriptors ──

const CASE_ENUM_UNIT_VARIANT: CaseDescriptor<UnitVariantEnum> = CaseDescriptor {
    id: "enum::unit_variant",
    description: "enum with unit variants",
    expected: || UnitVariantEnum::Active,
};

const CASE_NUMERIC_ENUM: CaseDescriptor<NumericEnum> = CaseDescriptor {
    id: "enum::numeric",
    description: "#[facet(is_numeric)] enum matches by structure",
    expected: || NumericEnum::B,
};

const CASE_ENUM_UNTAGGED: CaseDescriptor<UntaggedEnum> = CaseDescriptor {
    id: "enum::untagged",
    description: "#[facet(untagged)] enum matches by structure",
    expected: || UntaggedEnum::Point { x: 10, y: 20 },
};

const CASE_ENUM_VARIANT_RENAME: CaseDescriptor<EnumVariantRename> = CaseDescriptor {
    id: "enum::variant_rename",
    description: "#[facet(rename = \"...\")] on enum variants",
    expected: || EnumVariantRename::Active,
};

// ── Numeric enum variation case descriptors ──

const CASE_SIGNED_NUMERIC_ENUM: CaseDescriptor<SignedNumericEnum> = CaseDescriptor {
    id: "enum::numeric::signed",
    description: "Numeric enum with signed discriminant",
    expected: || SignedNumericEnum::Z,
};

const CASE_INFERRED_NUMERIC_ENUM: CaseDescriptor<NumericEnum> = CaseDescriptor {
    id: "enum::numeric::inferred",
    description: "Numeric enum that is inferred from string",
    expected: || NumericEnum::A,
};

// ── Untagged enum variation case descriptors ──

const CASE_UNTAGGED_WITH_NULL: CaseDescriptor<UntaggedWithNull> = CaseDescriptor {
    id: "untagged::with_null",
    description: "untagged enum with unit variant matching null",
    expected: || UntaggedWithNull::None,
};

const CASE_UNTAGGED_NEWTYPE_VARIANT: CaseDescriptor<UntaggedNewtype> = CaseDescriptor {
    id: "untagged::newtype_variant",
    description: "untagged enum with newtype variants (discrimination)",
    expected: || UntaggedNewtype::String("test".into()),
};

const CASE_UNTAGGED_AS_FIELD: CaseDescriptor<UntaggedAsField> = CaseDescriptor {
    id: "untagged::as_field",
    description: "untagged enum as struct field (nesting)",
    expected: || UntaggedAsField {
        name: "test".into(),
        value: UntaggedNewtype::Number(42),
    },
};

const CASE_UNTAGGED_UNIT_ONLY: CaseDescriptor<UntaggedUnitOnly> = CaseDescriptor {
    id: "untagged::unit_only",
    description: "untagged enum with only unit variants (dataless)",
    expected: || UntaggedUnitOnly::Alpha,
};

// ── Smart pointer case descriptors ──

const CASE_BOX_WRAPPER: CaseDescriptor<BoxWrapper> = CaseDescriptor {
    id: "pointer::box",
    description: "Box<T> smart pointer",
    expected: || BoxWrapper {
        inner: Box::new(42),
    },
};

const CASE_ARC_WRAPPER: CaseDescriptor<ArcWrapper> = CaseDescriptor {
    id: "pointer::arc",
    description: "Arc<T> smart pointer",
    expected: || ArcWrapper {
        inner: std::sync::Arc::new(42),
    },
};

const CASE_RC_WRAPPER: CaseDescriptor<RcWrapper> = CaseDescriptor {
    id: "pointer::rc",
    description: "Rc<T> smart pointer",
    expected: || RcWrapper {
        inner: std::rc::Rc::new(42),
    },
};

const CASE_BOX_STR: CaseDescriptor<BoxStrWrapper> = CaseDescriptor {
    id: "pointer::box_str",
    description: "Box<str> unsized smart pointer",
    expected: || BoxStrWrapper {
        inner: Box::from("hello world"),
    },
};

const CASE_ARC_STR: CaseDescriptor<ArcStrWrapper> = CaseDescriptor {
    id: "pointer::arc_str",
    description: "Arc<str> unsized smart pointer",
    expected: || ArcStrWrapper {
        inner: std::sync::Arc::from("hello world"),
    },
};

const CASE_RC_STR: CaseDescriptor<RcStrWrapper> = CaseDescriptor {
    id: "pointer::rc_str",
    description: "Rc<str> unsized smart pointer",
    expected: || RcStrWrapper {
        inner: std::rc::Rc::from("hello world"),
    },
};

const CASE_ARC_SLICE: CaseDescriptor<ArcSliceWrapper> = CaseDescriptor {
    id: "pointer::arc_slice",
    description: "Arc<[T]> unsized slice smart pointer",
    expected: || ArcSliceWrapper {
        inner: std::sync::Arc::from([1i32, 2, 3, 4]),
    },
};

// ── Yoke case descriptors ──

#[cfg(feature = "yoke")]
const CASE_YOKE_COW_STR: CaseDescriptor<YokeWrapper> = CaseDescriptor {
    id: "pointer::yoke_cow_str",
    description: "Yoke<Cow<'static, str>, Arc<str>> zero-copy smart pointer",
    expected: || YokeWrapper {
        value: yoke::Yoke::<std::borrow::Cow<'static, str>, std::sync::Arc<str>>::attach_to_cart(
            std::sync::Arc::from("hello yoke"),
            |s| std::borrow::Cow::Borrowed(s),
        ),
    },
};

#[cfg(feature = "yoke")]
const CASE_YOKE_CUSTOM: CaseDescriptor<YokingWrapper> = CaseDescriptor {
    id: "pointer::yoke_custom",
    description: "Custom type deriving Yokeable and using #[facet(try_from_ref)]",
    expected: || YokingWrapper {
        value: yoke::Yoke::<SplittingYoker<'static>, std::sync::Arc<str>>::attach_to_cart(
            std::sync::Arc::from("hello|yoke"),
            |s| SplittingYoker::try_from_ref(s).unwrap(),
        ),
    },
};

// ── Set case descriptors ──

const CASE_SET_BTREE: CaseDescriptor<SetWrapper> = CaseDescriptor {
    id: "collection::set",
    description: "BTreeSet<String>",
    expected: || {
        let mut set = std::collections::BTreeSet::new();
        set.insert("alpha".into());
        set.insert("beta".into());
        set.insert("gamma".into());
        SetWrapper { items: set }
    },
};

// ── Extended numeric case descriptors ──

const CASE_SCALAR_INTEGERS_16: CaseDescriptor<IntegerTypes16> = CaseDescriptor {
    id: "scalar::integers_16",
    description: "16-bit integer types (i16, u16)",
    expected: || IntegerTypes16 {
        signed_16: -32768,
        unsigned_16: 65535,
    },
};

const CASE_SCALAR_INTEGERS_128: CaseDescriptor<IntegerTypes128> = CaseDescriptor {
    id: "scalar::integers_128",
    description: "128-bit integer types (i128, u128)",
    expected: || IntegerTypes128 {
        signed_128: -170_141_183_460_469_231_731_687_303_715_884_105_728,
        unsigned_128: 340_282_366_920_938_463_463_374_607_431_768_211_455,
    },
};

const CASE_SCALAR_INTEGERS_SIZE: CaseDescriptor<IntegerTypesSize> = CaseDescriptor {
    id: "scalar::integers_size",
    description: "pointer-sized integer types (isize, usize)",
    expected: || IntegerTypesSize {
        signed_size: -1000,
        unsigned_size: 2000,
    },
};

// ── NonZero case descriptors ──

const CASE_NONZERO_INTEGERS: CaseDescriptor<NonZeroTypes> = CaseDescriptor {
    id: "scalar::nonzero",
    description: "NonZero integer types",
    expected: || NonZeroTypes {
        nz_u32: std::num::NonZeroU32::new(42).unwrap(),
        nz_i64: std::num::NonZeroI64::new(-100).unwrap(),
    },
};

const CASE_NONZERO_INTEGERS_EXTENDED: CaseDescriptor<NonZeroTypesExtended> = CaseDescriptor {
    id: "scalar::nonzero_extended",
    description: "Extended NonZero integer types (8, 16, 128, size)",
    expected: || NonZeroTypesExtended {
        nz_u8: std::num::NonZeroU8::new(255).unwrap(),
        nz_i8: std::num::NonZeroI8::new(-128).unwrap(),
        nz_u16: std::num::NonZeroU16::new(65535).unwrap(),
        nz_i16: std::num::NonZeroI16::new(-32768).unwrap(),
        nz_u128: std::num::NonZeroU128::new(1).unwrap(),
        nz_i128: std::num::NonZeroI128::new(-1).unwrap(),
        nz_usize: std::num::NonZeroUsize::new(1000).unwrap(),
        nz_isize: std::num::NonZeroIsize::new(-500).unwrap(),
    },
};

// ── Borrowed string case descriptors ──

const CASE_COW_STR: CaseDescriptor<CowStrWrapper> = CaseDescriptor {
    id: "string::cow_str",
    description: "Cow<'static, str> string fields",
    expected: || CowStrWrapper {
        owned: std::borrow::Cow::Owned("hello world".to_string()),
        message: std::borrow::Cow::Borrowed("borrowed"),
    },
};

// ── Newtype case descriptors ──

const CASE_NEWTYPE_U64: CaseDescriptor<NewtypeU64Wrapper> = CaseDescriptor {
    id: "newtype::u64",
    description: "newtype wrapper around u64",
    expected: || NewtypeU64Wrapper {
        value: NewtypeU64(42),
    },
};

const CASE_NEWTYPE_STRING: CaseDescriptor<NewtypeStringWrapper> = CaseDescriptor {
    id: "newtype::string",
    description: "newtype wrapper around String",
    expected: || NewtypeStringWrapper {
        value: NewtypeString("hello".into()),
    },
};

// ── Char case descriptors ──

const CASE_CHAR_SCALAR: CaseDescriptor<CharWrapper> = CaseDescriptor {
    id: "scalar::char",
    description: "char scalar type",
    expected: || CharWrapper {
        letter: 'A',
        emoji: '🦀',
    },
};

// ── HashSet case descriptors ──

const CASE_HASHSET: CaseDescriptor<HashSetWrapper> = CaseDescriptor {
    id: "collection::hashset",
    description: "HashSet<String>",
    expected: || {
        let mut set = std::collections::HashSet::new();
        set.insert("alpha".into());
        set.insert("beta".into());
        HashSetWrapper { items: set }
    },
};

// ── Nested collection case descriptors ──

const CASE_VEC_NESTED: CaseDescriptor<NestedVecWrapper> = CaseDescriptor {
    id: "collection::vec_nested",
    description: "nested Vec<Vec<i32>>",
    expected: || NestedVecWrapper {
        matrix: vec![vec![1, 2], vec![3, 4, 5]],
    },
};

// ── Bytes/binary data case descriptors ──

const CASE_BYTES_VEC_U8: CaseDescriptor<BytesWrapper> = CaseDescriptor {
    id: "slice::bytes::vec_u8",
    description: "Vec<u8> binary data as array of numbers",
    expected: || BytesWrapper {
        data: vec![0, 128, 255, 42],
    },
};

// ── Fixed-size array case descriptors ──

const CASE_ARRAY_FIXED_SIZE: CaseDescriptor<ArrayWrapper> = CaseDescriptor {
    id: "array::fixed_size",
    description: "[T; N] fixed-size array",
    expected: || ArrayWrapper { values: [1, 2, 3] },
};

// ── Unknown field handling case descriptors ──

const CASE_SKIP_UNKNOWN_FIELDS: CaseDescriptor<SkipUnknownStruct> = CaseDescriptor {
    id: "behavior::skip_unknown_fields",
    description: "unknown fields are silently skipped by default",
    expected: || SkipUnknownStruct {
        known: "value".into(),
    },
};

// ── String escape case descriptors ──

const CASE_STRING_ESCAPES: CaseDescriptor<StringEscapes> = CaseDescriptor {
    id: "string::escapes",
    description: "string with escape sequences (\\n, \\t, \\\", \\\\)",
    expected: || StringEscapes {
        text: "line1\nline2\ttab\"quote\\backslash".into(),
    },
};

const CASE_STRING_ESCAPES_EXTENDED: CaseDescriptor<StringEscapesExtended> = CaseDescriptor {
    id: "string::escapes_extended",
    description: "string with extended escape sequences (\\b, \\f, \\r, \\u0001)",
    expected: || StringEscapesExtended {
        backspace: "hello\x08world".into(),
        formfeed: "page\x0Cbreak".into(),
        carriage_return: "line\rreturn".into(),
        control_char: "\x01".into(),
    },
};

// ── Unit type case descriptors ──

const CASE_UNIT_STRUCT: CaseDescriptor<UnitStruct> = CaseDescriptor {
    id: "unit::struct",
    description: "unit struct (zero-sized type)",
    expected: || UnitStruct,
};

/// Shared fixture type for the struct case.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct StructSingleField {
    pub name: String,
}

/// Shared fixture type for the mixed scalars case.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
pub enum MixedScalar {
    Signed(i64),
    Float(f64),
    Bool(bool),
    Null,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct NestedParent {
    pub id: u64,
    pub child: NestedChild,
    pub tags: Vec<String>,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct NestedChild {
    pub code: String,
    pub active: bool,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum ComplexEnum {
    Empty,
    Count(u64),
    Label { name: String, level: u8 },
}

// ── Attribute test fixtures ──

/// Fixture for `#[facet(rename = "...")]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct RenamedField {
    #[facet(rename = "userName")]
    pub user_name: String,
    pub age: u32,
}

/// Fixture for `#[facet(rename_all = "camelCase")]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(rename_all = "camelCase")]
pub struct CamelCaseStruct {
    pub first_name: String,
    pub last_name: String,
    pub is_active: bool,
}

/// Fixture for `#[facet(default)]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WithDefault {
    pub required: String,
    #[facet(default)]
    pub optional_count: u32,
}

/// Fixture for struct-level `#[facet(default)]` test.
#[derive(Facet, Default, Debug, Clone, PartialEq)]
#[facet(default)]
pub struct StructLevelDefault {
    pub count: i32,
    pub message: String,
}

/// Default value function for `WithDefaultFunction`.
pub const fn custom_default_value() -> i32 {
    42
}

/// Fixture for `#[facet(default = expr)]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WithDefaultFunction {
    #[facet(default = custom_default_value())]
    pub magic_number: i32,
    pub name: String,
}

/// Fixture for `Option<T>` with `None`.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct WithOption {
    pub name: String,
    pub nickname: Option<String>,
}

/// Fixture for `#[facet(skip_serializing)]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WithSkipSerializing {
    pub visible: String,
    #[facet(skip_serializing)]
    #[facet(default)]
    pub hidden: String,
}

/// Fixture for `#[facet(skip_serializing_if = predicate)]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WithSkipSerializingIf {
    pub name: String,
    #[facet(skip_serializing_if = Option::is_none)]
    pub optional_data: Option<String>,
}

/// Fixture for `#[facet(skip)]` test (skipped for both ser and de).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WithSkip {
    pub visible: String,
    #[facet(skip)]
    #[facet(default)]
    pub internal: u32,
}

// ── Enum tagging fixtures ──

/// Internally tagged enum `#[facet(tag = "type")]`.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(tag = "type")]
#[repr(u8)]
pub enum InternallyTagged {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}

/// Adjacently tagged enum `#[facet(tag = "t", content = "c")]`.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(tag = "t", content = "c")]
#[repr(u8)]
pub enum AdjacentlyTagged {
    Message(String),
    Count(u64),
}

// ── Advanced fixtures ──

/// Inner struct for flatten test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenInner {
    pub x: i32,
    pub y: i32,
}

/// Outer struct with `#[facet(flatten)]`.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenOuter {
    pub name: String,
    #[facet(flatten)]
    pub coords: FlattenInner,
}

/// Transparent newtype wrapper.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent)]
pub struct UserId(pub u64);

/// Struct containing a transparent newtype.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct UserRecord {
    pub id: UserId,
    pub name: String,
}

// ── Flatten variation fixtures ──

/// Struct for flatten with optional flattened field (Some case).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenOptionalSome {
    pub name: String,
    #[facet(flatten)]
    pub metadata: Option<FlattenMetadata>,
}

/// Struct for flatten with optional flattened field (None case).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenOptionalNone {
    pub name: String,
    #[facet(flatten)]
    pub metadata: Option<FlattenMetadata>,
}

/// Metadata struct for flatten optional tests.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenMetadata {
    pub version: i32,
    pub author: String,
}

/// First flattened struct with overlapping fields.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenPartA {
    pub field_a: String,
    pub shared: i32,
}

/// Second flattened struct with overlapping fields.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenPartB {
    pub field_b: String,
    pub shared: i32,
}

/// Container with two flattened structs that have overlapping field names.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenOverlapping {
    #[facet(flatten)]
    pub part_a: FlattenPartA,
    #[facet(flatten)]
    pub part_b: FlattenPartB,
}

/// Deepest level for three-level flatten test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenLevel3 {
    pub deep_field: i32,
}

/// Middle level for three-level flatten test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenLevel2 {
    pub mid_field: i32,
    #[facet(flatten)]
    pub level3: FlattenLevel3,
}

/// Top level for three-level flatten test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenLevel1 {
    pub top_field: String,
    #[facet(flatten)]
    pub level2: FlattenLevel2,
}

/// Password auth data for multiple flattened enums test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenAuthPassword {
    pub password: String,
}

/// Token auth data for multiple flattened enums test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenAuthToken {
    pub token: String,
}

/// Auth method enum for multiple flattened enums test.
#[allow(dead_code)]
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum FlattenAuthMethod {
    Password(FlattenAuthPassword),
    Token(FlattenAuthToken),
}

/// TCP transport data for multiple flattened enums test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenTransportTcp {
    pub port: u16,
}

/// Unix transport data for multiple flattened enums test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenTransportUnix {
    pub socket: String,
}

/// Transport enum for multiple flattened enums test.
#[allow(dead_code)]
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum FlattenTransport {
    Tcp(FlattenTransportTcp),
    Unix(FlattenTransportUnix),
}

/// Container with two different flattened enums.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FlattenMultipleEnums {
    pub name: String,
    #[facet(flatten)]
    pub auth: FlattenAuthMethod,
    #[facet(flatten)]
    pub transport: FlattenTransport,
}

// ── Error test fixtures ──

/// Fixture for `#[facet(deny_unknown_fields)]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(deny_unknown_fields)]
pub struct DenyUnknownStruct {
    pub foo: String,
    pub bar: i32,
}

/// Fixture for type mismatch error (string to int).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ExpectsInteger {
    pub value: i32,
}

/// Fixture for structure mismatch error (object to array).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ExpectsArray {
    pub items: Vec<i32>,
}

/// Fixture for missing required field error.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct RequiresAllFields {
    pub name: String,
    pub age: u32,
    pub email: String,
}

/// Fixture for `#[facet(alias = "...")]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WithAlias {
    #[facet(alias = "old_name")]
    pub new_name: String,
    pub count: u32,
}

// ── Attribute precedence test fixtures ──

/// Fixture for testing rename vs alias precedence (rename should win).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct RenameVsAlias {
    #[facet(rename = "officialName")]
    #[facet(alias = "oldName")]
    pub field: String,
    pub id: u32,
}

/// Fixture for `#[facet(rename_all = "kebab-case")]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(rename_all = "kebab-case")]
pub struct RenameAllKebab {
    pub first_name: String,
    pub last_name: String,
    pub user_id: u32,
}

/// Fixture for `#[facet(rename_all = "SCREAMING_SNAKE_CASE")]` test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct RenameAllScreaming {
    pub api_key: String,
    pub max_retry_count: u32,
}

/// Struct with unicode (emoji) field name via rename.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct RenameUnicode {
    #[facet(rename = "🎉")]
    pub celebration: String,
}

/// Struct with special characters in field name via rename.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct RenameSpecialChars {
    #[facet(rename = "@type")]
    pub type_field: String,
}

// ── Proxy test fixtures ──

/// Proxy type that wraps a string for serialization.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct IntAsString(pub String);

/// Target type that uses the proxy for serialization.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(proxy = IntAsString)]
pub struct ProxyInt {
    pub value: i32,
}

/// Convert from proxy (deserialization): string -> ProxyInt
impl TryFrom<IntAsString> for ProxyInt {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: IntAsString) -> Result<Self, Self::Error> {
        Ok(ProxyInt {
            value: proxy.0.parse()?,
        })
    }
}

/// Convert to proxy (serialization): ProxyInt -> string
impl From<&ProxyInt> for IntAsString {
    fn from(v: &ProxyInt) -> Self {
        IntAsString(v.value.to_string())
    }
}

/// Struct with field-level proxy (tests field-level vs container-level).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ProxyFieldLevel {
    pub name: String,
    #[facet(proxy = IntAsString)]
    pub count: i32,
}

/// Convert from proxy for field-level proxy.
impl TryFrom<IntAsString> for i32 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: IntAsString) -> Result<Self, Self::Error> {
        proxy.0.parse()
    }
}

/// Convert to proxy for field-level proxy.
impl From<&i32> for IntAsString {
    fn from(value: &i32) -> Self {
        IntAsString(value.to_string())
    }
}

/// Struct with proxy wrapping `Option<T>`.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ProxyWithOption {
    pub name: String,
    #[facet(proxy = IntAsString)]
    pub count: Option<i32>,
}

/// Convert from proxy for `Option<i32>`.
impl TryFrom<IntAsString> for Option<i32> {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: IntAsString) -> Result<Self, Self::Error> {
        if proxy.0.is_empty() {
            Ok(None)
        } else {
            Ok(Some(proxy.0.parse()?))
        }
    }
}

/// Convert to proxy for `Option<i32>`.
impl From<&Option<i32>> for IntAsString {
    fn from(value: &Option<i32>) -> Self {
        match value {
            Some(v) => IntAsString(v.to_string()),
            None => IntAsString(String::new()),
        }
    }
}

/// Enum with proxy on a newtype variant.
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum ProxyEnum {
    None,
    #[facet(proxy = IntAsString)]
    Value(i32),
}

/// Transparent wrapper with proxy.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent, proxy = IntAsString)]
pub struct TransparentProxy(pub i32);

/// Convert from proxy for TransparentProxy.
impl TryFrom<IntAsString> for TransparentProxy {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: IntAsString) -> Result<Self, Self::Error> {
        Ok(TransparentProxy(proxy.0.parse()?))
    }
}

/// Convert to proxy for TransparentProxy.
impl From<&TransparentProxy> for IntAsString {
    fn from(value: &TransparentProxy) -> Self {
        IntAsString(value.0.to_string())
    }
}

// ── Opaque proxy test fixtures ──

/// An opaque type that does NOT implement Facet.
/// This tests the `#[facet(opaque, proxy = ...)]` pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct OpaqueType {
    pub inner: u64,
}

/// Proxy type for OpaqueType that implements Facet.
#[derive(Facet, Clone, Debug)]
pub struct OpaqueTypeProxy {
    pub inner: u64,
}

/// Convert from proxy (deserialization): OpaqueTypeProxy -> OpaqueType
impl From<OpaqueTypeProxy> for OpaqueType {
    fn from(proxy: OpaqueTypeProxy) -> Self {
        OpaqueType { inner: proxy.inner }
    }
}

/// Convert to proxy (serialization): &OpaqueType -> OpaqueTypeProxy
impl From<&OpaqueType> for OpaqueTypeProxy {
    fn from(v: &OpaqueType) -> Self {
        OpaqueTypeProxy { inner: v.inner }
    }
}

/// Wrapper struct with an opaque field using proxy.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct OpaqueProxyWrapper {
    #[facet(opaque, proxy = OpaqueTypeProxy)]
    pub value: OpaqueType,
}

/// Convert from proxy for `Option<OpaqueType>`
impl From<OpaqueTypeProxy> for Option<OpaqueType> {
    fn from(proxy: OpaqueTypeProxy) -> Self {
        Some(OpaqueType { inner: proxy.inner })
    }
}

/// Convert to proxy for `Option<OpaqueType>`
impl From<&Option<OpaqueType>> for OpaqueTypeProxy {
    fn from(v: &Option<OpaqueType>) -> Self {
        match v {
            Some(ot) => OpaqueTypeProxy { inner: ot.inner },
            None => OpaqueTypeProxy { inner: 0 },
        }
    }
}

/// Wrapper struct with an optional opaque field using proxy.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct OpaqueProxyOptionWrapper {
    #[facet(opaque, proxy = OpaqueTypeProxy)]
    pub value: Option<OpaqueType>,
}

// ── Transparent test fixtures ──

/// Inner transparent wrapper.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent)]
pub struct InnerTransparent(pub i32);

/// Outer transparent wrapper wrapping another transparent type.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent)]
pub struct OuterTransparent(pub InnerTransparent);

/// Transparent wrapper around `Option<T>`.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent)]
pub struct TransparentOption(pub Option<i32>);

/// Transparent wrapper around NonZero type.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent)]
pub struct TransparentNonZero(pub std::num::NonZeroU32);

// ── Scalar test fixtures ──

/// Fixture for boolean scalar test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct BoolWrapper {
    pub yes: bool,
    pub no: bool,
}

/// Fixture for integer scalar test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct IntegerTypes {
    pub signed_8: i8,
    pub unsigned_8: u8,
    pub signed_32: i32,
    pub unsigned_32: u32,
    pub signed_64: i64,
    pub unsigned_64: u64,
}

/// Fixture for float scalar test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct FloatTypes {
    pub float_32: f32,
    pub float_64: f64,
}

/// Fixture for scientific notation float test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct FloatTypesScientific {
    pub large: f64,
    pub small: f64,
    pub positive_exp: f64,
}

// ── Network type test fixtures ──

/// Fixture for IpAddr (IPv4) test.
#[cfg(feature = "net")]
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct IpAddrV4Wrapper {
    pub addr: std::net::IpAddr,
}

/// Fixture for IpAddr (IPv6) test.
#[cfg(feature = "net")]
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct IpAddrV6Wrapper {
    pub addr: std::net::IpAddr,
}

/// Fixture for Ipv4Addr test.
#[cfg(feature = "net")]
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct Ipv4AddrWrapper {
    pub addr: std::net::Ipv4Addr,
}

/// Fixture for Ipv6Addr test.
#[cfg(feature = "net")]
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct Ipv6AddrWrapper {
    pub addr: std::net::Ipv6Addr,
}

/// Fixture for SocketAddr (IPv4) test.
#[cfg(feature = "net")]
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct SocketAddrV4Wrapper {
    pub addr: std::net::SocketAddr,
}

/// Fixture for SocketAddr (IPv6) test.
#[cfg(feature = "net")]
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct SocketAddrV6Wrapper {
    pub addr: std::net::SocketAddr,
}

/// Fixture for SocketAddrV4 (explicit type) test.
#[cfg(feature = "net")]
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct SocketAddrV4ExplicitWrapper {
    pub addr: std::net::SocketAddrV4,
}

/// Fixture for SocketAddrV6 (explicit type) test.
#[cfg(feature = "net")]
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct SocketAddrV6ExplicitWrapper {
    pub addr: std::net::SocketAddrV6,
}

// ── Collection test fixtures ──

/// Fixture for BTreeMap test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct MapWrapper {
    pub data: std::collections::BTreeMap<String, i32>,
}

/// Fixture for tuple test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct TupleWrapper {
    pub triple: (String, i32, bool),
}

/// Fixture for nested tuple test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct NestedTupleWrapper {
    pub outer: ((i32, i32), (String, bool)),
}

/// Fixture for empty tuple test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct EmptyTupleWrapper {
    pub name: String,
    pub empty: (),
}

/// Fixture for single-element tuple test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct SingleElementTupleWrapper {
    pub name: String,
    pub single: (i32,),
}

// ── Enum variant test fixtures ──

/// Unit variant enum.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum UnitVariantEnum {
    Active,
    Inactive,
    Pending,
}

/// Enum with renamed variants.
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum EnumVariantRename {
    #[facet(rename = "enabled")]
    Active,
    #[facet(rename = "disabled")]
    Inactive,
}

/// Numeric enum with a u8 discriminant.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(is_numeric)]
#[repr(u8)]
pub enum NumericEnum {
    A,
    B,
}

/// Numeric enum with a u8 discriminant.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(is_numeric)]
#[repr(i16)]
pub enum SignedNumericEnum {
    Z = -1,
    A,
}

/// Untagged enum that matches by structure.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
pub enum UntaggedEnum {
    Point { x: i32, y: i32 },
    Value(i64),
}

/// Untagged enum with unit variant that matches null.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
pub enum UntaggedWithNull {
    None,
    Some(i32),
}

/// Untagged enum with newtype variants for discrimination testing.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
pub enum UntaggedNewtype {
    String(String),
    Number(u64),
    Bool(bool),
}

/// Struct containing an untagged enum field (nesting test).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct UntaggedAsField {
    pub name: String,
    pub value: UntaggedNewtype,
}

/// Untagged enum with only unit variants (dataless enum).
/// Tests issue #1228: deserializing untagged dataless enums from strings.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
pub enum UntaggedUnitOnly {
    Alpha,
    Beta,
    Gamma,
}

/// Enum with tuple variant (multiple fields).
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum TupleVariantEnum {
    Empty,
    Pair(String, i32),
    Triple(bool, f64, String),
}

/// Enum with newtype variant (single field).
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum NewtypeVariantEnum {
    None,
    Some(i32),
}

// ── Smart pointer test fixtures ──

/// Fixture for `Box<T>` test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct BoxWrapper {
    pub inner: Box<i32>,
}

/// Fixture for `Arc<T>` test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct ArcWrapper {
    pub inner: std::sync::Arc<i32>,
}

/// Fixture for `Rc<T>` test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct RcWrapper {
    pub inner: std::rc::Rc<i32>,
}

/// Fixture for `Box<str>` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct BoxStrWrapper {
    pub inner: Box<str>,
}

/// Fixture for `Arc<str>` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ArcStrWrapper {
    pub inner: std::sync::Arc<str>,
}

/// Fixture for `Rc<str>` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct RcStrWrapper {
    pub inner: std::rc::Rc<str>,
}

/// Fixture for `Arc<[T]>` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ArcSliceWrapper {
    pub inner: std::sync::Arc<[i32]>,
}

// ── Yoke test fixtures ──

/// Fixture for `Yoke<Cow<'static, str>, Arc<str>>` test.
#[cfg(feature = "yoke")]
#[derive(Facet, Debug)]
pub struct YokeWrapper {
    pub value: yoke::Yoke<std::borrow::Cow<'static, str>, std::sync::Arc<str>>,
}

#[cfg(feature = "yoke")]
impl PartialEq for YokeWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.value.get() == other.value.get()
    }
}

/// Fixture for `YokingWrapper` test.
#[cfg(feature = "yoke")]
#[derive(Facet, Debug)]
pub struct YokingWrapper {
    pub value: yoke::Yoke<SplittingYoker<'static>, std::sync::Arc<str>>,
}

#[cfg(feature = "yoke")]
impl PartialEq for YokingWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.value.get() == other.value.get()
    }
}

#[cfg(feature = "yoke")]
#[derive(Facet, Debug, PartialEq, yoke::Yokeable)]
#[facet(try_from_ref = SplittingYoker::try_from_ref)]
pub struct SplittingYoker<'a> {
    left: &'a str,
    right: &'a str,
}

#[cfg(feature = "yoke")]
impl<'a> SplittingYoker<'a> {
    pub fn try_from_ref(value: &'a str) -> Result<Self, &'static str> {
        if let Some((left, right)) = value.split_once('|') {
            Ok(SplittingYoker { left, right })
        } else {
            Err("Invalid format")
        }
    }
}

// ── Set test fixtures ──

/// Fixture for BTreeSet test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct SetWrapper {
    pub items: std::collections::BTreeSet<String>,
}

// ── Extended numeric test fixtures ──

/// Fixture for 16-bit integer test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct IntegerTypes16 {
    pub signed_16: i16,
    pub unsigned_16: u16,
}

/// Fixture for 128-bit integer test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct IntegerTypes128 {
    pub signed_128: i128,
    pub unsigned_128: u128,
}

/// Fixture for pointer-sized integer test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct IntegerTypesSize {
    pub signed_size: isize,
    pub unsigned_size: usize,
}

// ── NonZero test fixtures ──

/// Fixture for NonZero integer test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct NonZeroTypes {
    pub nz_u32: std::num::NonZeroU32,
    pub nz_i64: std::num::NonZeroI64,
}

/// Fixture for extended NonZero integer test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct NonZeroTypesExtended {
    pub nz_u8: std::num::NonZeroU8,
    pub nz_i8: std::num::NonZeroI8,
    pub nz_u16: std::num::NonZeroU16,
    pub nz_i16: std::num::NonZeroI16,
    pub nz_u128: std::num::NonZeroU128,
    pub nz_i128: std::num::NonZeroI128,
    pub nz_usize: std::num::NonZeroUsize,
    pub nz_isize: std::num::NonZeroIsize,
}

// ── Borrowed string test fixtures ──

/// Fixture for Cow<'static, str> test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct CowStrWrapper {
    pub owned: std::borrow::Cow<'static, str>,
    pub message: std::borrow::Cow<'static, str>,
}

// ── Newtype test fixtures ──

/// Newtype wrapper around u64.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent)]
pub struct NewtypeU64(pub u64);

/// Fixture containing newtype u64.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct NewtypeU64Wrapper {
    pub value: NewtypeU64,
}

/// Newtype wrapper around String.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent)]
pub struct NewtypeString(pub String);

/// Fixture containing newtype String.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct NewtypeStringWrapper {
    pub value: NewtypeString,
}

// ── Char test fixtures ──

/// Fixture for char scalar test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct CharWrapper {
    pub letter: char,
    pub emoji: char,
}

// ── HashSet test fixtures ──

/// Fixture for HashSet test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct HashSetWrapper {
    pub items: std::collections::HashSet<String>,
}

// ── Nested collection test fixtures ──

/// Fixture for nested Vec test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct NestedVecWrapper {
    pub matrix: Vec<Vec<i32>>,
}

// ── Bytes/binary data test fixtures ──

/// Fixture for `Vec<u8>` binary data test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct BytesWrapper {
    pub data: Vec<u8>,
}

// ── Fixed-size array test fixtures ──

/// Fixture for fixed-size array test.
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct ArrayWrapper {
    pub values: [u64; 3],
}

// ── Unknown field handling test fixtures ──

/// Fixture for skip_unknown_fields test (no deny_unknown_fields attribute).
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct SkipUnknownStruct {
    pub known: String,
}

// ── String escape test fixtures ──

/// Fixture for string escape test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct StringEscapes {
    pub text: String,
}

/// Fixture for extended string escape test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct StringEscapesExtended {
    pub backspace: String,
    pub formfeed: String,
    pub carriage_return: String,
    pub control_char: String,
}

// ── Unit type test fixtures ──

/// Fixture for unit struct test (zero-sized type).
#[derive(Facet, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "msgpack", derive(serde::Serialize, serde::Deserialize))]
pub struct UnitStruct;

// ── Third-party type case descriptors ──

#[cfg(feature = "uuid")]
const CASE_UUID: CaseDescriptor<UuidWrapper> = CaseDescriptor {
    id: "third_party::uuid",
    description: "uuid::Uuid type",
    expected: || UuidWrapper {
        id: uuid::Uuid::from_bytes([
            0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44,
            0x00, 0x00,
        ]),
    },
};

#[cfg(feature = "ulid")]
const CASE_ULID: CaseDescriptor<UlidWrapper> = CaseDescriptor {
    id: "third_party::ulid",
    description: "ulid::Ulid type",
    expected: || UlidWrapper {
        id: ulid::Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
    },
};

#[cfg(feature = "camino")]
const CASE_CAMINO_PATH: CaseDescriptor<CaminoWrapper> = CaseDescriptor {
    id: "third_party::camino",
    description: "camino::Utf8PathBuf type",
    expected: || CaminoWrapper {
        path: camino::Utf8PathBuf::from("/home/user/documents"),
    },
};

#[cfg(feature = "ordered-float")]
const CASE_ORDERED_FLOAT: CaseDescriptor<OrderedFloatWrapper> = CaseDescriptor {
    id: "third_party::ordered_float",
    description: "ordered_float::OrderedFloat type",
    expected: || OrderedFloatWrapper {
        value: ordered_float::OrderedFloat(1.23456),
    },
};

#[cfg(feature = "rust_decimal")]
const CASE_RUST_DECIMAL: CaseDescriptor<RustDecimalWrapper> = CaseDescriptor {
    id: "third_party::rust_decimal",
    description: "rust_decimal::Decimal type",
    expected: || RustDecimalWrapper {
        amount: rust_decimal::Decimal::new(2499, 2), // 24.99
    },
};

#[cfg(feature = "time")]
const CASE_TIME_OFFSET_DATETIME: CaseDescriptor<TimeOffsetDateTimeWrapper> = CaseDescriptor {
    id: "third_party::time_offset_datetime",
    description: "time::OffsetDateTime type",
    expected: || TimeOffsetDateTimeWrapper {
        created_at: time::macros::datetime!(2023-01-15 12:34:56 UTC),
    },
};

#[cfg(feature = "jiff02")]
const CASE_JIFF_TIMESTAMP: CaseDescriptor<JiffTimestampWrapper> = CaseDescriptor {
    id: "third_party::jiff_timestamp",
    description: "jiff::Timestamp type",
    expected: || JiffTimestampWrapper {
        created_at: "2023-12-31T11:30:00Z".parse().unwrap(),
    },
};

#[cfg(feature = "jiff02")]
const CASE_JIFF_CIVIL_DATETIME: CaseDescriptor<JiffCivilDateTimeWrapper> = CaseDescriptor {
    id: "third_party::jiff_civil_datetime",
    description: "jiff::civil::DateTime type",
    expected: || JiffCivilDateTimeWrapper {
        created_at: "2024-06-19T15:22:45".parse().unwrap(),
    },
};

#[cfg(feature = "chrono")]
const CASE_CHRONO_DATETIME_UTC: CaseDescriptor<ChronoDateTimeUtcWrapper> = CaseDescriptor {
    id: "third_party::chrono_datetime_utc",
    description: "chrono::DateTime<Utc> type",
    expected: || ChronoDateTimeUtcWrapper {
        created_at: chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2023, 1, 15, 12, 34, 56)
            .unwrap(),
    },
};

#[cfg(feature = "chrono")]
const CASE_CHRONO_NAIVE_DATETIME: CaseDescriptor<ChronoNaiveDateTimeWrapper> = CaseDescriptor {
    id: "third_party::chrono_naive_datetime",
    description: "chrono::NaiveDateTime type",
    expected: || ChronoNaiveDateTimeWrapper {
        created_at: chrono::NaiveDate::from_ymd_opt(2023, 1, 15)
            .unwrap()
            .and_hms_opt(12, 34, 56)
            .unwrap(),
    },
};

#[cfg(feature = "chrono")]
const CASE_CHRONO_NAIVE_DATE: CaseDescriptor<ChronoNaiveDateWrapper> = CaseDescriptor {
    id: "third_party::chrono_naive_date",
    description: "chrono::NaiveDate type",
    expected: || ChronoNaiveDateWrapper {
        birth_date: chrono::NaiveDate::from_ymd_opt(2023, 1, 15).unwrap(),
    },
};

#[cfg(feature = "chrono")]
const CASE_CHRONO_NAIVE_TIME: CaseDescriptor<ChronoNaiveTimeWrapper> = CaseDescriptor {
    id: "third_party::chrono_naive_time",
    description: "chrono::NaiveTime type",
    expected: || ChronoNaiveTimeWrapper {
        alarm_time: chrono::NaiveTime::from_hms_opt(12, 34, 56).unwrap(),
    },
};

#[cfg(feature = "chrono")]
const CASE_CHRONO_IN_VEC: CaseDescriptor<ChronoInVecWrapper> = CaseDescriptor {
    id: "third_party::chrono_in_vec",
    description: "Vec<chrono::DateTime<Utc>> - chrono in collections",
    expected: || ChronoInVecWrapper {
        timestamps: vec![
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2023, 1, 1, 0, 0, 0).unwrap(),
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2023, 6, 15, 12, 30, 0).unwrap(),
        ],
    },
};

#[cfg(feature = "chrono")]
const CASE_CHRONO_DURATION: CaseDescriptor<ChronoDurationWrapper> = CaseDescriptor {
    id: "third_party::chrono_duration",
    description: "chrono::Duration type - serializes as (secs, nanos) tuple",
    expected: || ChronoDurationWrapper {
        duration: chrono::Duration::seconds(3600) + chrono::Duration::nanoseconds(500_000_000),
    },
};

#[cfg(feature = "chrono")]
const CASE_CHRONO_DURATION_NEGATIVE: CaseDescriptor<ChronoDurationNegativeWrapper> =
    CaseDescriptor {
        id: "third_party::chrono_duration_negative",
        description: "chrono::Duration negative value - tests signed (secs, nanos) handling",
        expected: || ChronoDurationNegativeWrapper {
            duration: chrono::Duration::seconds(-90) + chrono::Duration::nanoseconds(-250_000_000),
        },
    };

// ── Standard library time case descriptors ──

const CASE_STD_DURATION: CaseDescriptor<StdDurationWrapper> = CaseDescriptor {
    id: "std::std_duration",
    description: "core::time::Duration type - serializes as (secs, nanos) tuple",
    expected: || StdDurationWrapper {
        duration: core::time::Duration::new(3600, 500_000_000),
    },
};

// ── Bytes crate case descriptors ──

#[cfg(feature = "bytes")]
const CASE_BYTES_BYTES: CaseDescriptor<BytesBytesWrapper> = CaseDescriptor {
    id: "third_party::bytes_bytes",
    description: "bytes::Bytes type",
    expected: || BytesBytesWrapper {
        data: bytes::Bytes::from_static(&[1, 2, 3, 4, 255]),
    },
};

#[cfg(feature = "bytes")]
const CASE_BYTES_BYTES_MUT: CaseDescriptor<BytesBytesMutWrapper> = CaseDescriptor {
    id: "third_party::bytes_bytes_mut",
    description: "bytes::BytesMut type",
    expected: || BytesBytesMutWrapper {
        data: bytes::BytesMut::from(&[1, 2, 3, 4, 255][..]),
    },
};

// ── String optimization crate case descriptors ──

#[cfg(feature = "bytestring")]
const CASE_BYTESTRING: CaseDescriptor<ByteStringWrapper> = CaseDescriptor {
    id: "third_party::bytestring",
    description: "bytestring::ByteString type",
    expected: || ByteStringWrapper {
        value: bytestring::ByteString::from("hello world"),
    },
};

#[cfg(feature = "compact_str")]
const CASE_COMPACT_STRING: CaseDescriptor<CompactStringWrapper> = CaseDescriptor {
    id: "third_party::compact_string",
    description: "compact_str::CompactString type",
    expected: || CompactStringWrapper {
        value: compact_str::CompactString::from("hello world"),
    },
};

#[cfg(feature = "smartstring")]
const CASE_SMARTSTRING: CaseDescriptor<SmartStringWrapper> = CaseDescriptor {
    id: "third_party::smartstring",
    description: "smartstring::SmartString type",
    expected: || SmartStringWrapper {
        value: smartstring::SmartString::from("hello world"),
    },
};

#[cfg(feature = "smol_str")]
const CASE_SMOL_STR: CaseDescriptor<SmolStrWrapper> = CaseDescriptor {
    id: "third_party::smol_str",
    description: "smol_str::SmolStr type",
    expected: || SmolStrWrapper {
        value: smol_str::SmolStr::from("hello world"),
    },
};

#[cfg(feature = "iddqd")]
const CASE_IDDQD_ID_HASH_MAP: CaseDescriptor<IddqdIdHashMapWrapper> = CaseDescriptor {
    id: "third_party::iddqd_id_hash_map",
    description: "iddqd::IdHashMap type (set-like collection with key extraction)",
    expected: || {
        let mut map = iddqd::IdHashMap::with_hasher(std::hash::RandomState::new());
        map.insert_overwrite(IddqdTestItem {
            id: 1,
            name: String::from("Alice"),
        });
        IddqdIdHashMapWrapper { items: map }
    },
};

#[cfg(feature = "iddqd")]
const CASE_IDDQD_ID_ORD_MAP: CaseDescriptor<IddqdIdOrdMapWrapper> = CaseDescriptor {
    id: "third_party::iddqd_id_ord_map",
    description: "iddqd::IdOrdMap type (ordered set-like collection with key extraction)",
    expected: || {
        let mut map = iddqd::IdOrdMap::new();
        map.insert_overwrite(IddqdOrdTestItem {
            id: 1,
            name: String::from("Alice"),
        });
        IddqdIdOrdMapWrapper { items: map }
    },
};

#[cfg(feature = "iddqd")]
const CASE_IDDQD_BI_HASH_MAP: CaseDescriptor<IddqdBiHashMapWrapper> = CaseDescriptor {
    id: "third_party::iddqd_bi_hash_map",
    description: "iddqd::BiHashMap type (bijective map with two keys per value)",
    expected: || {
        let mut map = iddqd::BiHashMap::with_hasher(std::hash::RandomState::new());
        map.insert_overwrite(IddqdBiTestItem {
            id: 1,
            code: String::from("A001"),
            name: String::from("Alice"),
        });
        IddqdBiHashMapWrapper { items: map }
    },
};

#[cfg(feature = "iddqd")]
const CASE_IDDQD_TRI_HASH_MAP: CaseDescriptor<IddqdTriHashMapWrapper> = CaseDescriptor {
    id: "third_party::iddqd_tri_hash_map",
    description: "iddqd::TriHashMap type (trijective map with three keys per value)",
    expected: || {
        let mut map = iddqd::TriHashMap::with_hasher(std::hash::RandomState::new());
        map.insert_overwrite(IddqdTriTestItem {
            id: 1,
            code: String::from("A001"),
            email: String::from("alice@example.com"),
            name: String::from("Alice"),
        });
        IddqdTriHashMapWrapper { items: map }
    },
};

// ── Dynamic value case descriptors ──

#[cfg(feature = "facet-value")]
const CASE_VALUE_NULL: CaseDescriptor<facet_value::Value> = CaseDescriptor {
    id: "value::null",
    description: "facet_value::Value - null",
    expected: || facet_value::Value::NULL,
};

#[cfg(feature = "facet-value")]
const CASE_VALUE_BOOL: CaseDescriptor<facet_value::Value> = CaseDescriptor {
    id: "value::bool",
    description: "facet_value::Value - bool",
    expected: || facet_value::Value::TRUE,
};

#[cfg(feature = "facet-value")]
const CASE_VALUE_INTEGER: CaseDescriptor<facet_value::Value> = CaseDescriptor {
    id: "value::integer",
    description: "facet_value::Value - integer",
    expected: || facet_value::Value::from(42i64),
};

#[cfg(feature = "facet-value")]
const CASE_VALUE_FLOAT: CaseDescriptor<facet_value::Value> = CaseDescriptor {
    id: "value::float",
    description: "facet_value::Value - float",
    expected: || facet_value::Value::from(2.5f64),
};

#[cfg(feature = "facet-value")]
const CASE_VALUE_STRING: CaseDescriptor<facet_value::Value> = CaseDescriptor {
    id: "value::string",
    description: "facet_value::Value - string",
    expected: || facet_value::Value::from("hello world"),
};

#[cfg(feature = "facet-value")]
const CASE_VALUE_ARRAY: CaseDescriptor<facet_value::Value> = CaseDescriptor {
    id: "value::array",
    description: "facet_value::Value - array",
    expected: || {
        facet_value::VArray::from_iter([
            facet_value::Value::from(1i64),
            facet_value::Value::from(2i64),
            facet_value::Value::from(3i64),
        ])
        .into()
    },
};

#[cfg(feature = "facet-value")]
const CASE_VALUE_OBJECT: CaseDescriptor<facet_value::Value> = CaseDescriptor {
    id: "value::object",
    description: "facet_value::Value - object",
    expected: || {
        let mut map = facet_value::VObject::new();
        map.insert("name", facet_value::Value::from("test"));
        map.insert("count", facet_value::Value::from(42i64));
        map.into()
    },
};

// ── Third-party type test fixtures ──

/// Fixture for uuid::Uuid test.
#[cfg(feature = "uuid")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct UuidWrapper {
    pub id: uuid::Uuid,
}

/// Fixture for ulid::Ulid test.
#[cfg(feature = "ulid")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct UlidWrapper {
    pub id: ulid::Ulid,
}

/// Fixture for camino::Utf8PathBuf test.
#[cfg(feature = "camino")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct CaminoWrapper {
    pub path: camino::Utf8PathBuf,
}

/// Fixture for ordered_float::OrderedFloat test.
#[cfg(feature = "ordered-float")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct OrderedFloatWrapper {
    pub value: ordered_float::OrderedFloat<f64>,
}

/// Fixture for rust_decimal::Decimal test.
#[cfg(feature = "rust_decimal")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct RustDecimalWrapper {
    pub amount: rust_decimal::Decimal,
}

/// Fixture for time::OffsetDateTime test.
#[cfg(feature = "time")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct TimeOffsetDateTimeWrapper {
    pub created_at: time::OffsetDateTime,
}

/// Fixture for jiff::Timestamp test.
#[cfg(feature = "jiff02")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct JiffTimestampWrapper {
    pub created_at: jiff::Timestamp,
}

/// Fixture for jiff::civil::DateTime test.
#[cfg(feature = "jiff02")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct JiffCivilDateTimeWrapper {
    pub created_at: jiff::civil::DateTime,
}

/// Fixture for `chrono::DateTime<Utc>` test.
#[cfg(feature = "chrono")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ChronoDateTimeUtcWrapper {
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Fixture for chrono::NaiveDateTime test.
#[cfg(feature = "chrono")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ChronoNaiveDateTimeWrapper {
    pub created_at: chrono::NaiveDateTime,
}

/// Fixture for chrono::NaiveDate test.
#[cfg(feature = "chrono")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ChronoNaiveDateWrapper {
    pub birth_date: chrono::NaiveDate,
}

/// Fixture for chrono::NaiveTime test.
#[cfg(feature = "chrono")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ChronoNaiveTimeWrapper {
    pub alarm_time: chrono::NaiveTime,
}

/// Fixture for chrono in collections test.
#[cfg(feature = "chrono")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ChronoInVecWrapper {
    pub timestamps: Vec<chrono::DateTime<chrono::Utc>>,
}

/// Fixture for `chrono::Duration` test.
#[cfg(feature = "chrono")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ChronoDurationWrapper {
    pub duration: chrono::Duration,
}

/// Fixture for `chrono::Duration` negative test.
#[cfg(feature = "chrono")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ChronoDurationNegativeWrapper {
    pub duration: chrono::Duration,
}

// ── Standard library time test fixtures ──

/// Fixture for `core::time::Duration` test.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct StdDurationWrapper {
    pub duration: core::time::Duration,
}

// ── Bytes crate test fixtures ──

/// Fixture for `bytes::Bytes` test.
#[cfg(feature = "bytes")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct BytesBytesWrapper {
    pub data: bytes::Bytes,
}

/// Fixture for `bytes::BytesMut` test.
#[cfg(feature = "bytes")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct BytesBytesMutWrapper {
    pub data: bytes::BytesMut,
}

// ── String optimization crate test fixtures ──

/// Fixture for `bytestring::ByteString` test.
#[cfg(feature = "bytestring")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ByteStringWrapper {
    pub value: bytestring::ByteString,
}

/// Fixture for `compact_str::CompactString` test.
#[cfg(feature = "compact_str")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct CompactStringWrapper {
    pub value: compact_str::CompactString,
}

/// Fixture for `smartstring::SmartString` test.
#[cfg(feature = "smartstring")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct SmartStringWrapper {
    pub value: smartstring::SmartString<smartstring::LazyCompact>,
}

/// Fixture for `smol_str::SmolStr` test.
#[cfg(feature = "smol_str")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct SmolStrWrapper {
    pub value: smol_str::SmolStr,
}

// ── iddqd crate test fixtures ──

/// Test item type for iddqd collections.
/// Implements `IdHashItem` with `id` as the key.
#[cfg(feature = "iddqd")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct IddqdTestItem {
    pub id: u64,
    pub name: String,
}

#[cfg(feature = "iddqd")]
impl iddqd::IdHashItem for IddqdTestItem {
    type Key<'a> = u64;
    fn key(&self) -> Self::Key<'_> {
        self.id
    }
    iddqd::id_upcast!();
}

/// Fixture for `iddqd::IdHashMap` test.
/// Uses `std::hash::RandomState` as the hasher since it implements `Facet`.
#[cfg(feature = "iddqd")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct IddqdIdHashMapWrapper {
    pub items: iddqd::IdHashMap<IddqdTestItem, std::hash::RandomState>,
}

/// Test item type for iddqd IdOrdMap.
/// Implements `IdOrdItem` with `id` as the key.
#[cfg(feature = "iddqd")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct IddqdOrdTestItem {
    pub id: u64,
    pub name: String,
}

#[cfg(feature = "iddqd")]
impl iddqd::IdOrdItem for IddqdOrdTestItem {
    type Key<'a> = u64;
    fn key(&self) -> Self::Key<'_> {
        self.id
    }
    iddqd::id_upcast!();
}

/// Fixture for `iddqd::IdOrdMap` test.
#[cfg(feature = "iddqd")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct IddqdIdOrdMapWrapper {
    pub items: iddqd::IdOrdMap<IddqdOrdTestItem>,
}

/// Test item type for iddqd BiHashMap.
/// Implements `BiHashItem` with `id` as key1 and `code` as key2.
#[cfg(feature = "iddqd")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct IddqdBiTestItem {
    pub id: u64,
    pub code: String,
    pub name: String,
}

#[cfg(feature = "iddqd")]
impl iddqd::BiHashItem for IddqdBiTestItem {
    type K1<'a> = u64;
    type K2<'a> = &'a str;
    fn key1(&self) -> Self::K1<'_> {
        self.id
    }
    fn key2(&self) -> Self::K2<'_> {
        &self.code
    }
    iddqd::bi_upcast!();
}

/// Fixture for `iddqd::BiHashMap` test.
/// Uses `std::hash::RandomState` as the hasher since it implements `Facet`.
#[cfg(feature = "iddqd")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct IddqdBiHashMapWrapper {
    pub items: iddqd::BiHashMap<IddqdBiTestItem, std::hash::RandomState>,
}

/// Test item type for iddqd TriHashMap.
/// Implements `TriHashItem` with `id` as key1, `code` as key2, and `email` as key3.
#[cfg(feature = "iddqd")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct IddqdTriTestItem {
    pub id: u64,
    pub code: String,
    pub email: String,
    pub name: String,
}

#[cfg(feature = "iddqd")]
impl iddqd::TriHashItem for IddqdTriTestItem {
    type K1<'a> = u64;
    type K2<'a> = &'a str;
    type K3<'a> = &'a str;
    fn key1(&self) -> Self::K1<'_> {
        self.id
    }
    fn key2(&self) -> Self::K2<'_> {
        &self.code
    }
    fn key3(&self) -> Self::K3<'_> {
        &self.email
    }
    iddqd::tri_upcast!();
}

/// Fixture for `iddqd::TriHashMap` test.
/// Uses `std::hash::RandomState` as the hasher since it implements `Facet`.
#[cfg(feature = "iddqd")]
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct IddqdTriHashMapWrapper {
    pub items: iddqd::TriHashMap<IddqdTriTestItem, std::hash::RandomState>,
}

fn emit_case_showcase<S, T>(
    desc: &'static CaseDescriptor<T>,
    note: Option<&'static str>,
    roundtrip_disabled_reason: Option<&'static str>,
    input: &'static [u8],
    highlight_language: Option<&'static str>,
    actual: &T,
) where
    S: FormatSuite,
    for<'facet> T: Facet<'facet>,
    T: Debug,
{
    let (input_label, input_block) = match highlight_language {
        Some(language) => match highlight_payload(language, input) {
            Some(html) => (format!("Input highlighted via arborium ({language})"), html),
            None => (
                format!("Input (UTF-8, highlighting unavailable for {language})"),
                String::from_utf8_lossy(input).into_owned(),
            ),
        },
        None => (
            "Input (UTF-8)".to_string(),
            String::from_utf8_lossy(input).into_owned(),
        ),
    };

    let pretty_output = format!(
        "{}",
        actual.pretty_with(PrettyPrinter::new().with_indent_size(2))
    );
    let note_line = note.map(|n| format!("note: {n}\n")).unwrap_or_default();
    let roundtrip_line = roundtrip_disabled_reason
        .map(|r| format!("roundtrip: disabled ({r})\n"))
        .unwrap_or_default();

    println!(
        "{}",
        formatdoc!(
            "
            ── facet-format-suite :: {format_name} :: {case_id} ──
            description: {description}
            {note_line}{roundtrip_line}{input_label}:
            {input_block}

            facet-pretty output:
            {pretty_output}
            ",
            format_name = S::format_name(),
            case_id = desc.id,
            description = desc.description,
            note_line = note_line,
            roundtrip_line = roundtrip_line,
            input_label = input_label,
            input_block = input_block,
            pretty_output = pretty_output,
        )
    );
}

fn emit_error_case_showcase<S: FormatSuite>(
    case_id: &str,
    description: &str,
    note: Option<&'static str>,
    input: &[u8],
    highlight_language: Option<&'static str>,
    error_contains: &str,
) {
    let (input_label, input_block) = match highlight_language {
        Some(language) => match highlight_payload(language, input) {
            Some(html) => (format!("Input highlighted via arborium ({language})"), html),
            None => (
                format!("Input (UTF-8, highlighting unavailable for {language})"),
                String::from_utf8_lossy(input).into_owned(),
            ),
        },
        None => (
            "Input (UTF-8)".to_string(),
            String::from_utf8_lossy(input).into_owned(),
        ),
    };

    let note_line = note.map(|n| format!("note: {n}\n")).unwrap_or_default();

    println!(
        "{}",
        formatdoc!(
            "
            ── facet-format-suite :: {format_name} :: {case_id} ──
            description: {description}
            {note_line}expects error containing: \"{error_contains}\"
            {input_label}:
            {input_block}
            ",
            format_name = S::format_name(),
            case_id = case_id,
            description = description,
            note_line = note_line,
            error_contains = error_contains,
            input_label = input_label,
            input_block = input_block,
        )
    );
}

fn highlight_payload(language: &str, input: &[u8]) -> Option<String> {
    let source = core::str::from_utf8(input).ok()?;
    let mut highlighter = Highlighter::new();
    highlighter.highlight(language, source).ok()
}

// ── MsgPack interoperability helpers ──

/// Generate MsgPack bytes using `rmp-serde` for interoperability testing.
///
/// This module provides functions to serialize test fixtures using the
/// reference MsgPack implementation, allowing our parser to be tested
/// against known-good input.
#[cfg(feature = "msgpack")]
pub mod msgpack {
    use super::*;

    /// Serialize a value to MsgPack bytes using rmp-serde.
    ///
    /// Uses `to_vec_named` to serialize structs as maps with field names,
    /// which is the format our parser expects.
    pub fn serialize<T: serde::Serialize>(value: &T) -> Vec<u8> {
        rmp_serde::to_vec_named(value).expect("rmp-serde serialization failed")
    }

    /// Create a CaseSpec with MsgPack input from the expected value.
    ///
    /// This serializes the expected value using rmp-serde (reference impl)
    /// and returns a CaseSpec that will test our deserializer against it.
    pub fn case_from_expected<T>(expected: &T) -> CaseSpec
    where
        T: serde::Serialize,
    {
        CaseSpec::from_bytes_vec(serialize(expected))
    }

    // ── Pre-built test inputs ──

    /// MsgPack bytes for StructSingleField { name: "facet" }
    pub fn struct_single_field_bytes() -> Vec<u8> {
        serialize(&StructSingleField {
            name: "facet".into(),
        })
    }

    /// MsgPack bytes for sequence [1, 2, 3]
    pub fn sequence_numbers_bytes() -> Vec<u8> {
        serialize(&vec![1u64, 2, 3])
    }

    /// MsgPack bytes for NestedParent
    pub fn struct_nested_bytes() -> Vec<u8> {
        serialize(&NestedParent {
            id: 42,
            child: NestedChild {
                code: "alpha".into(),
                active: true,
            },
            tags: vec!["core".into(), "json".into()],
        })
    }

    /// MsgPack bytes for BoolWrapper
    pub fn scalar_bool_bytes() -> Vec<u8> {
        serialize(&BoolWrapper {
            yes: true,
            no: false,
        })
    }

    /// MsgPack bytes for IntegerTypes
    pub fn scalar_integers_bytes() -> Vec<u8> {
        serialize(&IntegerTypes {
            signed_8: -128,
            unsigned_8: 255,
            signed_32: -2_147_483_648,
            unsigned_32: 4_294_967_295,
            signed_64: -9_223_372_036_854_775_808,
            unsigned_64: 18_446_744_073_709_551_615,
        })
    }

    /// MsgPack bytes for FloatTypes
    pub fn scalar_floats_bytes() -> Vec<u8> {
        serialize(&FloatTypes {
            float_32: 1.5,
            float_64: 2.25,
        })
    }

    /// MsgPack bytes for MapWrapper (HashMap/BTreeMap)
    pub fn map_string_keys_bytes() -> Vec<u8> {
        let mut map = std::collections::BTreeMap::new();
        map.insert("alpha".to_string(), 1);
        map.insert("beta".to_string(), 2);
        serialize(&MapWrapper { data: map })
    }

    /// MsgPack bytes for TupleWrapper
    pub fn tuple_simple_bytes() -> Vec<u8> {
        serialize(&TupleWrapper {
            triple: ("hello".to_string(), 42, true),
        })
    }

    /// MsgPack bytes for NestedTupleWrapper
    pub fn tuple_nested_bytes() -> Vec<u8> {
        serialize(&NestedTupleWrapper {
            outer: ((1, 2), ("test".to_string(), true)),
        })
    }

    /// MsgPack bytes for WithOption (None case)
    pub fn option_none_bytes() -> Vec<u8> {
        serialize(&WithOption {
            name: "test".to_string(),
            nickname: None,
        })
    }

    /// MsgPack bytes for WithOption (Some case)
    pub fn option_some_bytes() -> Vec<u8> {
        serialize(&WithOption {
            name: "test".to_string(),
            nickname: Some("nick".to_string()),
        })
    }

    /// MsgPack bytes for UnitVariantEnum
    pub fn enum_unit_variant_bytes() -> Vec<u8> {
        serialize(&UnitVariantEnum::Active)
    }

    /// MsgPack bytes for NestedVecWrapper
    pub fn vec_nested_bytes() -> Vec<u8> {
        serialize(&NestedVecWrapper {
            matrix: vec![vec![1, 2], vec![3, 4, 5]],
        })
    }

    /// MsgPack bytes for BytesWrapper
    pub fn bytes_vec_u8_bytes() -> Vec<u8> {
        serialize(&BytesWrapper {
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        })
    }

    /// MsgPack bytes for CharWrapper
    pub fn char_scalar_bytes() -> Vec<u8> {
        serialize(&CharWrapper {
            letter: 'A',
            emoji: '🦀',
        })
    }

    /// MsgPack bytes for UnitStruct
    pub fn unit_struct_bytes() -> Vec<u8> {
        serialize(&UnitStruct)
    }

    /// MsgPack bytes for HashSetWrapper
    pub fn hashset_bytes() -> Vec<u8> {
        let mut items = std::collections::HashSet::new();
        items.insert("alpha".to_string());
        items.insert("beta".to_string());
        items.insert("gamma".to_string());
        serialize(&HashSetWrapper { items })
    }

    /// MsgPack bytes for EmptyTupleWrapper
    pub fn tuple_empty_bytes() -> Vec<u8> {
        serialize(&EmptyTupleWrapper {
            name: "test".to_string(),
            empty: (),
        })
    }

    /// MsgPack bytes for SingleElementTupleWrapper
    pub fn tuple_single_element_bytes() -> Vec<u8> {
        serialize(&SingleElementTupleWrapper {
            name: "test".to_string(),
            single: (42,),
        })
    }

    /// MsgPack bytes for ComplexEnum (Label variant)
    pub fn enum_complex_bytes() -> Vec<u8> {
        serialize(&ComplexEnum::Label {
            name: "facet".into(),
            level: 7,
        })
    }

    /// MsgPack bytes for BoxWrapper
    pub fn box_wrapper_bytes() -> Vec<u8> {
        serialize(&BoxWrapper {
            inner: Box::new(42),
        })
    }

    /// MsgPack bytes for ArcWrapper
    pub fn arc_wrapper_bytes() -> Vec<u8> {
        serialize(&ArcWrapper {
            inner: std::sync::Arc::new(42),
        })
    }

    /// MsgPack bytes for RcWrapper
    pub fn rc_wrapper_bytes() -> Vec<u8> {
        serialize(&RcWrapper {
            inner: std::rc::Rc::new(42),
        })
    }

    /// MsgPack bytes for SetWrapper (BTreeSet)
    pub fn set_btree_bytes() -> Vec<u8> {
        let mut items = std::collections::BTreeSet::new();
        items.insert("alpha".to_string());
        items.insert("beta".to_string());
        items.insert("gamma".to_string());
        serialize(&SetWrapper { items })
    }

    /// MsgPack bytes for IntegerTypes16
    pub fn scalar_integers_16_bytes() -> Vec<u8> {
        serialize(&IntegerTypes16 {
            signed_16: -32768,
            unsigned_16: 65535,
        })
    }

    /// MsgPack bytes for IntegerTypes128
    pub fn scalar_integers_128_bytes() -> Vec<u8> {
        serialize(&IntegerTypes128 {
            signed_128: -170141183460469231731687303715884105728,
            unsigned_128: 340282366920938463463374607431768211455,
        })
    }

    /// MsgPack bytes for IntegerTypesSize
    pub fn scalar_integers_size_bytes() -> Vec<u8> {
        serialize(&IntegerTypesSize {
            signed_size: -1000,
            unsigned_size: 2000,
        })
    }

    /// MsgPack bytes for NonZeroTypes
    pub fn nonzero_integers_bytes() -> Vec<u8> {
        serialize(&NonZeroTypes {
            nz_u32: std::num::NonZeroU32::new(42).unwrap(),
            nz_i64: std::num::NonZeroI64::new(-100).unwrap(),
        })
    }

    /// MsgPack bytes for CowStrWrapper
    pub fn cow_str_bytes() -> Vec<u8> {
        serialize(&CowStrWrapper {
            owned: std::borrow::Cow::Owned("hello world".to_string()),
            message: std::borrow::Cow::Borrowed("borrowed"),
        })
    }

    /// MsgPack bytes for ArrayWrapper
    pub fn array_fixed_size_bytes() -> Vec<u8> {
        serialize(&ArrayWrapper { values: [1, 2, 3] })
    }

    /// MsgPack bytes for SkipUnknownStruct with extra fields
    /// Includes unknown field "extra" that should be ignored during deserialization
    pub fn skip_unknown_fields_bytes() -> Vec<u8> {
        #[derive(serde::Serialize)]
        struct WithExtra {
            known: String,
            extra: String,
        }
        serialize(&WithExtra {
            known: "value".to_string(),
            extra: "ignored".to_string(),
        })
    }

    // ── Network type MsgPack bytes ──
    // Note: facet serializes net types as strings, not as serde's tuple/array format.
    // These helpers create MsgPack with string values for compatibility with facet.

    /// Helper to create MsgPack bytes for a struct with a single string "addr" field.
    /// MsgPack format: fixmap(1) + fixstr("addr") + str(value)
    #[cfg(feature = "net")]
    fn net_addr_wrapper_bytes(addr_str: &str) -> Vec<u8> {
        use std::collections::BTreeMap;
        let mut map: BTreeMap<&str, &str> = BTreeMap::new();
        map.insert("addr", addr_str);
        serialize(&map)
    }

    /// MsgPack bytes for IpAddrV4Wrapper
    #[cfg(feature = "net")]
    pub fn net_ip_addr_v4_bytes() -> Vec<u8> {
        net_addr_wrapper_bytes("192.168.1.1")
    }

    /// MsgPack bytes for IpAddrV6Wrapper
    #[cfg(feature = "net")]
    pub fn net_ip_addr_v6_bytes() -> Vec<u8> {
        net_addr_wrapper_bytes("2001:db8::1")
    }

    /// MsgPack bytes for Ipv4AddrWrapper
    #[cfg(feature = "net")]
    pub fn net_ipv4_addr_bytes() -> Vec<u8> {
        net_addr_wrapper_bytes("127.0.0.1")
    }

    /// MsgPack bytes for Ipv6AddrWrapper
    #[cfg(feature = "net")]
    pub fn net_ipv6_addr_bytes() -> Vec<u8> {
        net_addr_wrapper_bytes("::1")
    }

    /// MsgPack bytes for SocketAddrV4Wrapper
    #[cfg(feature = "net")]
    pub fn net_socket_addr_v4_bytes() -> Vec<u8> {
        net_addr_wrapper_bytes("192.168.1.1:8080")
    }

    /// MsgPack bytes for SocketAddrV6Wrapper
    #[cfg(feature = "net")]
    pub fn net_socket_addr_v6_bytes() -> Vec<u8> {
        net_addr_wrapper_bytes("[2001:db8::1]:443")
    }

    /// MsgPack bytes for SocketAddrV4ExplicitWrapper
    #[cfg(feature = "net")]
    pub fn net_socket_addr_v4_explicit_bytes() -> Vec<u8> {
        net_addr_wrapper_bytes("10.0.0.1:3000")
    }

    /// MsgPack bytes for SocketAddrV6ExplicitWrapper
    #[cfg(feature = "net")]
    pub fn net_socket_addr_v6_explicit_bytes() -> Vec<u8> {
        net_addr_wrapper_bytes("[fe80::1]:9000")
    }
}

/// Display showcase for dynamic input (binary formats).
fn emit_case_showcase_dynamic<S, T>(
    desc: &'static CaseDescriptor<T>,
    note: Option<&'static str>,
    roundtrip_disabled_reason: Option<&'static str>,
    input: &[u8],
    actual: &T,
) where
    S: FormatSuite,
    for<'facet> T: Facet<'facet>,
    T: Debug,
{
    // Format binary input as hex dump
    let input_block = format_hex_dump(input);

    let pretty_output = format!(
        "{}",
        actual.pretty_with(PrettyPrinter::new().with_indent_size(2))
    );
    let note_line = note.map(|n| format!("note: {n}\n")).unwrap_or_default();
    let roundtrip_line = roundtrip_disabled_reason
        .map(|r| format!("roundtrip: disabled ({r})\n"))
        .unwrap_or_default();

    println!(
        "{}",
        formatdoc!(
            "
            ── facet-format-suite :: {format_name} :: {case_id} ──
            description: {description}
            {note_line}{roundtrip_line}Input (binary, {len} bytes):
            {input_block}

            facet-pretty output:
            {pretty_output}
            ",
            format_name = S::format_name(),
            case_id = desc.id,
            description = desc.description,
            note_line = note_line,
            roundtrip_line = roundtrip_line,
            len = input.len(),
            input_block = input_block,
            pretty_output = pretty_output,
        )
    );
}

/// Format bytes as a hex dump.
fn format_hex_dump(input: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for (i, chunk) in input.chunks(16).enumerate() {
        // Offset
        write!(output, "{:08x}  ", i * 16).unwrap();
        // Hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            if j == 8 {
                output.push(' ');
            }
            write!(output, "{:02x} ", byte).unwrap();
        }
        // Padding for incomplete lines
        for j in chunk.len()..16 {
            if j == 8 {
                output.push(' ');
            }
            output.push_str("   ");
        }
        // ASCII representation
        output.push_str(" |");
        for byte in chunk {
            if byte.is_ascii_graphic() || *byte == b' ' {
                output.push(*byte as char);
            } else {
                output.push('.');
            }
        }
        output.push_str("|\n");
    }
    output
}
