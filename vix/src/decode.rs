//! Type-directed document decoding — the compile-time half of typed decode.
//!
//! One [`FormatParser`] pass per document, walked directly against the
//! compiler-known Vix target [`Type`]. No generic `Doc`/`Value` is
//! materialized and nothing walks an untyped tree: each parse event is
//! matched against the exact declared type it lands on, and the walk yields
//! typed leaves ([`DecodedValue`]) that the compiler lowers into ordinary
//! typed-construction VIR (records → `Op::Record`, strings → `Op::String`,
//! …). The constructed value therefore interns to the same content-addressed
//! handle a hand-written literal of that value would.
//!
//! This is the **constant-fold subset** of
//! `r[machine.primitive.typed-deserialization]`, restricted to compile-time-
//! constant document literals — not a realization of that runtime primitive.
//! The doctrine's doc-parse primitive is a *runtime* host call that serves
//! *dynamic* documents (a fetched index, a manifest read from disk) and returns
//! a runtime `Outcome`; this lane performs zero host calls and runs entirely in
//! `Compiler::compile`. Folding a pure, deterministic decode of a literal is a
//! legitimate as-if rewrite of that primitive's constant-input case, and the
//! zero-`HostCall` certificate on the lowered frames is an as-if optimization
//! proof — not evidence that the runtime primitive exists. When it lands, this
//! code must become the constant-folded case *of* it; nonliteral or
//! unknown-target decodes are rejected at a named runtime seam
//! ([`crate::diagnostic::DiagnosticCode::RuntimeDecodeUnavailable`]), never
//! host-evaluated here.

use facet_format::{FieldKey, FormatParser, ParseEventKind, ScalarValue};
use facet_json::JsonParser;
use facet_toml::TomlParser;

use crate::vir::{EnumType, RecordField, Type, VariantPayload};

pub use crate::vir::DecodeFormat;

impl DecodeFormat {
    fn label(self) -> &'static str {
        match self {
            DecodeFormat::Json => "JSON",
            DecodeFormat::Toml => "TOML",
        }
    }
}

/// A byte span inside the *decoded document* (the parser's own coordinate
/// space), not the vix source. The document reaches the decoder as a string
/// literal whose escapes are never reversed, so mapping this offset back into
/// the vix source through those escapes is not available; the boundary is
/// represented explicitly here rather than fabricating a source offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentSpan {
    pub offset: u32,
    pub len: u32,
}

/// The document span carried by one parse event. Reads the event's span fields
/// directly so the (transitive) `facet_reflect::Span` type need not be named.
fn event_span(event: &facet_format::ParseEvent<'_>) -> DocumentSpan {
    DocumentSpan {
        offset: event.span.offset,
        len: event.span.len,
    }
}

/// One step of a structured field path into the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathSegment {
    Field(String),
}

/// A typed decode failure kind. No prose is identity-bearing: [`label`] is the
/// stable machine discriminant and [`render`] is a human convenience.
///
/// [`label`]: DecodeErrorKind::label
/// [`render`]: DecodeErrorKind::render
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeErrorKind {
    ExpectedScalar {
        expected: String,
        found: &'static str,
    },
    ExpectedObject {
        container: String,
        found: &'static str,
    },
    ExpectedStringOrObject {
        enum_name: String,
        found: &'static str,
    },
    IntOutOfRange,
    MissingField {
        container: String,
        field: String,
    },
    UnknownField {
        container: String,
        field: String,
    },
    DuplicateField {
        container: String,
        field: String,
    },
    NoStringForm {
        enum_name: String,
    },
    NoTableForm {
        enum_name: String,
    },
    AmbiguousStringForm {
        enum_name: String,
    },
    AmbiguousTableForm {
        enum_name: String,
    },
    UnsupportedTarget {
        type_name: String,
    },
    TrailingContent {
        format: &'static str,
        found: &'static str,
    },
    UnexpectedEnd {
        container: String,
    },
    /// A leaf failure reported by the underlying parser. The parser only offers
    /// a rendered string here; it stays quarantined in `detail` and is never
    /// promoted to a discriminant.
    Parse {
        detail: String,
    },
}

impl DecodeErrorKind {
    /// A stable machine discriminant — never rendered prose.
    pub fn label(&self) -> &'static str {
        match self {
            DecodeErrorKind::ExpectedScalar { .. } => "expected-scalar",
            DecodeErrorKind::ExpectedObject { .. } => "expected-object",
            DecodeErrorKind::ExpectedStringOrObject { .. } => "expected-string-or-object",
            DecodeErrorKind::IntOutOfRange => "int-out-of-range",
            DecodeErrorKind::MissingField { .. } => "missing-field",
            DecodeErrorKind::UnknownField { .. } => "unknown-field",
            DecodeErrorKind::DuplicateField { .. } => "duplicate-field",
            DecodeErrorKind::NoStringForm { .. } => "no-string-form",
            DecodeErrorKind::NoTableForm { .. } => "no-table-form",
            DecodeErrorKind::AmbiguousStringForm { .. } => "ambiguous-string-form",
            DecodeErrorKind::AmbiguousTableForm { .. } => "ambiguous-table-form",
            DecodeErrorKind::UnsupportedTarget { .. } => "unsupported-target",
            DecodeErrorKind::TrailingContent { .. } => "trailing-content",
            DecodeErrorKind::UnexpectedEnd { .. } => "unexpected-end",
            DecodeErrorKind::Parse { .. } => "parse-error",
        }
    }

    /// A human-readable rendering. Convenience only; identity lives in the kind.
    pub fn render(&self) -> String {
        match self {
            DecodeErrorKind::ExpectedScalar { expected, found } => {
                format!("expected {expected}, found {found}")
            }
            DecodeErrorKind::ExpectedObject { container, found } => {
                format!("expected an object for {container}, found {found}")
            }
            DecodeErrorKind::ExpectedStringOrObject { enum_name, found } => {
                format!("expected a string or object for {enum_name}, found {found}")
            }
            DecodeErrorKind::IntOutOfRange => {
                "expected Int, found an out-of-range integer".to_owned()
            }
            DecodeErrorKind::MissingField { container, field } => {
                format!("missing field \"{field}\" in {container}")
            }
            DecodeErrorKind::UnknownField { container, field } => {
                format!("unknown field \"{field}\" in {container}")
            }
            DecodeErrorKind::DuplicateField { container, field } => {
                format!("duplicate field \"{field}\" in {container}")
            }
            DecodeErrorKind::NoStringForm { enum_name } => {
                format!("{enum_name} has no short (single-string) form for a scalar document")
            }
            DecodeErrorKind::NoTableForm { enum_name } => {
                format!("{enum_name} has no detailed (table) form for an object document")
            }
            DecodeErrorKind::AmbiguousStringForm { enum_name } => format!(
                "{enum_name} has more than one short (single-string) form; a scalar document \
                 selects no variant unambiguously"
            ),
            DecodeErrorKind::AmbiguousTableForm { enum_name } => format!(
                "{enum_name} has more than one detailed (table) form; an object document selects \
                 no variant unambiguously"
            ),
            DecodeErrorKind::UnsupportedTarget { type_name } => {
                format!("cannot decode into {type_name}")
            }
            DecodeErrorKind::TrailingContent { format, found } => {
                format!("unexpected trailing {format} after the decoded value: {found}")
            }
            DecodeErrorKind::UnexpectedEnd { container } => {
                format!("expected a value for {container}, found end of document")
            }
            DecodeErrorKind::Parse { detail } => detail.clone(),
        }
    }
}

/// A typed decode failure: a kind, the structured field path that reached it,
/// and the offending document span when the parser offered one. This is the
/// compiler-internal error the constant-fold lane raises; it is rendered into a
/// [`crate::diagnostic::Diagnostic`] at the call site. No identity depends on a
/// prose string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub kind: DecodeErrorKind,
    pub path: Vec<PathSegment>,
    pub span: Option<DocumentSpan>,
}

impl DecodeError {
    fn of(kind: DecodeErrorKind) -> Self {
        Self {
            kind,
            path: Vec::new(),
            span: None,
        }
    }

    fn with_span(mut self, span: DocumentSpan) -> Self {
        if self.span.is_none() {
            self.span = Some(span);
        }
        self
    }

    /// Prepend one field as the error unwinds, so the path reads outer→inner.
    fn under(mut self, field: &str) -> Self {
        self.path.insert(0, PathSegment::Field(field.to_owned()));
        self
    }

    /// The field-path names, outer→inner.
    #[must_use]
    pub fn path_names(&self) -> Vec<String> {
        self.path
            .iter()
            .map(|PathSegment::Field(name)| name.clone())
            .collect()
    }

    /// A human rendering: the kind's message with no path prefix (the path is
    /// carried separately and joined by the diagnostic renderer).
    #[must_use]
    pub fn render(&self) -> String {
        self.kind.render()
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
            let mut parser = TomlParser::new(source).map_err(|err| {
                DecodeError::of(DecodeErrorKind::Parse {
                    detail: format!("TOML parse error: {err:?}"),
                })
            })?;
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
        Type::Int => {
            let (value, span) = scalar(parser, "Int")?;
            match value {
                ScalarValue::I64(value) => Ok(DecodedValue::Int(value)),
                ScalarValue::U64(value) => i64::try_from(value)
                    .map(DecodedValue::Int)
                    .map_err(|_| DecodeError::of(DecodeErrorKind::IntOutOfRange).with_span(span)),
                other => Err(scalar_mismatch("Int", &other, span)),
            }
        }
        Type::Bool => {
            let (value, span) = scalar(parser, "Bool")?;
            match value {
                ScalarValue::Bool(value) => Ok(DecodedValue::Bool(value)),
                other => Err(scalar_mismatch("Bool", &other, span)),
            }
        }
        Type::String => {
            let (value, span) = scalar(parser, "String")?;
            match value {
                ScalarValue::Str(value) => Ok(DecodedValue::Str(value.into_owned())),
                other => Err(scalar_mismatch("String", &other, span)),
            }
        }
        Type::Record(record) => Ok(DecodedValue::Record(decode_fields(
            parser,
            &record.name,
            &record.fields,
        )?)),
        Type::Enum(enumeration) => decode_enum(parser, enumeration),
        other => Err(DecodeError::of(DecodeErrorKind::UnsupportedTarget {
            type_name: other.name(),
        })),
    }
}

/// The string-or-table enum form (the Cargo dependency shape). A scalar string
/// selects the short single-`String` tuple variant; an object selects the
/// detailed record variant and decodes directly into its fields. Selection is
/// deterministic: an ambiguous payload shape (two short forms, or two table
/// forms) is a typed failure, never a first-match guess.
fn decode_enum<'de>(
    parser: &mut dyn FormatParser<'de>,
    enumeration: &EnumType,
) -> Result<DecodedValue, DecodeError> {
    let (kind, span) = peek(parser)?;
    match kind {
        Some(ParseEventKind::Scalar(_)) => {
            let (index, inner) = string_form_variant(enumeration)?;
            let field = decode_value(parser, inner)?;
            Ok(DecodedValue::Variant {
                index,
                fields: vec![field],
            })
        }
        Some(ParseEventKind::StructStart(_)) => {
            let (index, fields) = table_form_variant(enumeration)?;
            let values = decode_fields(parser, &enumeration.name, fields)?;
            Ok(DecodedValue::Variant {
                index,
                fields: values,
            })
        }
        Some(other) => Err(span_opt(
            DecodeError::of(DecodeErrorKind::ExpectedStringOrObject {
                enum_name: enumeration.name.clone(),
                found: event_label(&other),
            }),
            span,
        )),
        None => Err(DecodeError::of(DecodeErrorKind::UnexpectedEnd {
            container: enumeration.name.clone(),
        })),
    }
}

/// Select the unique short-form variant (a single-`String` tuple payload).
/// Zero matches is [`DecodeErrorKind::NoStringForm`], more than one is
/// [`DecodeErrorKind::AmbiguousStringForm`] — never a first-match pick.
fn string_form_variant(enumeration: &EnumType) -> Result<(u32, &Type), DecodeError> {
    let mut matches = enumeration
        .variants
        .iter()
        .enumerate()
        .filter_map(|(index, variant)| match &variant.payload {
            VariantPayload::Tuple(types) => match types.as_slice() {
                [inner @ Type::String] => Some((index as u32, inner)),
                _ => None,
            },
            _ => None,
        });
    let first = matches.next().ok_or_else(|| {
        DecodeError::of(DecodeErrorKind::NoStringForm {
            enum_name: enumeration.name.clone(),
        })
    })?;
    if matches.next().is_some() {
        return Err(DecodeError::of(DecodeErrorKind::AmbiguousStringForm {
            enum_name: enumeration.name.clone(),
        }));
    }
    Ok(first)
}

/// Select the unique table-form variant (a record payload). Zero matches is
/// [`DecodeErrorKind::NoTableForm`], more than one is
/// [`DecodeErrorKind::AmbiguousTableForm`] — never a first-match pick.
fn table_form_variant(enumeration: &EnumType) -> Result<(u32, &[RecordField]), DecodeError> {
    let mut matches = enumeration
        .variants
        .iter()
        .enumerate()
        .filter_map(|(index, variant)| match &variant.payload {
            VariantPayload::Record(fields) => Some((index as u32, fields.as_slice())),
            _ => None,
        });
    let first = matches.next().ok_or_else(|| {
        DecodeError::of(DecodeErrorKind::NoTableForm {
            enum_name: enumeration.name.clone(),
        })
    })?;
    if matches.next().is_some() {
        return Err(DecodeError::of(DecodeErrorKind::AmbiguousTableForm {
            enum_name: enumeration.name.clone(),
        }));
    }
    Ok(first)
}

/// Decode one object's fields against a declared field list, in declaration
/// order. Shared by struct records and record-variant table forms: fields are
/// matched by name, absent `Option` fields resolve to `None`, and any other
/// absence or duplicate field is a typed failure. Unknown fields are consumed
/// and ignored so open documents such as Cargo manifests can decode into a
/// smaller typed projection. `container` names the struct or enum for error
/// attribution.
fn decode_fields<'de>(
    parser: &mut dyn FormatParser<'de>,
    container: &str,
    fields: &[RecordField],
) -> Result<Vec<DecodedValue>, DecodeError> {
    let start = consume(parser)?;
    let start_span = event_span(&start);
    match start.kind {
        ParseEventKind::StructStart(_) => {}
        other => {
            return Err(DecodeError::of(DecodeErrorKind::ExpectedObject {
                container: container.to_owned(),
                found: event_label(&other),
            })
            .with_span(start_span));
        }
    }

    let mut slots: Vec<Option<DecodedValue>> = fields.iter().map(|_| None).collect();
    loop {
        let (kind, span) = peek(parser)?;
        match kind {
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
                            return Err(span_opt(
                                DecodeError::of(DecodeErrorKind::DuplicateField {
                                    container: container.to_owned(),
                                    field: name,
                                }),
                                span,
                            ));
                        }
                        let value = decode_value(parser, &fields[index].ty)
                            .map_err(|err| err.under(&name))?;
                        slots[index] = Some(value);
                    }
                    None => {
                        let _ = (container, name, span);
                        skip_value(parser)?;
                    }
                }
            }
            Some(other) => {
                return Err(span_opt(
                    DecodeError::of(DecodeErrorKind::ExpectedObject {
                        container: container.to_owned(),
                        found: event_label(&other),
                    }),
                    span,
                ));
            }
        }
    }

    let mut out = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        match slots[index].take() {
            Some(value) => out.push(value),
            None if field.ty.option_inner().is_some() => out.push(DecodedValue::OptionNone),
            None => {
                return Err(DecodeError::of(DecodeErrorKind::MissingField {
                    container: container.to_owned(),
                    field: field.name.clone(),
                }));
            }
        }
    }
    Ok(out)
}

fn skip_value<'de>(parser: &mut dyn FormatParser<'de>) -> Result<(), DecodeError> {
    match consume(parser)?.kind {
        ParseEventKind::Scalar(_) => Ok(()),
        ParseEventKind::StructStart(_) => loop {
            match consume(parser)?.kind {
                ParseEventKind::StructEnd => return Ok(()),
                ParseEventKind::FieldKey(_) => skip_value(parser)?,
                other => {
                    return Err(DecodeError::of(DecodeErrorKind::ExpectedObject {
                        container: "unknown field".to_owned(),
                        found: event_label(&other),
                    }));
                }
            }
        },
        ParseEventKind::SequenceStart(_) => loop {
            match peek(parser)?.0 {
                Some(ParseEventKind::SequenceEnd) => {
                    consume(parser)?;
                    return Ok(());
                }
                Some(_) => skip_value(parser)?,
                None => {
                    return Err(DecodeError::of(DecodeErrorKind::UnexpectedEnd {
                        container: "unknown array".to_owned(),
                    }));
                }
            }
        },
        ParseEventKind::VariantTag(_) => skip_value(parser),
        other => Err(DecodeError::of(DecodeErrorKind::ExpectedObject {
            container: "unknown field".to_owned(),
            found: event_label(&other),
        })),
    }
}

/// Consume one scalar, returning its value and document span. `expected` names
/// the target type for the mismatch message.
fn scalar<'de>(
    parser: &mut dyn FormatParser<'de>,
    expected: &str,
) -> Result<(ScalarValue<'de>, DocumentSpan), DecodeError> {
    let event = consume(parser)?;
    let span = event_span(&event);
    match event.kind {
        ParseEventKind::Scalar(value) => Ok((value, span)),
        other => Err(DecodeError::of(DecodeErrorKind::ExpectedScalar {
            expected: expected.to_owned(),
            found: event_label(&other),
        })
        .with_span(span)),
    }
}

fn is_null<'de>(parser: &mut dyn FormatParser<'de>) -> Result<bool, DecodeError> {
    Ok(matches!(
        peek(parser)?.0,
        Some(ParseEventKind::Scalar(
            ScalarValue::Null | ScalarValue::Unit
        ))
    ))
}

fn consume<'de>(
    parser: &mut dyn FormatParser<'de>,
) -> Result<facet_format::ParseEvent<'de>, DecodeError> {
    match parser.next_event() {
        Ok(Some(event)) => Ok(event),
        Ok(None) => Err(DecodeError::of(DecodeErrorKind::UnexpectedEnd {
            container: "the document".to_owned(),
        })),
        Err(err) => Err(DecodeError::of(DecodeErrorKind::Parse {
            detail: format!("parse error: {err:?}"),
        })),
    }
}

/// Peek the next event's kind and document span without consuming it.
fn peek<'de>(
    parser: &mut dyn FormatParser<'de>,
) -> Result<(Option<ParseEventKind<'de>>, Option<DocumentSpan>), DecodeError> {
    match parser.peek_event() {
        Ok(Some(event)) => {
            let span = event_span(&event);
            Ok((Some(event.kind), Some(span)))
        }
        Ok(None) => Ok((None, None)),
        Err(err) => Err(DecodeError::of(DecodeErrorKind::Parse {
            detail: format!("parse error: {err:?}"),
        })),
    }
}

fn expect_end<'de>(
    parser: &mut dyn FormatParser<'de>,
    format: DecodeFormat,
) -> Result<(), DecodeError> {
    let (kind, span) = peek(parser)?;
    match kind {
        None => Ok(()),
        Some(other) => Err(span_opt(
            DecodeError::of(DecodeErrorKind::TrailingContent {
                format: format.label(),
                found: event_label(&other),
            }),
            span,
        )),
    }
}

fn span_opt(error: DecodeError, span: Option<DocumentSpan>) -> DecodeError {
    match span {
        Some(span) => error.with_span(span),
        None => error,
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

fn scalar_mismatch(expected: &str, found: &ScalarValue<'_>, span: DocumentSpan) -> DecodeError {
    DecodeError::of(DecodeErrorKind::ExpectedScalar {
        expected: expected.to_owned(),
        found: scalar_label(found),
    })
    .with_span(span)
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
                RecordField {
                    name: "name".to_owned(),
                    ty: Type::String,
                },
                RecordField {
                    name: "vers".to_owned(),
                    ty: Type::String,
                },
                RecordField {
                    name: "yanked".to_owned(),
                    ty: Type::Bool,
                },
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
                        RecordField {
                            name: "name".to_owned(),
                            ty: Type::String,
                        },
                        RecordField {
                            name: "version".to_owned(),
                            ty: Type::String,
                        },
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
                RecordField {
                    name: "version".to_owned(),
                    ty: Type::option(Type::String),
                },
                RecordField {
                    name: "path".to_owned(),
                    ty: Type::option(Type::String),
                },
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
    fn decode_failure_is_typed_with_a_field_path_and_span() {
        let err = decode(DecodeFormat::Json, "{\"name\": 42}", &pkg_row())
            .expect_err("an integer where a string is expected fails");
        // Typed kind, not a prose match.
        assert_eq!(
            err.kind,
            DecodeErrorKind::ExpectedScalar {
                expected: "String".to_owned(),
                found: "an integer",
            }
        );
        // Structured field path names the offending field.
        assert_eq!(err.path_names(), vec!["name".to_owned()]);
        // The document byte span of the offending value (`42`) is preserved.
        let span = err.span.expect("the offending document span is preserved");
        assert_eq!(
            &"{\"name\": 42}"[span.offset as usize..(span.offset + span.len) as usize],
            "42"
        );
        assert_eq!(err.kind.label(), "expected-scalar");
    }

    #[test]
    fn ambiguous_enum_forms_are_rejected_not_first_matched() {
        // Two short (single-String tuple) forms: a scalar document is ambiguous.
        let two_short = Type::Enum(EnumType {
            name: "TwoShort".to_owned(),
            variants: vec![
                EnumVariant {
                    name: "A".to_owned(),
                    payload: VariantPayload::Tuple(vec![Type::String]),
                },
                EnumVariant {
                    name: "B".to_owned(),
                    payload: VariantPayload::Tuple(vec![Type::String]),
                },
            ],
        });
        let err = decode(DecodeFormat::Json, "\"x\"", &two_short)
            .expect_err("two short forms are ambiguous");
        assert_eq!(
            err.kind,
            DecodeErrorKind::AmbiguousStringForm {
                enum_name: "TwoShort".to_owned(),
            }
        );

        // Two table (record) forms: an object document is ambiguous.
        let two_table = Type::Enum(EnumType {
            name: "TwoTable".to_owned(),
            variants: vec![
                EnumVariant {
                    name: "A".to_owned(),
                    payload: VariantPayload::Record(vec![RecordField {
                        name: "x".to_owned(),
                        ty: Type::Bool,
                    }]),
                },
                EnumVariant {
                    name: "B".to_owned(),
                    payload: VariantPayload::Record(vec![RecordField {
                        name: "y".to_owned(),
                        ty: Type::Bool,
                    }]),
                },
            ],
        });
        let err = decode(DecodeFormat::Json, "{\"x\":true}", &two_table)
            .expect_err("two table forms are ambiguous");
        assert_eq!(
            err.kind,
            DecodeErrorKind::AmbiguousTableForm {
                enum_name: "TwoTable".to_owned(),
            }
        );
    }
}
