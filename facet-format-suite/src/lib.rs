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
/// The [`deserialize`] hook is intentionally generic over every `T: Facet` – in
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
}

/// Execute suite cases; kept for convenience, but formats should register each
/// case individually via [`all_cases`].
pub fn run_suite<S: FormatSuite>() {
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
pub fn all_cases<S: FormatSuite>() -> Vec<SuiteCase> {
    vec![
        SuiteCase::new::<S, StructSingleField>(&CASE_STRUCT_SINGLE_FIELD, S::struct_single_field),
        SuiteCase::new::<S, Vec<u64>>(&CASE_SEQUENCE_NUMBERS, S::sequence_numbers),
        SuiteCase::new::<S, Vec<MixedScalar>>(
            &CASE_SEQUENCE_MIXED_SCALARS,
            S::sequence_mixed_scalars,
        ),
        SuiteCase::new::<S, NestedParent>(&CASE_STRUCT_NESTED, S::struct_nested),
        SuiteCase::new::<S, ComplexEnum>(&CASE_ENUM_COMPLEX, S::enum_complex),
    ]
}

/// Specification returned by each trait method.
#[derive(Debug, Clone)]
pub struct CaseSpec {
    payload: CasePayload,
    note: Option<&'static str>,
    roundtrip: RoundtripSpec,
}

impl CaseSpec {
    /// Provide raw bytes for the case input.
    pub const fn from_bytes(input: &'static [u8]) -> Self {
        Self {
            payload: CasePayload::Input(input),
            note: None,
            roundtrip: RoundtripSpec::Enabled,
        }
    }

    /// Convenience for UTF-8 inputs.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(input: &'static str) -> Self {
        Self::from_bytes(input.as_bytes())
    }

    /// Mark the case as skipped for this format, documenting the reason.
    pub const fn skip(reason: &'static str) -> Self {
        Self {
            payload: CasePayload::Skip { reason },
            note: None,
            roundtrip: RoundtripSpec::Enabled,
        }
    }

    /// Attach an optional note for diagnostics.
    pub fn with_note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }

    /// Disable round-trip checks for this case, documenting the reason.
    pub fn without_roundtrip(mut self, reason: &'static str) -> Self {
        self.roundtrip = RoundtripSpec::Disabled { reason };
        self
    }
}

#[derive(Debug, Clone)]
enum CasePayload {
    Input(&'static [u8]),
    Skip { reason: &'static str },
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
}

impl SuiteCase {
    fn new<S, T>(desc: &'static CaseDescriptor<T>, provider: fn() -> CaseSpec) -> Self
    where
        S: FormatSuite,
        for<'facet> T: Facet<'facet>,
        T: Debug + 'static,
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

    pub fn run(&self) -> CaseOutcome {
        (self.runner)()
    }

    pub fn skip_reason(&self) -> Option<&'static str> {
        self.skip_reason
    }
}

fn execute_case<S, T>(desc: &'static CaseDescriptor<T>, spec: CaseSpec) -> CaseOutcome
where
    S: FormatSuite,
    for<'facet> T: Facet<'facet>,
    T: Debug,
{
    let note = spec.note;
    let roundtrip_disabled_reason = match spec.roundtrip {
        RoundtripSpec::Enabled => None,
        RoundtripSpec::Disabled { reason } => Some(reason),
    };
    let highlight_language = S::highlight_language();
    match spec.payload {
        CasePayload::Skip { reason } => CaseOutcome::Skipped(reason),
        CasePayload::Input(input) => {
            let expected = (desc.expected)();
            let actual = match S::deserialize::<T>(input) {
                Ok(value) => value,
                Err(err) => return CaseOutcome::Failed(err.to_string()),
            };

            emit_case_showcase::<S, T>(
                desc,
                note,
                roundtrip_disabled_reason,
                input,
                highlight_language,
                &actual,
            );

            let first_assert = panic::catch_unwind(AssertUnwindSafe(|| {
                assert_same!(
                    actual,
                    expected,
                    "facet-format-suite {} ({}) produced unexpected value",
                    desc.id,
                    desc.description
                );
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

            match panic::catch_unwind(AssertUnwindSafe(|| {
                assert_same!(
                    roundtripped,
                    actual,
                    "facet-format-suite {} ({}) round-trip mismatch",
                    desc.id,
                    desc.description
                );
            })) {
                Ok(_) => CaseOutcome::Passed,
                Err(payload) => CaseOutcome::Failed(format_panic(payload)),
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

/// Shared fixture type for the struct case.
#[derive(Facet, Debug, Clone)]
pub struct StructSingleField {
    pub name: String,
}

/// Shared fixture type for the mixed scalars case.
#[derive(Facet, Debug, Clone)]
#[facet(untagged)]
#[repr(u8)]
pub enum MixedScalar {
    Signed(i64),
    Float(f64),
    Bool(bool),
    Null,
}

#[derive(Facet, Debug, Clone)]
pub struct NestedParent {
    pub id: u64,
    pub child: NestedChild,
    pub tags: Vec<String>,
}

#[derive(Facet, Debug, Clone)]
pub struct NestedChild {
    pub code: String,
    pub active: bool,
}

#[derive(Facet, Debug, Clone)]
#[repr(u8)]
pub enum ComplexEnum {
    Empty,
    Count(u64),
    Label { name: String, level: u8 },
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

fn highlight_payload(language: &str, input: &[u8]) -> Option<String> {
    let source = core::str::from_utf8(input).ok()?;
    let mut highlighter = Highlighter::new();
    highlighter.highlight_to_html(language, source).ok()
}
