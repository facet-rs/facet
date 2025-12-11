#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::{DeserializeError, FormatDeserializer};
use facet_format_xml::{XmlError, XmlParser, to_vec};
use facet_format_suite::{CaseOutcome, CaseSpec, FormatSuite, all_cases};
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
