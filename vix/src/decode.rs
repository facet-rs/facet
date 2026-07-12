//! Type-directed document decoding.
//!
//! One [`FormatParser`] pass per document, walked directly against the
//! compiler-known Vix target [`Type`]. No generic `Doc`/`Value` is
//! materialized and nothing walks an untyped tree: each parse event is
//! matched against the exact declared type it lands on, and the walk yields
//! typed leaves ([`DecodedValue`]) that the compiler lowers into ordinary
//! typed-construction VIR (records → `Op::Record`, strings → `Op::String`,
//! …). The machine therefore retains typed construction and framed store
//! identity; the "host call" is this single per-document parser drive.
//!
//! This is the scheduler-edge realization of
//! `r[machine.primitive.typed-deserialization]`: format parsing targets vix
//! structs directly via schema, one host call per document, typed store
//! values out, zero generic-Doc projection walking.

use facet_format::{FieldKey, FormatParser, ParseEventKind, ScalarValue};
use facet_json::JsonParser;
use facet_toml::TomlParser;

use crate::vir::{EnumType, RecordField, Type, VariantPayload};

/// Which document grammar a decode targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeFormat {
    Json,
    Toml,
}

impl DecodeFormat {
    fn label(self) -> &'static str {
        match self {
            DecodeFormat::Json => "JSON",
            DecodeFormat::Toml => "TOML",
        }
    }
}

/// A typed decode failure. Its `message` is the same value a runtime
/// `try_*_decode` surfaces as `DecodeError { message }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub message: String,
}

impl DecodeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// A decoded value shaped by the target type — never an untyped tree. Each
/// node was produced against a known declared type and is aligned to it
/// (record fields in declaration order, option presence resolved, variant
/// selected), so lowering it is a mechanical fold, not a projection walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodedValue {
    Int(i64),
    Bool(bool),
    Str(String),
    /// Record fields in declaration order.
    Record(Vec<DecodedValue>),
    OptionSome(Box<DecodedValue>),
    OptionNone,
    /// A selected enum variant with its payload in declaration order.
    Variant {
        index: u32,
        fields: Vec<DecodedValue>,
    },
}

/// Decode `source` against `target`, driving one parser to completion.
pub fn decode(
    format: DecodeFormat,
    source: &str,
    target: &Type,
) -> Result<DecodedValue, DecodeError> {
    match format {
        DecodeFormat::Json => {
            let mut parser: JsonParser<'_, false> = JsonParser::new(source.as_bytes());
            let value = decode_value(&mut parser, target)?;
            expect_end(&mut parser, format)?;
            Ok(value)
        }
        DecodeFormat::Toml => {
            let mut parser = TomlParser::new(source)
                .map_err(|err| DecodeError::new(format!("TOML parse error: {err:?}")))?;
            let value = decode_value(&mut parser, target)?;
            expect_end(&mut parser, format)?;
            Ok(value)
        }
    }
}

fn decode_value<'de>(
    parser: &mut dyn FormatParser<'de>,
    ty: &Type,
) -> Result<DecodedValue, DecodeError> {
    // Option is decoded before the generic enum path: absence at a document
    // leaf (an explicit null) is None, presence is Some(inner).
    if let Some(inner) = ty.option_inner() {
        if is_null(parser)? {
            consume(parser)?;
            return Ok(DecodedValue::OptionNone);
        }
        return Ok(DecodedValue::OptionSome(Box::new(decode_value(
            parser, inner,
        )?)));
    }

    match ty {
        Type::Int => match scalar(parser, ty)? {
            ScalarValue::I64(value) => Ok(DecodedValue::Int(value)),
            ScalarValue::U64(value) => i64::try_from(value)
                .map(DecodedValue::Int)
                .map_err(|_| DecodeError::new("expected Int, found an out-of-range integer")),
            other => Err(scalar_mismatch("Int", &other)),
        },
        Type::Bool => match scalar(parser, ty)? {
            ScalarValue::Bool(value) => Ok(DecodedValue::Bool(value)),
            other => Err(scalar_mismatch("Bool", &other)),
        },
        Type::String => match scalar(parser, ty)? {
            ScalarValue::Str(value) => Ok(DecodedValue::Str(value.into_owned())),
            other => Err(scalar_mismatch("String", &other)),
        },
        Type::Record(record) => Ok(DecodedValue::Record(decode_fields(
            parser,
            &record.name,
            &record.fields,
        )?)),
        Type::Enum(enumeration) => decode_enum(parser, enumeration),
        other => Err(DecodeError::new(format!(
            "cannot decode into {}",
            other.name()
        ))),
    }
}

/// The string-or-table enum form (the Cargo dependency shape). A scalar string
/// selects the short single-`String` tuple variant; an object selects the
/// detailed record variant and decodes directly into its fields. Both compose
/// through the same decoder as any other value.
fn decode_enum<'de>(
    parser: &mut dyn FormatParser<'de>,
    enumeration: &EnumType,
) -> Result<DecodedValue, DecodeError> {
    match peek_kind(parser)? {
        Some(ParseEventKind::Scalar(_)) => {
            let (index, inner) = string_form_variant(enumeration).ok_or_else(|| {
                DecodeError::new(format!(
                    "{} has no short (single-string) form for a scalar document",
                    enumeration.name
                ))
            })?;
            let field = decode_value(parser, inner)?;
            Ok(DecodedValue::Variant {
                index,
                fields: vec![field],
            })
        }
        Some(ParseEventKind::StructStart(_)) => {
            let (index, fields) = table_form_variant(enumeration).ok_or_else(|| {
                DecodeError::new(format!(
                    "{} has no detailed (table) form for an object document",
                    enumeration.name
                ))
            })?;
            let values = decode_fields(parser, &enumeration.name, fields)?;
            Ok(DecodedValue::Variant {
                index,
                fields: values,
            })
        }
        Some(other) => Err(DecodeError::new(format!(
            "expected a string or object for {}, found {}",
            enumeration.name,
            event_label(&other)
        ))),
        None => Err(DecodeError::new(format!(
            "expected a value for {}, found end of document",
            enumeration.name
        ))),
    }
}

/// The variant that carries the short form: a single-`String` tuple payload.
fn string_form_variant(enumeration: &EnumType) -> Option<(u32, &Type)> {
    enumeration
        .variants
        .iter()
        .enumerate()
        .find_map(|(index, variant)| match &variant.payload {
            VariantPayload::Tuple(types) => match types.as_slice() {
                [inner @ Type::String] => Some((index as u32, inner)),
                _ => None,
            },
            _ => None,
        })
}

/// The variant that carries the detailed form: a record payload.
fn table_form_variant(enumeration: &EnumType) -> Option<(u32, &[RecordField])> {
    enumeration
        .variants
        .iter()
        .enumerate()
        .find_map(|(index, variant)| match &variant.payload {
            VariantPayload::Record(fields) => Some((index as u32, fields.as_slice())),
            _ => None,
        })
}

/// Decode one object's fields against a declared field list, in declaration
/// order. Shared by struct records and record-variant table forms: fields are
/// matched by name, absent `Option` fields resolve to `None`, and any other
/// absence or unknown/duplicate field is a typed failure. `container` names the
/// struct or enum for error attribution.
fn decode_fields<'de>(
    parser: &mut dyn FormatParser<'de>,
    container: &str,
    fields: &[RecordField],
) -> Result<Vec<DecodedValue>, DecodeError> {
    match consume(parser)?.kind {
        ParseEventKind::StructStart(_) => {}
        other => {
            return Err(DecodeError::new(format!(
                "expected an object for {container}, found {}",
                event_label(&other)
            )));
        }
    }

    let mut slots: Vec<Option<DecodedValue>> = fields.iter().map(|_| None).collect();
    loop {
        match peek_kind(parser)? {
            None => break,
            Some(ParseEventKind::StructEnd) => {
                consume(parser)?;
                break;
            }
            Some(ParseEventKind::FieldKey(_)) => {
                let ParseEventKind::FieldKey(key) = consume(parser)?.kind else {
                    unreachable!("peeked a field key");
                };
                let name = field_name(&key);
                match fields.iter().position(|field| field.name == name) {
                    Some(index) => {
                        if slots[index].is_some() {
                            return Err(DecodeError::new(format!(
                                "duplicate field \"{name}\" in {container}"
                            )));
                        }
                        let value = decode_value(parser, &fields[index].ty)
                            .map_err(|err| field_context(&name, err))?;
                        slots[index] = Some(value);
                    }
                    None => {
                        return Err(DecodeError::new(format!(
                            "unknown field \"{name}\" in {container}"
                        )));
                    }
                }
            }
            Some(other) => {
                return Err(DecodeError::new(format!(
                    "expected a field of {container}, found {}",
                    event_label(&other)
                )));
            }
        }
    }

    let mut out = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        match slots[index].take() {
            Some(value) => out.push(value),
            None if field.ty.option_inner().is_some() => out.push(DecodedValue::OptionNone),
            None => {
                return Err(DecodeError::new(format!(
                    "missing field \"{}\" in {container}",
                    field.name
                )));
            }
        }
    }
    Ok(out)
}

fn scalar<'de>(
    parser: &mut dyn FormatParser<'de>,
    ty: &Type,
) -> Result<ScalarValue<'de>, DecodeError> {
    match consume(parser)?.kind {
        ParseEventKind::Scalar(value) => Ok(value),
        other => Err(DecodeError::new(format!(
            "expected {}, found {}",
            ty.name(),
            event_label(&other)
        ))),
    }
}

fn is_null<'de>(parser: &mut dyn FormatParser<'de>) -> Result<bool, DecodeError> {
    Ok(matches!(
        peek_kind(parser)?,
        Some(ParseEventKind::Scalar(ScalarValue::Null | ScalarValue::Unit))
    ))
}

fn consume<'de>(
    parser: &mut dyn FormatParser<'de>,
) -> Result<facet_format::ParseEvent<'de>, DecodeError> {
    match parser.next_event() {
        Ok(Some(event)) => Ok(event),
        Ok(None) => Err(DecodeError::new("unexpected end of document")),
        Err(err) => Err(DecodeError::new(format!("parse error: {err:?}"))),
    }
}

fn peek_kind<'de>(
    parser: &mut dyn FormatParser<'de>,
) -> Result<Option<ParseEventKind<'de>>, DecodeError> {
    match parser.peek_event() {
        Ok(Some(event)) => Ok(Some(event.kind)),
        Ok(None) => Ok(None),
        Err(err) => Err(DecodeError::new(format!("parse error: {err:?}"))),
    }
}

fn expect_end<'de>(
    parser: &mut dyn FormatParser<'de>,
    format: DecodeFormat,
) -> Result<(), DecodeError> {
    match peek_kind(parser)? {
        None => Ok(()),
        Some(other) => Err(DecodeError::new(format!(
            "unexpected trailing {} after the decoded value: {}",
            format.label(),
            event_label(&other)
        ))),
    }
}

fn field_name(key: &FieldKey<'_>) -> String {
    match key {
        FieldKey::Name(name) => name.as_ref().to_owned(),
        FieldKey::Full(full) => full
            .name
            .as_ref()
            .map(|name| name.as_ref().to_owned())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn field_context(name: &str, err: DecodeError) -> DecodeError {
    DecodeError::new(format!("field \"{name}\": {}", err.message))
}

fn scalar_mismatch(expected: &str, found: &ScalarValue<'_>) -> DecodeError {
    DecodeError::new(format!(
        "expected {expected}, found {}",
        scalar_label(found)
    ))
}

fn scalar_label(value: &ScalarValue<'_>) -> &'static str {
    match value {
        ScalarValue::Unit | ScalarValue::Null => "null",
        ScalarValue::Bool(_) => "a boolean",
        ScalarValue::Char(_) => "a character",
        ScalarValue::I64(_) | ScalarValue::U64(_) | ScalarValue::I128(_) | ScalarValue::U128(_) => {
            "an integer"
        }
        ScalarValue::F64(_) => "a float",
        ScalarValue::Str(_) => "a string",
        ScalarValue::Bytes(_) => "bytes",
        _ => "a value",
    }
}

fn event_label(kind: &ParseEventKind<'_>) -> &'static str {
    match kind {
        ParseEventKind::StructStart(_) => "an object",
        ParseEventKind::StructEnd => "an object end",
        ParseEventKind::FieldKey(_) => "a field key",
        ParseEventKind::OrderedField => "an ordered field",
        ParseEventKind::SequenceStart(_) => "an array",
        ParseEventKind::SequenceEnd => "an array end",
        ParseEventKind::Scalar(value) => scalar_label(value),
        ParseEventKind::VariantTag(_) => "a variant tag",
        _ => "a value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vir::{EnumType, EnumVariant, RecordField, RecordType, Type, VariantPayload};

    fn dep_spec() -> Type {
        Type::Enum(EnumType {
            name: "DepSpec".to_owned(),
            variants: vec![
                EnumVariant {
                    name: "Req".to_owned(),
                    payload: VariantPayload::Tuple(vec![Type::String]),
                },
                EnumVariant {
                    name: "Detailed".to_owned(),
                    payload: VariantPayload::Record(vec![
                        RecordField {
                            name: "version".to_owned(),
                            ty: Type::option(Type::String),
                        },
                        RecordField {
                            name: "optional".to_owned(),
                            ty: Type::Bool,
                        },
                    ]),
                },
            ],
        })
    }

    #[test]
    fn decode_enum_string_form() {
        let value = decode(DecodeFormat::Json, "\"^1.2\"", &dep_spec())
            .expect("a scalar string selects the short tuple variant");
        assert_eq!(
            value,
            DecodedValue::Variant {
                index: 0,
                fields: vec![DecodedValue::Str("^1.2".to_owned())],
            }
        );
    }

    #[test]
    fn decode_enum_table_form() {
        let value = decode(
            DecodeFormat::Json,
            "{\"version\":\"^2.0\",\"optional\":true}",
            &dep_spec(),
        )
        .expect("an object selects the detailed record variant");
        assert_eq!(
            value,
            DecodedValue::Variant {
                index: 1,
                fields: vec![
                    DecodedValue::OptionSome(Box::new(DecodedValue::Str("^2.0".to_owned()))),
                    DecodedValue::Bool(true),
                ],
            }
        );
    }

    fn pkg_row() -> Type {
        Type::Record(RecordType {
            name: "PkgRow".to_owned(),
            fields: vec![
                RecordField { name: "name".to_owned(), ty: Type::String },
                RecordField { name: "vers".to_owned(), ty: Type::String },
                RecordField { name: "yanked".to_owned(), ty: Type::Bool },
            ],
        })
    }

    #[test]
    fn decode_json_struct() {
        let value = decode(
            DecodeFormat::Json,
            "{\"name\":\"mio\",\"vers\":\"0.8.11\",\"yanked\":false}",
            &pkg_row(),
        )
        .expect("json decodes onto PkgRow");
        assert_eq!(
            value,
            DecodedValue::Record(vec![
                DecodedValue::Str("mio".to_owned()),
                DecodedValue::Str("0.8.11".to_owned()),
                DecodedValue::Bool(false),
            ])
        );
    }

    #[test]
    fn decode_toml_nested_struct() {
        let manifest = Type::Record(RecordType {
            name: "Manifest".to_owned(),
            fields: vec![RecordField {
                name: "package".to_owned(),
                ty: Type::Record(RecordType {
                    name: "Package".to_owned(),
                    fields: vec![
                        RecordField { name: "name".to_owned(), ty: Type::String },
                        RecordField { name: "version".to_owned(), ty: Type::String },
                    ],
                }),
            }],
        });
        let value = decode(
            DecodeFormat::Toml,
            "[package]\nname = \"taxon\"\nversion = \"0.1.0\"\n",
            &manifest,
        )
        .expect("toml decodes onto Manifest");
        assert_eq!(
            value,
            DecodedValue::Record(vec![DecodedValue::Record(vec![
                DecodedValue::Str("taxon".to_owned()),
                DecodedValue::Str("0.1.0".to_owned()),
            ])])
        );
    }

    #[test]
    fn decode_optional_fields() {
        let dep_decl = Type::Record(RecordType {
            name: "DepDecl".to_owned(),
            fields: vec![
                RecordField { name: "version".to_owned(), ty: Type::option(Type::String) },
                RecordField { name: "path".to_owned(), ty: Type::option(Type::String) },
            ],
        });
        let value = decode(DecodeFormat::Json, "{\"version\":\"^1.0\"}", &dep_decl)
            .expect("absent option field decodes to None");
        assert_eq!(
            value,
            DecodedValue::Record(vec![
                DecodedValue::OptionSome(Box::new(DecodedValue::Str("^1.0".to_owned()))),
                DecodedValue::OptionNone,
            ])
        );
    }

    #[test]
    fn decode_failure_names_the_field() {
        let err = decode(DecodeFormat::Json, "{\"name\": 42}", &pkg_row())
            .expect_err("an integer where a string is expected fails");
        assert!(err.message.contains("name"), "message = {}", err.message);
    }
}
