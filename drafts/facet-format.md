# Facet Format Codex (current)

This is the **single, up-to-date** design+status document for the `facet-format*` effort.

It is intended to replace the split drafts:
- `drafts/facet-format-codex-serialize.md`
- `drafts/facet-format-codex-deserialize.md`

Those files still contain useful long-form rationale and "end-state" sketches, but they may not match the code in this branch.

## Goals

- **One shared core** for cross-format semantics: flatten, enum tagging, rename rules, solver hooks.
- **Thin per-format adapters**: each format crate implements just the format surface (JSON punctuation, XML tag mapping, etc.).
- A **conformance suite** that every format backend can run (`facet-format-suite`).

## What exists today (code status)

Crates in this branch:
- `facet-format`: shared core (deserialization and serialization traversal).
- `facet-format-suite`: canonical cross-format fixtures + harness (supports optional round-trip).
- `facet-format-json`: JSON parser adapter + JSON serializer for round-trip.
- `facet-format-xml`: XML parser adapter + XML serializer for round-trip.

### Feature status

| Feature | Status | Notes |
|---------|--------|-------|
| Direct Partial deserialization | ✅ | `ParseEvent` → `Partial<T>` → `HeapValue<T>` → `T` (no intermediate Value) |
| Parser probing (JSON slice) | ✅ | `begin_probe` returns real `FieldEvidence` |
| Parser probing (JSON streaming) | ✅ | Buffers events during probing and replays them |
| Parser probing (XML) | ✅ | Scans ahead through pre-parsed events |
| `FieldLocationHint` | ✅ | Full enum: KeyValue, Attribute, Text, Child, Property, Argument |
| `BORROW` const generic | ✅ | `from_str`/`from_slice` borrow via `Facet<'de>` |
| `capture_raw` in parser trait | ✅ | JSON implements it |
| RawJson end-to-end | ❌ | Deserializer doesn't call `capture_raw` yet |
| Streaming (std/tokio/futures-io) | ✅ | Full support in facet-format-json |
| Enum tagging modes | ✅ | external/internal/adjacent/untagged |
| Flatten with defaults | ✅ | Fixed in issue 1297 |

### Not in scope (facet-json specific)

- Cranelift JIT compilation
- Axum integration
- These remain in `facet-json` as format-specific optimizations

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
- `FormatParser<'de>`: produces a format-agnostic `ParseEvent<'de>` stream, supports `skip_value`, `begin_probe`, and `capture_raw`.
- `FormatDeserializer<'input, BORROW, P>`: consumes `ParseEvent`s and deserializes directly into `Partial<T>`.

See:
- `facet-format/src/parser.rs`
- `facet-format/src/event.rs`
- `facet-format/src/deserializer.rs`

### Serialization

`facet-format` defines:
- `FormatSerializer`: a low-level "writer" interface (`begin_struct`, `field_key`, `begin_seq`, `scalar`, …).
- `serialize_root`: shared traversal from `facet_reflect::Peek` into `FormatSerializer`.

See:
- `facet-format/src/serializer.rs`

This interface is intentionally minimal to unblock round-trip tests across multiple formats; the older "SynthesizedStruct" plan remains a potential next iteration.

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

## Remaining work for production readiness

1. **Wire up RawJson**: Deserializer needs to call `capture_raw` for types that want raw JSON capture

### Future polish (not blocking)

- **SynthesizedStruct API**: Refactor serializer from event-based to declarative struct layout. Current event-based API works fine, and field reordering (attributes before elements) is already handled via `FieldOrdering::AttributesFirst` + `sort_fields_if_needed`.

## Recommended doc layout for merge review

If we want to move out of `drafts/`, the "merge-ready" version of this document should probably live under `docs/` and contain:
- a short "Status / Not yet implemented" section (to prevent reviewers expecting the full end-state)
- pointers to the old long-form drafts for rationale
- links to the suite and the current core traits
