//! Hand-crafted malformed byte corpus for failure-path tests.
//!
//! Each entry is a `(&str, Vec<u8>)` pair: a label and the bytes.
//! Tests iterate over these and verify that the oracle returns the
//! expected `ErrorClass` (and that candidates agree).

use vox_postcard::error::DeserializeError;

use crate::differential::ErrorClass;

pub struct CorpusEntry {
    pub label: &'static str,
    pub bytes: Vec<u8>,
    pub expected: ErrorClass,
}

impl CorpusEntry {
    pub fn new(label: &'static str, bytes: Vec<u8>, expected: ErrorClass) -> Self {
        Self {
            label,
            bytes,
            expected,
        }
    }
}

/// Encode a varint (little-endian 7-bit groups, MSB continuation bit).
pub fn encode_varint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        } else {
            out.push(b | 0x80);
        }
    }
    out
}

/// All failure modes from the design doc, expressed as byte sequences that
/// should each produce a known error class against a u32 identity plan.
pub fn failure_corpus() -> Vec<CorpusEntry> {
    vec![
        // EOF: empty input for a u32 (varint with no bytes)
        CorpusEntry::new("eof-empty", vec![], ErrorClass::UnexpectedEof),
        // EOF: varint starts but continuation bit set with no following byte
        CorpusEntry::new(
            "eof-varint-truncated",
            vec![0x80],
            ErrorClass::UnexpectedEof,
        ),
        // Varint overflow: 10 bytes all with MSB set — goes past 64 bits
        CorpusEntry::new(
            "varint-overflow",
            vec![
                0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
            ],
            ErrorClass::VarintOverflow,
        ),
    ]
}

/// Corpus entries for String decode failures.
pub fn string_failure_corpus() -> Vec<CorpusEntry> {
    vec![
        // Invalid UTF-8: length 3, then 3 bytes that are not valid UTF-8 (0xFF 0xFE 0xFD)
        CorpusEntry::new(
            "invalid-utf8",
            {
                let mut v = encode_varint(3);
                v.extend_from_slice(&[0xFF, 0xFE, 0xFD]);
                v
            },
            ErrorClass::InvalidUtf8,
        ),
        // EOF: length claims 10 bytes but only 3 follow
        CorpusEntry::new(
            "string-eof-truncated",
            {
                let mut v = encode_varint(10);
                v.extend_from_slice(b"abc");
                v
            },
            ErrorClass::UnexpectedEof,
        ),
    ]
}

/// Corpus entries for enum discriminant failures during raw skip (no plan).
///
/// These are used with `skip_value` (no translation plan), where an
/// out-of-range discriminant produces `InvalidEnumDiscriminant`.
///
/// NOTE: when decoding via a translation plan, out-of-range discriminants
/// that are not in the variant map produce `UnknownVariant` instead.
/// See `failure_mode_tests::oracle_unknown_remote_variant` for that case.
pub fn enum_skip_discriminant_corpus() -> Vec<CorpusEntry> {
    vec![
        // Discriminant 99 — out of range, produces InvalidEnumDiscriminant on skip path
        CorpusEntry::new(
            "skip-invalid-discriminant-99",
            encode_varint(99),
            ErrorClass::InvalidEnumDiscriminant,
        ),
        // Discriminant 255
        CorpusEntry::new(
            "skip-invalid-discriminant-255",
            encode_varint(255),
            ErrorClass::InvalidEnumDiscriminant,
        ),
    ]
}

/// Corpus for enum discriminant failures via plan-based decode.
/// These produce `UnknownVariant` (not `InvalidEnumDiscriminant`) because
/// the plan handles unknown variants explicitly.
pub fn enum_plan_discriminant_corpus() -> Vec<CorpusEntry> {
    vec![
        CorpusEntry::new(
            "plan-unknown-discriminant-99",
            encode_varint(99),
            ErrorClass::UnknownVariant,
        ),
        CorpusEntry::new(
            "plan-unknown-discriminant-255",
            encode_varint(255),
            ErrorClass::UnknownVariant,
        ),
    ]
}

/// Corpus entries for Option tag failures.
pub fn option_tag_corpus() -> Vec<CorpusEntry> {
    vec![
        // Option tag 0x02 is invalid (only 0x00 and 0x01 are valid)
        CorpusEntry::new(
            "invalid-option-tag-2",
            vec![0x02],
            ErrorClass::InvalidOptionTag,
        ),
        CorpusEntry::new(
            "invalid-option-tag-ff",
            vec![0xFF],
            ErrorClass::InvalidOptionTag,
        ),
        // EOF: no tag byte at all
        CorpusEntry::new("option-eof", vec![], ErrorClass::UnexpectedEof),
    ]
}

/// Verify a corpus entry against the oracle (reflective interpreter).
/// Returns the actual error for callers who want to also check the candidate.
pub fn check_oracle_error(entry: &CorpusEntry, oracle_err: &DeserializeError) {
    let actual = ErrorClass::of(oracle_err);
    assert_eq!(
        actual, entry.expected,
        "corpus entry '{}': expected {:?}, got {:?} (error: {oracle_err})",
        entry.label, entry.expected, actual,
    );
}
