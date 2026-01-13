#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
use facet_xml::{XmlError, XmlParser, to_vec};
use indoc::indoc;
use libtest_mimic::{Arguments, Failed, Trial};
use std::sync::Arc;

struct XmlSlice;

impl FormatSuite for XmlSlice {
    type Error = DeserializeError<XmlError>;

    fn format_name() -> &'static str {
        "facet-xml/slice"
    }

    fn highlight_language() -> Option<&'static str> {
        Some("xml")
    }

    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        T: Facet<'static> + core::fmt::Debug,
    {
        let parser = XmlParser::new(input);
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
        use facet_xml::from_async_reader_tokio;
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
            <record>
                <name>facet</name>
            </record>
        "#
        ))
    }

    fn sequence_numbers() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            <numbers>
                <value>1</value>
                <value>2</value>
                <value>3</value>
            </numbers>
        "#
        ))
    }

    fn sequence_mixed_scalars() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            <mixed>
                <entry>-1</entry>
                <entry>4.625</entry>
                <entry>null</entry>
                <entry>true</entry>
            </mixed>
        "#
        ))
    }

    fn struct_nested() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            <parent>
                <id>42</id>
                <child>
                    <code>alpha</code>
                    <active>true</active>
                </child>
                <tags>
                    <item>core</item>
                    <item>json</item>
                </tags>
            </parent>
        "#
        ))
    }

    fn enum_complex() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            <enum>
                <Label>
                    <name>facet</name>
                    <level>7</level>
                </Label>
            </enum>
        "#
        ))
    }

    // ── Attribute cases ──

    fn attr_rename_field() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <userName>alice</userName>
                <age>30</age>
            </record>
        "#
        ))
    }

    fn attr_rename_all_camel() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <firstName>Jane</firstName>
                <lastName>Doe</lastName>
                <isActive>true</isActive>
            </record>
        "#
        ))
    }

    fn attr_default_field() -> CaseSpec {
        // optional_count is missing, should default to 0
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <required>present</required>
            </record>
        "#
        ))
    }

    fn attr_default_struct() -> CaseSpec {
        // message is missing, should use String::default() (empty string)
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <count>123</count>
            </record>
        "#
        ))
        .without_roundtrip("empty string serializes as empty element, which XML parses as struct")
    }

    fn attr_default_function() -> CaseSpec {
        // magic_number is missing, should use custom_default_value() = 42
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <name>hello</name>
            </record>
        "#
        ))
    }

    fn option_none() -> CaseSpec {
        // nickname is missing, should be None
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <name>test</name>
            </record>
        "#
        ))
    }

    fn option_some() -> CaseSpec {
        // nickname has a value
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <name>test</name>
                <nickname>nick</nickname>
            </record>
        "#
        ))
    }

    fn option_null() -> CaseSpec {
        // XML doesn't have null, skip this test
        CaseSpec::skip("XML has no null literal")
    }

    fn attr_skip_serializing() -> CaseSpec {
        // hidden field not in input (will use default), not serialized on roundtrip
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <visible>shown</visible>
            </record>
        "#
        ))
    }

    fn attr_skip_serializing_if() -> CaseSpec {
        // optional_data is None, skip_serializing_if = Option::is_none makes it absent in output
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <name>test</name>
            </record>
        "#
        ))
    }

    fn attr_skip() -> CaseSpec {
        // internal field is completely ignored - not read from input, not written on output
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <visible>data</visible>
            </record>
        "#
        ))
    }

    // ── Enum tagging cases ──

    fn enum_internally_tagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            <shape>
                <type>Circle</type>
                <radius>5.0</radius>
            </shape>
        "#
        ))
    }

    fn enum_adjacently_tagged() -> CaseSpec {
        CaseSpec::from_str(indoc!(
            r#"
            <value>
                <t>Message</t>
                <c>hello</c>
            </value>
        "#
        ))
    }

    // ── Advanced cases ──

    fn struct_flatten() -> CaseSpec {
        // x and y are flattened into the outer element
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <name>point</name>
                <x>10</x>
                <y>20</y>
            </record>
        "#
        ))
    }

    fn transparent_newtype() -> CaseSpec {
        // UserId(42) serializes as just 42, not a nested element
        CaseSpec::from_str(indoc!(
            r#"
            <record>
                <id>42</id>
                <name>alice</name>
            </record>
        "#
        ))
    }

    // ── Error cases ──

    fn deny_unknown_fields() -> CaseSpec {
        // Input has extra element "baz" which should trigger an error
        CaseSpec::expect_error(
            r#"<record><foo>abc</foo><bar>42</bar><baz>true</baz></record>"#,
            "unknown field",
        )
    }

    fn error_type_mismatch_string_to_int() -> CaseSpec {
        // String provided where integer expected
        CaseSpec::expect_error(
            r#"<record><value>not_a_number</value></record>"#,
            "failed to parse",
        )
    }

    fn error_type_mismatch_object_to_array() -> CaseSpec {
        // Object (nested struct) provided where array expected
        // Skip: XML elements are semantically ambiguous (ContainerKind::Element),
        // so they're accepted as potential sequences. JSON gives "type mismatch"
        // errors because it uses unambiguous ContainerKind::Object.
        CaseSpec::skip(
            "XML elements are ambiguous (ContainerKind::Element) - no type mismatch possible",
        )
    }

    fn error_missing_required_field() -> CaseSpec {
        // Missing required field "email"
        CaseSpec::expect_error(
            r#"<record><name>Alice</name><age>30</age></record>"#,
            "missing field",
        )
    }

    // ── Alias cases ──

    fn attr_alias() -> CaseSpec {
        // Input uses the alias "old_name" which should map to field "new_name"
        CaseSpec::from_str(r#"<record><old_name>value</old_name><count>5</count></record>"#)
            .without_roundtrip("alias is only for deserialization, serializes as new_name")
    }

    // ── Attribute precedence cases ──

    fn attr_rename_vs_alias_precedence() -> CaseSpec {
        // When both rename and alias are present, rename takes precedence for serialization
        CaseSpec::from_str(r#"<record><officialName>test</officialName><id>1</id></record>"#)
    }

    fn attr_rename_all_kebab() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><first-name>John</first-name><last-name>Doe</last-name><user-id>42</user-id></record>"#,
        )
    }

    fn attr_rename_all_screaming() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><API_KEY>secret-123</API_KEY><MAX_RETRY_COUNT>5</MAX_RETRY_COUNT></record>"#,
        )
    }

    fn attr_rename_unicode() -> CaseSpec {
        // Emoji is not a valid XML element name character
        CaseSpec::skip("Emoji not valid in XML element names")
    }

    fn attr_rename_special_chars() -> CaseSpec {
        // @ is not a valid XML element name character
        CaseSpec::skip("@ not valid in XML element names")
    }

    // ── Proxy cases ──

    fn proxy_container() -> CaseSpec {
        // ProxyInt deserializes from a string "42" via IntAsString proxy
        CaseSpec::from_str(r#"<value>42</value>"#)
    }

    fn proxy_field_level() -> CaseSpec {
        // Field-level proxy: "count" field deserializes from string "100" via proxy
        CaseSpec::from_str(r#"<record><name>test</name><count>100</count></record>"#)
    }

    fn proxy_validation_error() -> CaseSpec {
        // Proxy conversion fails with non-numeric string
        CaseSpec::expect_error(r#"<value>not_a_number</value>"#, "invalid digit")
    }

    fn proxy_with_option() -> CaseSpec {
        CaseSpec::from_str(r#"<record><name>test</name><count>42</count></record>"#)
    }

    fn proxy_with_enum() -> CaseSpec {
        CaseSpec::from_str(r#"<value><Value>99</Value></value>"#)
    }

    fn proxy_with_transparent() -> CaseSpec {
        CaseSpec::from_str(r#"<value>42</value>"#)
    }

    fn opaque_proxy() -> CaseSpec {
        // OpaqueType doesn't implement Facet, but OpaqueTypeProxy does
        // Use PartialEq comparison since reflection can't peek into opaque types
        CaseSpec::from_str(r#"<record><value><inner>42</inner></value></record>"#).with_partial_eq()
    }

    fn opaque_proxy_option() -> CaseSpec {
        // Optional opaque field with proxy
        // Use PartialEq comparison since reflection can't peek into opaque types
        CaseSpec::from_str(r#"<record><value><inner>99</inner></value></record>"#).with_partial_eq()
    }

    fn transparent_multilevel() -> CaseSpec {
        CaseSpec::from_str(r#"<value>42</value>"#)
    }

    fn transparent_option() -> CaseSpec {
        CaseSpec::from_str(r#"<value>99</value>"#)
    }

    fn transparent_nonzero() -> CaseSpec {
        CaseSpec::from_str(r#"<value>42</value>"#)
    }

    fn flatten_optional_some() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><name>test</name><version>1</version><author>alice</author></record>"#,
        )
    }

    fn flatten_optional_none() -> CaseSpec {
        CaseSpec::from_str(r#"<record><name>test</name></record>"#)
    }

    fn flatten_overlapping_fields_error() -> CaseSpec {
        // Two flattened structs both have a "shared" element - should error
        CaseSpec::expect_error(
            r#"<record><field_a>a</field_a><field_b>b</field_b><shared>1</shared></record>"#,
            "duplicate field",
        )
    }

    fn flatten_multilevel() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><top_field>top</top_field><mid_field>42</mid_field><deep_field>100</deep_field></record>"#,
        )
    }

    fn flatten_multiple_enums() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><name>service</name><Password><password>secret</password></Password><Tcp><port>8080</port></Tcp></record>"#,
        )
    }

    // ── Scalar cases ──

    fn scalar_bool() -> CaseSpec {
        CaseSpec::from_str(r#"<record><yes>true</yes><no>false</no></record>"#)
    }

    fn scalar_integers() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><signed_8>-128</signed_8><unsigned_8>255</unsigned_8><signed_32>-2147483648</signed_32><unsigned_32>4294967295</unsigned_32><signed_64>-9223372036854775808</signed_64><unsigned_64>18446744073709551615</unsigned_64></record>"#,
        )
    }

    fn scalar_floats() -> CaseSpec {
        CaseSpec::from_str(r#"<record><float_32>1.5</float_32><float_64>2.25</float_64></record>"#)
    }

    // ── Collection cases ──

    fn map_string_keys() -> CaseSpec {
        CaseSpec::from_str(r#"<record><data><alpha>1</alpha><beta>2</beta></data></record>"#)
    }

    fn tuple_simple() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><triple><item>hello</item><item>42</item><item>true</item></triple></record>"#,
        )
    }

    fn tuple_nested() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><outer><item><item>1</item><item>2</item></item><item><item>test</item><item>true</item></item></outer></record>"#,
        )
    }

    fn tuple_empty() -> CaseSpec {
        CaseSpec::from_str(r#"<record><name>test</name><empty/></record>"#)
            .without_roundtrip("empty tuple serialization format mismatch")
    }

    fn tuple_single_element() -> CaseSpec {
        CaseSpec::from_str(r#"<record><name>test</name><single><item>42</item></single></record>"#)
    }

    fn tuple_struct_variant() -> CaseSpec {
        CaseSpec::from_str(r#"<value><Pair><item>test</item><item>42</item></Pair></value>"#)
    }

    fn tuple_newtype_variant() -> CaseSpec {
        CaseSpec::from_str(r#"<value><Some>99</Some></value>"#)
    }

    // ── Enum variant cases ──

    fn enum_unit_variant() -> CaseSpec {
        CaseSpec::from_str(r#"<value>Active</value>"#)
    }

    fn enum_untagged() -> CaseSpec {
        CaseSpec::from_str(r#"<value><x>10</x><y>20</y></value>"#)
    }

    fn enum_variant_rename() -> CaseSpec {
        // Variant "Active" is renamed to "enabled" in the input
        CaseSpec::from_str(r#"<value>enabled</value>"#)
    }

    fn untagged_with_null() -> CaseSpec {
        CaseSpec::skip("XML empty elements don't map to unit variants in untagged enums")
    }

    fn untagged_newtype_variant() -> CaseSpec {
        CaseSpec::from_str(r#"<value>test</value>"#)
    }

    fn untagged_as_field() -> CaseSpec {
        CaseSpec::from_str(r#"<value><name>test</name><value>42</value></value>"#)
    }

    fn untagged_unit_only() -> CaseSpec {
        // Untagged enum with only unit variants, deserialized from string "Alpha"
        CaseSpec::from_str(r#"<value>Alpha</value>"#)
    }

    // ── Smart pointer cases ──

    fn box_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"<record><inner>42</inner></record>"#)
    }

    fn arc_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"<record><inner>42</inner></record>"#)
    }

    fn rc_wrapper() -> CaseSpec {
        CaseSpec::from_str(r#"<record><inner>42</inner></record>"#)
    }

    // ── Set cases ──

    fn set_btree() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><items><item>alpha</item><item>beta</item><item>gamma</item></items></record>"#,
        )
    }

    // ── Extended numeric cases ──

    fn scalar_integers_16() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><signed_16>-32768</signed_16><unsigned_16>65535</unsigned_16></record>"#,
        )
    }

    fn scalar_integers_128() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><signed_128>-170141183460469231731687303715884105728</signed_128><unsigned_128>340282366920938463463374607431768211455</unsigned_128></record>"#,
        )
    }

    fn scalar_integers_size() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><signed_size>-1000</signed_size><unsigned_size>2000</unsigned_size></record>"#,
        )
    }

    // ── NonZero cases ──

    fn nonzero_integers() -> CaseSpec {
        CaseSpec::from_str(r#"<record><nz_u32>42</nz_u32><nz_i64>-100</nz_i64></record>"#)
    }

    // ── Borrowed string cases ──

    fn cow_str() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><owned>hello world</owned><message>borrowed</message></record>"#,
        )
    }

    // ── Bytes/binary data cases ──

    fn bytes_vec_u8() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><data><value>0</value><value>128</value><value>255</value><value>42</value></data></record>"#,
        )
    }

    // ── Fixed-size array cases ──

    fn array_fixed_size() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><values><value>1</value><value>2</value><value>3</value></values></record>"#,
        )
    }

    // ── Unknown field handling cases ──

    fn skip_unknown_fields() -> CaseSpec {
        // Input has extra "unknown" element which should be silently skipped
        CaseSpec::from_str(r#"<record><unknown>ignored</unknown><known>value</known></record>"#)
            .without_roundtrip("unknown field is not preserved")
    }

    // ── String escape cases ──

    fn string_escapes() -> CaseSpec {
        // XML escapes: &#10; (newline), &#9; (tab), &quot; ("), backslash is literal
        CaseSpec::from_str(
            r#"<record><text>line1&#10;line2&#9;tab&quot;quote\backslash</text></record>"#,
        )
    }

    // ── Unit type cases ──

    fn unit_struct() -> CaseSpec {
        // Unit struct serializes as empty element in XML
        CaseSpec::from_str(r#"<UnitStruct/>"#)
    }

    // ── Newtype cases ──

    fn newtype_u64() -> CaseSpec {
        CaseSpec::from_str(r#"<record><value>42</value></record>"#)
    }

    fn newtype_string() -> CaseSpec {
        CaseSpec::from_str(r#"<record><value>hello</value></record>"#)
    }

    // ── Char cases ──

    fn char_scalar() -> CaseSpec {
        CaseSpec::from_str(r#"<record><letter>A</letter><emoji>🦀</emoji></record>"#)
    }

    // ── HashSet cases ──

    fn hashset() -> CaseSpec {
        CaseSpec::from_str(r#"<record><items><item>alpha</item><item>beta</item></items></record>"#)
    }

    // ── Nested collection cases ──

    fn vec_nested() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><matrix><item><value>1</value><value>2</value></item><item><value>3</value><value>4</value><value>5</value></item></matrix></record>"#,
        )
    }

    // ── Third-party type cases ──

    fn uuid() -> CaseSpec {
        // UUID in canonical hyphenated format
        CaseSpec::from_str(r#"<record><id>550e8400-e29b-41d4-a716-446655440000</id></record>"#)
    }

    fn ulid() -> CaseSpec {
        // ULID in standard Crockford Base32 format
        CaseSpec::from_str(r#"<record><id>01ARZ3NDEKTSV4RRFFQ69G5FAV</id></record>"#)
    }

    fn camino_path() -> CaseSpec {
        CaseSpec::from_str(r#"<record><path>/home/user/documents</path></record>"#)
    }

    fn ordered_float() -> CaseSpec {
        CaseSpec::from_str(r#"<record><value>1.23456</value></record>"#)
    }

    fn rust_decimal() -> CaseSpec {
        CaseSpec::from_str(r#"<record><amount>24.99</amount></record>"#)
    }

    // ── Scientific notation floats ──

    fn scalar_floats_scientific() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><large>1.23e10</large><small>-4.56e-7</small><positive_exp>5e3</positive_exp></record>"#,
        )
    }

    // ── Extended escape sequences ──

    fn string_escapes_extended() -> CaseSpec {
        // XML uses numeric character references for control characters
        CaseSpec::from_str(
            r#"<record><backspace>hello&#8;world</backspace><formfeed>page&#12;break</formfeed><carriage_return>line&#13;return</carriage_return><control_char>&#1;</control_char></record>"#,
        )
    }

    // ── Unsized smart pointer cases ──

    fn box_str() -> CaseSpec {
        CaseSpec::from_str(r#"<record><inner>hello world</inner></record>"#)
    }

    fn arc_str() -> CaseSpec {
        CaseSpec::from_str(r#"<record><inner>hello world</inner></record>"#)
    }

    fn rc_str() -> CaseSpec {
        CaseSpec::from_str(r#"<record><inner>hello world</inner></record>"#)
    }

    fn arc_slice() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><inner><item>1</item><item>2</item><item>3</item><item>4</item></inner></record>"#,
        )
    }

    // ── Extended NonZero cases ──

    fn nonzero_integers_extended() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><nz_u8>255</nz_u8><nz_i8>-128</nz_i8><nz_u16>65535</nz_u16><nz_i16>-32768</nz_i16><nz_u128>1</nz_u128><nz_i128>-1</nz_i128><nz_usize>1000</nz_usize><nz_isize>-500</nz_isize></record>"#,
        )
    }

    // ── DateTime type cases ──

    fn time_offset_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"<record><created_at>2023-01-15T12:34:56Z</created_at></record>"#)
    }

    fn jiff_timestamp() -> CaseSpec {
        CaseSpec::from_str(r#"<record><created_at>2023-12-31T11:30:00Z</created_at></record>"#)
    }

    fn jiff_civil_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"<record><created_at>2024-06-19T15:22:45</created_at></record>"#)
    }

    fn chrono_datetime_utc() -> CaseSpec {
        CaseSpec::from_str(r#"<record><created_at>2023-01-15T12:34:56Z</created_at></record>"#)
    }

    fn chrono_naive_datetime() -> CaseSpec {
        CaseSpec::from_str(r#"<record><created_at>2023-01-15T12:34:56</created_at></record>"#)
    }

    fn chrono_naive_date() -> CaseSpec {
        CaseSpec::from_str(r#"<record><birth_date>2023-01-15</birth_date></record>"#)
    }

    fn chrono_naive_time() -> CaseSpec {
        CaseSpec::from_str(r#"<record><alarm_time>12:34:56</alarm_time></record>"#)
    }

    fn chrono_in_vec() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><timestamps><item>2023-01-01T00:00:00Z</item><item>2023-06-15T12:30:00Z</item></timestamps></record>"#,
        )
    }

    fn chrono_duration() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><duration><item>3600</item><item>500000000</item></duration></record>"#,
        )
    }

    fn chrono_duration_negative() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><duration><item>-90</item><item>-250000000</item></duration></record>"#,
        )
    }

    // ── Bytes crate cases ──

    fn bytes_bytes() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><data><item>1</item><item>2</item><item>3</item><item>4</item><item>255</item></data></record>"#,
        )
    }

    fn bytes_bytes_mut() -> CaseSpec {
        CaseSpec::from_str(
            r#"<record><data><item>1</item><item>2</item><item>3</item><item>4</item><item>255</item></data></record>"#,
        )
    }

    // ── String optimization crate cases ──

    fn bytestring() -> CaseSpec {
        CaseSpec::from_str(r#"<record><value>hello world</value></record>"#)
    }

    fn compact_string() -> CaseSpec {
        CaseSpec::from_str(r#"<record><value>hello world</value></record>"#)
    }

    fn smartstring() -> CaseSpec {
        CaseSpec::from_str(r#"<record><value>hello world</value></record>"#)
    }

    fn smol_str() -> CaseSpec {
        CaseSpec::from_str(r#"<record><value>hello world</value></record>"#)
    }

    // ── Dynamic value cases ──

    fn value_null() -> CaseSpec {
        CaseSpec::from_str("<value>null</value>")
    }

    fn value_bool() -> CaseSpec {
        CaseSpec::from_str("<value>true</value>")
    }

    fn value_integer() -> CaseSpec {
        CaseSpec::from_str("<value>42</value>")
    }

    fn value_float() -> CaseSpec {
        CaseSpec::from_str("<value>2.5</value>")
    }

    fn value_string() -> CaseSpec {
        CaseSpec::from_str("<value>hello world</value>")
    }

    fn value_array() -> CaseSpec {
        CaseSpec::from_str("<array><item>1</item><item>2</item><item>3</item></array>")
    }

    fn value_object() -> CaseSpec {
        CaseSpec::from_str("<object><name>test</name><count>42</count></object>")
    }

    fn numeric_enum() -> CaseSpec {
        CaseSpec::from_str("<value>1</value>")
    }

    fn signed_numeric_enum() -> CaseSpec {
        CaseSpec::from_str("<value>-1</value>")
    }

    fn inferred_numeric_enum() -> CaseSpec {
        CaseSpec::from_str("<value>0</value>")
    }

    // ── Network type cases ──

    fn net_ip_addr_v4() -> CaseSpec {
        CaseSpec::from_str("<record><addr>192.168.1.1</addr></record>")
    }

    fn net_ip_addr_v6() -> CaseSpec {
        CaseSpec::from_str("<record><addr>2001:db8::1</addr></record>")
    }

    fn net_ipv4_addr() -> CaseSpec {
        CaseSpec::from_str("<record><addr>127.0.0.1</addr></record>")
    }

    fn net_ipv6_addr() -> CaseSpec {
        CaseSpec::from_str("<record><addr>::1</addr></record>")
    }

    fn net_socket_addr_v4() -> CaseSpec {
        CaseSpec::from_str("<record><addr>192.168.1.1:8080</addr></record>")
    }

    fn net_socket_addr_v6() -> CaseSpec {
        CaseSpec::from_str("<record><addr>[2001:db8::1]:443</addr></record>")
    }

    fn net_socket_addr_v4_explicit() -> CaseSpec {
        CaseSpec::from_str("<record><addr>10.0.0.1:3000</addr></record>")
    }

    fn net_socket_addr_v6_explicit() -> CaseSpec {
        CaseSpec::from_str("<record><addr>[fe80::1]:9000</addr></record>")
    }
}

fn main() {
    let args = Arguments::from_args();

    // Sync tests
    let sync_trials: Vec<Trial> = all_cases::<XmlSlice>()
        .into_iter()
        .map(|case| {
            let case = Arc::new(case);
            let name = format!("{}::{}", XmlSlice::format_name(), case.id);
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

    // Async tests (tokio)
    #[cfg(feature = "tokio")]
    let async_trials: Vec<Trial> = all_cases::<XmlSlice>()
        .into_iter()
        .map(|case| {
            let case = Arc::new(case);
            let name = format!("{}::{}/async", XmlSlice::format_name(), case.id);
            let skip_reason = case.skip_reason();
            let case_clone = Arc::clone(&case);
            let mut trial = Trial::test(name, move || match case_clone.run_async() {
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

    #[cfg(feature = "tokio")]
    let trials: Vec<Trial> = sync_trials.into_iter().chain(async_trials).collect();

    #[cfg(not(feature = "tokio"))]
    let trials = sync_trials;

    libtest_mimic::run(&args, trials).exit()
}
