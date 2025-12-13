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
    /// Case: `Option<T>` field with `None` value (missing in input).
    fn option_none() -> CaseSpec;
    /// Case: `#[facet(skip_serializing)]` field.
    fn attr_skip_serializing() -> CaseSpec;
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

    // ── Error cases ──

    /// Case: `#[facet(deny_unknown_fields)]` rejects unknown fields.
    fn deny_unknown_fields() -> CaseSpec;

    // ── Alias tests ──

    /// Case: field with `#[facet(alias = "...")]` accepts alternative name.
    fn attr_alias() -> CaseSpec;

    // ── Proxy tests ──

    /// Case: container-level `#[facet(proxy = ...)]` for custom serialization.
    fn proxy_container() -> CaseSpec;

    // ── Scalar tests ──

    /// Case: boolean scalar value.
    fn scalar_bool() -> CaseSpec;
    /// Case: various integer types.
    fn scalar_integers() -> CaseSpec;
    /// Case: floating point types.
    fn scalar_floats() -> CaseSpec;

    // ── Collection tests ──

    /// Case: `HashMap<String, T>`.
    fn map_string_keys() -> CaseSpec;
    /// Case: tuple types.
    fn tuple_simple() -> CaseSpec;

    // ── Enum variant tests ──

    /// Case: unit enum variant.
    fn enum_unit_variant() -> CaseSpec;
    /// Case: untagged enum.
    fn enum_untagged() -> CaseSpec;

    // ── Smart pointer tests ──

    /// Case: `Box<T>` smart pointer.
    fn box_wrapper() -> CaseSpec;
    /// Case: `Arc<T>` smart pointer.
    fn arc_wrapper() -> CaseSpec;
    /// Case: `Rc<T>` smart pointer.
    fn rc_wrapper() -> CaseSpec;

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

    // ── Borrowed string tests ──

    /// Case: Cow<'static, str> field.
    fn cow_str() -> CaseSpec;
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
        SuiteCase::new::<S, WithOption>(&CASE_OPTION_NONE, S::option_none),
        SuiteCase::new::<S, WithSkipSerializing>(
            &CASE_ATTR_SKIP_SERIALIZING,
            S::attr_skip_serializing,
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
        // Error cases
        SuiteCase::new::<S, DenyUnknownStruct>(&CASE_DENY_UNKNOWN_FIELDS, S::deny_unknown_fields),
        // Alias cases
        SuiteCase::new::<S, WithAlias>(&CASE_ATTR_ALIAS, S::attr_alias),
        // Proxy cases
        SuiteCase::new::<S, ProxyInt>(&CASE_PROXY_CONTAINER, S::proxy_container),
        // Scalar cases
        SuiteCase::new::<S, BoolWrapper>(&CASE_SCALAR_BOOL, S::scalar_bool),
        SuiteCase::new::<S, IntegerTypes>(&CASE_SCALAR_INTEGERS, S::scalar_integers),
        SuiteCase::new::<S, FloatTypes>(&CASE_SCALAR_FLOATS, S::scalar_floats),
        // Collection cases
        SuiteCase::new::<S, MapWrapper>(&CASE_MAP_STRING_KEYS, S::map_string_keys),
        SuiteCase::new::<S, TupleWrapper>(&CASE_TUPLE_SIMPLE, S::tuple_simple),
        // Enum variant cases
        SuiteCase::new::<S, UnitVariantEnum>(&CASE_ENUM_UNIT_VARIANT, S::enum_unit_variant),
        SuiteCase::new::<S, UntaggedEnum>(&CASE_ENUM_UNTAGGED, S::enum_untagged),
        // Smart pointer cases
        SuiteCase::new::<S, BoxWrapper>(&CASE_BOX_WRAPPER, S::box_wrapper),
        SuiteCase::new::<S, ArcWrapper>(&CASE_ARC_WRAPPER, S::arc_wrapper),
        SuiteCase::new::<S, RcWrapper>(&CASE_RC_WRAPPER, S::rc_wrapper),
        // Set cases
        SuiteCase::new::<S, SetWrapper>(&CASE_SET_BTREE, S::set_btree),
        // Extended numeric cases
        SuiteCase::new::<S, IntegerTypes16>(&CASE_SCALAR_INTEGERS_16, S::scalar_integers_16),
        SuiteCase::new::<S, IntegerTypes128>(&CASE_SCALAR_INTEGERS_128, S::scalar_integers_128),
        SuiteCase::new::<S, IntegerTypesSize>(&CASE_SCALAR_INTEGERS_SIZE, S::scalar_integers_size),
        // NonZero cases
        SuiteCase::new::<S, NonZeroTypes>(&CASE_NONZERO_INTEGERS, S::nonzero_integers),
        // Borrowed string cases
        SuiteCase::new::<S, CowStrWrapper>(&CASE_COW_STR, S::cow_str),
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

    /// Expect deserialization to fail with an error containing the given substring.
    pub fn expect_error(input: &'static str, error_contains: &'static str) -> Self {
        Self {
            payload: CasePayload::ExpectError {
                input: input.as_bytes(),
                error_contains,
            },
            note: None,
            roundtrip: RoundtripSpec::Disabled {
                reason: "error case",
            },
        }
    }
}

#[derive(Debug, Clone)]
enum CasePayload {
    Input(&'static [u8]),
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

const CASE_OPTION_NONE: CaseDescriptor<WithOption> = CaseDescriptor {
    id: "option::none",
    description: "Option<T> field missing from input becomes None",
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

// ── Error case descriptors ──

const CASE_DENY_UNKNOWN_FIELDS: CaseDescriptor<DenyUnknownStruct> = CaseDescriptor {
    id: "error::deny_unknown_fields",
    description: "#[facet(deny_unknown_fields)] rejects input with extra fields",
    expected: || DenyUnknownStruct {
        foo: "abc".into(),
        bar: 42,
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

// ── Proxy case descriptors ──

const CASE_PROXY_CONTAINER: CaseDescriptor<ProxyInt> = CaseDescriptor {
    id: "proxy::container",
    description: "container-level #[facet(proxy = IntAsString)] deserializes int from string",
    expected: || ProxyInt { value: 42 },
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

// ── Enum variant case descriptors ──

const CASE_ENUM_UNIT_VARIANT: CaseDescriptor<UnitVariantEnum> = CaseDescriptor {
    id: "enum::unit_variant",
    description: "enum with unit variants",
    expected: || UnitVariantEnum::Active,
};

const CASE_ENUM_UNTAGGED: CaseDescriptor<UntaggedEnum> = CaseDescriptor {
    id: "enum::untagged",
    description: "#[facet(untagged)] enum matches by structure",
    expected: || UntaggedEnum::Point { x: 10, y: 20 },
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

// ── Borrowed string case descriptors ──

const CASE_COW_STR: CaseDescriptor<CowStrWrapper> = CaseDescriptor {
    id: "string::cow_str",
    description: "Cow<'static, str> string fields",
    expected: || CowStrWrapper {
        owned: std::borrow::Cow::Owned("hello world".to_string()),
        message: std::borrow::Cow::Borrowed("borrowed"),
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

// ── Attribute test fixtures ──

/// Fixture for `#[facet(rename = "...")]` test.
#[derive(Facet, Debug, Clone)]
pub struct RenamedField {
    #[facet(rename = "userName")]
    pub user_name: String,
    pub age: u32,
}

/// Fixture for `#[facet(rename_all = "camelCase")]` test.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "camelCase")]
pub struct CamelCaseStruct {
    pub first_name: String,
    pub last_name: String,
    pub is_active: bool,
}

/// Fixture for `#[facet(default)]` test.
#[derive(Facet, Debug, Clone)]
pub struct WithDefault {
    pub required: String,
    #[facet(default)]
    pub optional_count: u32,
}

/// Fixture for `Option<T>` with `None`.
#[derive(Facet, Debug, Clone)]
pub struct WithOption {
    pub name: String,
    pub nickname: Option<String>,
}

/// Fixture for `#[facet(skip_serializing)]` test.
#[derive(Facet, Debug, Clone)]
pub struct WithSkipSerializing {
    pub visible: String,
    #[facet(skip_serializing)]
    #[facet(default)]
    pub hidden: String,
}

/// Fixture for `#[facet(skip)]` test (skipped for both ser and de).
#[derive(Facet, Debug, Clone)]
pub struct WithSkip {
    pub visible: String,
    #[facet(skip)]
    #[facet(default)]
    pub internal: u32,
}

// ── Enum tagging fixtures ──

/// Internally tagged enum `#[facet(tag = "type")]`.
#[derive(Facet, Debug, Clone)]
#[facet(tag = "type")]
#[repr(u8)]
pub enum InternallyTagged {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}

/// Adjacently tagged enum `#[facet(tag = "t", content = "c")]`.
#[derive(Facet, Debug, Clone)]
#[facet(tag = "t", content = "c")]
#[repr(u8)]
pub enum AdjacentlyTagged {
    Message(String),
    Count(u64),
}

// ── Advanced fixtures ──

/// Inner struct for flatten test.
#[derive(Facet, Debug, Clone)]
pub struct FlattenInner {
    pub x: i32,
    pub y: i32,
}

/// Outer struct with `#[facet(flatten)]`.
#[derive(Facet, Debug, Clone)]
pub struct FlattenOuter {
    pub name: String,
    #[facet(flatten)]
    pub coords: FlattenInner,
}

/// Transparent newtype wrapper.
#[derive(Facet, Debug, Clone)]
#[facet(transparent)]
pub struct UserId(pub u64);

/// Struct containing a transparent newtype.
#[derive(Facet, Debug, Clone)]
pub struct UserRecord {
    pub id: UserId,
    pub name: String,
}

// ── Error test fixtures ──

/// Fixture for `#[facet(deny_unknown_fields)]` test.
#[derive(Facet, Debug, Clone)]
#[facet(deny_unknown_fields)]
pub struct DenyUnknownStruct {
    pub foo: String,
    pub bar: i32,
}

/// Fixture for `#[facet(alias = "...")]` test.
#[derive(Facet, Debug, Clone)]
pub struct WithAlias {
    #[facet(alias = "old_name")]
    pub new_name: String,
    pub count: u32,
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

// ── Scalar test fixtures ──

/// Fixture for boolean scalar test.
#[derive(Facet, Debug, Clone)]
pub struct BoolWrapper {
    pub yes: bool,
    pub no: bool,
}

/// Fixture for integer scalar test.
#[derive(Facet, Debug, Clone)]
pub struct IntegerTypes {
    pub signed_8: i8,
    pub unsigned_8: u8,
    pub signed_32: i32,
    pub unsigned_32: u32,
    pub signed_64: i64,
    pub unsigned_64: u64,
}

/// Fixture for float scalar test.
#[derive(Facet, Debug, Clone)]
pub struct FloatTypes {
    pub float_32: f32,
    pub float_64: f64,
}

// ── Collection test fixtures ──

/// Fixture for BTreeMap test.
#[derive(Facet, Debug, Clone)]
pub struct MapWrapper {
    pub data: std::collections::BTreeMap<String, i32>,
}

/// Fixture for tuple test.
#[derive(Facet, Debug, Clone)]
pub struct TupleWrapper {
    pub triple: (String, i32, bool),
}

// ── Enum variant test fixtures ──

/// Unit variant enum.
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum UnitVariantEnum {
    Active,
    Inactive,
    Pending,
}

/// Untagged enum that matches by structure.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
pub enum UntaggedEnum {
    Point { x: i32, y: i32 },
    Value(i64),
}

// ── Smart pointer test fixtures ──

/// Fixture for `Box<T>` test.
#[derive(Facet, Debug, Clone)]
pub struct BoxWrapper {
    pub inner: Box<i32>,
}

/// Fixture for `Arc<T>` test.
#[derive(Facet, Debug, Clone)]
pub struct ArcWrapper {
    pub inner: std::sync::Arc<i32>,
}

/// Fixture for `Rc<T>` test.
#[derive(Facet, Debug, Clone)]
pub struct RcWrapper {
    pub inner: std::rc::Rc<i32>,
}

// ── Set test fixtures ──

/// Fixture for BTreeSet test.
#[derive(Facet, Debug, Clone)]
pub struct SetWrapper {
    pub items: std::collections::BTreeSet<String>,
}

// ── Extended numeric test fixtures ──

/// Fixture for 16-bit integer test.
#[derive(Facet, Debug, Clone)]
pub struct IntegerTypes16 {
    pub signed_16: i16,
    pub unsigned_16: u16,
}

/// Fixture for 128-bit integer test.
#[derive(Facet, Debug, Clone)]
pub struct IntegerTypes128 {
    pub signed_128: i128,
    pub unsigned_128: u128,
}

/// Fixture for pointer-sized integer test.
#[derive(Facet, Debug, Clone)]
pub struct IntegerTypesSize {
    pub signed_size: isize,
    pub unsigned_size: usize,
}

// ── NonZero test fixtures ──

/// Fixture for NonZero integer test.
#[derive(Facet, Debug, Clone)]
pub struct NonZeroTypes {
    pub nz_u32: std::num::NonZeroU32,
    pub nz_i64: std::num::NonZeroI64,
}

// ── Borrowed string test fixtures ──

/// Fixture for Cow<'static, str> test.
#[derive(Facet, Debug, Clone)]
pub struct CowStrWrapper {
    pub owned: std::borrow::Cow<'static, str>,
    pub message: std::borrow::Cow<'static, str>,
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
    highlighter.highlight_to_html(language, source).ok()
}
