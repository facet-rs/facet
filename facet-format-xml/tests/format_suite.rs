#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
use facet_format_xml::{XmlError, XmlParser, to_vec};
use indoc::indoc;
use libtest_mimic::{Arguments, Failed, Trial};

struct XmlSlice;

impl FormatSuite for XmlSlice {
    type Error = DeserializeError<XmlError>;

    fn format_name() -> &'static str {
        "facet-format-xml/slice"
    }

    fn highlight_language() -> Option<&'static str> {
        Some("xml")
    }

    fn deserialize<T>(input: &[u8]) -> Result<T, Self::Error>
    where
        T: Facet<'static> + core::fmt::Debug,
    {
        let parser = XmlParser::new(input);
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

    // ── Alias cases ──

    fn attr_alias() -> CaseSpec {
        // Input uses the alias "old_name" which should map to field "new_name"
        CaseSpec::from_str(r#"<record><old_name>value</old_name><count>5</count></record>"#)
            .without_roundtrip("alias is only for deserialization, serializes as new_name")
    }

    // ── Proxy cases ──

    fn proxy_container() -> CaseSpec {
        // ProxyInt deserializes from a string "42" via IntAsString proxy
        CaseSpec::from_str(r#"<value>42</value>"#)
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

    // ── Enum variant cases ──

    fn enum_unit_variant() -> CaseSpec {
        CaseSpec::from_str(r#"<value>Active</value>"#)
    }

    fn enum_untagged() -> CaseSpec {
        CaseSpec::from_str(r#"<value><x>10</x><y>20</y></value>"#)
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
        // Skip: VNumber can't hold values outside i64/u64 range, so deserialization fails
        CaseSpec::skip("i128/u128 values exceed VNumber range")
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
}

fn main() {
    let args = Arguments::from_args();
    let trials: Vec<Trial> = all_cases::<XmlSlice>()
        .into_iter()
        .map(|case| {
            let name = format!("{}::{}", XmlSlice::format_name(), case.id);
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
