# Facet Format Codex (current)

This is the **single, up-to-date** design+status document for the `facet-format*` effort.

It is intended to replace the split drafts:
- `drafts/facet-format-codex-serialize.md`
- `drafts/facet-format-codex-deserialize.md`

Those files still contain useful long-form rationale and “end-state” sketches, but they may not match the code in this branch.

## Goals

- **One shared core** for cross-format semantics: flatten, enum tagging, rename rules, solver hooks.
- **Thin per-format adapters**: each format crate implements just the format surface (JSON punctuation, XML tag mapping, etc.).
- A **conformance suite** that every format backend can run (`facet-format-suite`).

## What exists today (code status)

Crates in this branch:
- `facet-format`: shared core (currently contains both deserialization and serialization traversal).
- `facet-format-suite`: canonical cross-format fixtures + harness (now supports optional round-trip).
- `facet-format-json`: JSON parser adapter + minimal JSON serializer for round-trip.
- `facet-format-xml`: XML parser adapter + minimal XML serializer for round-trip.

Current limitations (important for reviewers):
- Deserialization in `facet-format` currently builds an intermediate `facet_value::Value` from `ParseEvent`s and then uses `facet_value::from_value`.
  - This is **not** yet the “evidence + solver + Partial” end-state described in older drafts.
- Parser “probing” (`begin_probe`) is not yet implemented for XML and is limited for JSON.
- The serializer core supports common enum tagging modes (external/internal/adjacent/untagged), but it is **not** yet the “SynthesizedStruct/FieldLocation” API described in older drafts.

## Conformance suite

`facet-format-suite` defines canonical cases (structs, sequences, enums).

Each format implements:
- `FormatSuite::deserialize` (required)
- `FormatSuite::serialize` (optional)

If `serialize` is implemented, the suite performs:

`deserialize(input) -> value -> serialize(value) -> deserialize(serialized) -> assert_same!(roundtrip, value)`

Formats can opt out per-case via `CaseSpec::without_roundtrip(reason)` (useful while a backend lacks coverage).

## Current core interfaces

### Deserialization

`facet-format` defines:
- `FormatParser<'de>`: produces a format-agnostic `ParseEvent<'de>` stream and supports `skip_value` and `begin_probe` (WIP).
- `FormatDeserializer<P>`: consumes `ParseEvent`s and produces a typed value via `facet_value::Value`.

See:
- `facet-format/src/parser.rs`
- `facet-format/src/event.rs`
- `facet-format/src/deserializer.rs`

### Serialization

`facet-format` defines:
- `FormatSerializer`: a low-level “writer” interface (`begin_struct`, `field_key`, `begin_seq`, `scalar`, …).
- `serialize_root`: shared traversal from `facet_reflect::Peek` into `FormatSerializer`.

See:
- `facet-format/src/serializer.rs`

This interface is intentionally minimal to unblock round-trip tests across multiple formats; the older “SynthesizedStruct” plan remains a potential next iteration.

## XML mapping (current)

The `facet-format-xml` parser currently maps XML into a value model roughly like:
- elements with uniform repeated children → `SequenceStart/End`
- attributes → object fields with `FieldLocationHint::Attribute`
- child elements → object fields with `FieldLocationHint::Child`
- text content → either a scalar or an `_text` field (when mixed with children/attrs)

The current XML serializer is designed specifically to **round-trip through the above parser**:
- root wrapper element `<root>…</root>` is used (root element name is ignored by the parser)
- struct fields become child elements `<field>…</field>`
- sequences become repeated `<item>…</item>`
- scalars are emitted as text; null uses literal `null` (so it round-trips through `parse_scalar`)

## Recommended doc layout for merge review

If we want to move out of `drafts/`, the “merge-ready” version of this document should probably live under `docs/` and contain:
- a short “Status / Not yet implemented” section (to prevent reviewers expecting the full end-state)
- pointers to the old long-form drafts for rationale
- links to the suite and the current core traits
