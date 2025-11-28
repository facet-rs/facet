# Format Crate Feature Matrix

This document tracks feature parity across all facet format crates. Use it to identify gaps and prioritize work.

Legend:
- ✅ = Fully supported with tests
- 🟡 = Partial support or untested
- 🚫 = Not supported
- ➖ = Not applicable to this format

## Overview

| Crate | Direction | Format Type | Parser | Showcase | Error Showcase |
|-------|-----------|-------------|--------|----------|----------------|
| facet-json | ser + deser | Text | Event-based | ✅ | ✅ |
| facet-kdl | ser + deser | Text (node-based) | DOM (kdl-rs) | ✅ | ✅ |
| facet-yaml | ser + deser | Text | Event-based (saphyr) | ✅ | ✅ |
| facet-toml | ser + deser | Text | DOM (toml_edit) | 🚫 | 🚫 |
| facet-msgpack | ser + deser | Binary | Event-based | 🚫 | 🚫 |
| facet-asn1 | ser only | Binary | ➖ | 🚫 | 🚫 |
| facet-xdr | ser only | Binary | ➖ | 🚫 | 🚫 |
| | | | | | |
| facet-args | deser only | CLI | Custom | 🚫 | 🚫 |
| facet-urlencoded | deser only | Text | Custom | 🚫 | 🚫 |
| facet-csv | ser only | Text | ➖ | 🚫 | 🚫 |

## API Surface

| Feature | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|---------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `from_str` | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ➖ | ✅ | ✅ | ➖ |
| `from_slice` | ➖ | ➖ | ➖ | ➖ | ✅ | ➖ | ➖ | ✅ | ➖ | ➖ |
| `to_string` | ✅ | ✅ | ✅ | ✅ | ➖ | 🚫 | 🚫 | ➖ | ➖ | ✅ |
| `to_vec` | ➖ | ➖ | ➖ | ➖ | ✅ | ✅ | ✅ | ➖ | ➖ | ➖ |
| `to_writer` | ✅ | ✅ | ✅ | 🚫 | ✅ | 🚫 | 🚫 | ➖ | ➖ | ✅ |
| `to_string_pretty` | ✅ | 🚫 | 🚫 | 🚫 | ➖ | ➖ | ➖ | ➖ | ➖ | ➖ |
| `miette::Diagnostic` | ✅ | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | ✅ | 🚫 | 🚫 |

## Scalar Types

| Type | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `bool` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `u8..u64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `i8..i64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `u128/i128` | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| `f32/f64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `char` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | ✅ |
| NonZero integers | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🟡 | 🟡 | 🟡 |

## String Types

| Type | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `String` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

### Zero-Copy / Borrowing

These types attempt to borrow from the input when possible (e.g., unescaped strings), falling back to allocation when necessary.

| Type | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `&str` (best-effort borrow) | ✅ | ✅ | ✅ | 🟡 | 🚫 | ➖ | ➖ | ✅ | ✅ | 🚫 |
| `Cow<str>` (borrow or own) | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |

## Compound Types

| Type | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `Option<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | 🟡 |
| `Result<T, E>` | ✅ | 🟡 | 🟡 | 🟡 | ✅ | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `Vec<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 |
| `[T; N]` (arrays) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🟡 |
| `HashMap<K, V>` | ✅ | 🟡 | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `BTreeMap<K, V>` | ✅ | 🟡 | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `HashSet<T>` | ✅ | ✅ | ✅ | 🟡 | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `BTreeSet<T>` | ✅ | ✅ | ✅ | 🟡 | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| Non-string map keys | ✅ | 🚫 | ✅ | 🚫 | ✅ | 🚫 | 🚫 | ➖ | ➖ | ➖ |

## Smart Pointers

| Type | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `Box<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 |
| `Rc<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `Arc<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |

## Struct Types

| Type | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|---------|------|-----|------|--------|-----|
| Named structs | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Tuple structs | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ |
| Unit structs | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 |

## Enum Representations

| Representation | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|----------------|------|-----|------|------|---------|------|-----|------|--------|-----|
| Externally tagged (default) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ➖ |
| Internally tagged (`tag=`) | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | ➖ | ➖ | ➖ |
| Adjacently tagged (`tag+content`) | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | ➖ | ➖ | ➖ |
| Untagged | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | ➖ | ➖ | ➖ |
| Unit variants | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ➖ |
| Newtype variants | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ➖ |
| Tuple variants | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ➖ |
| Struct variants | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ➖ |

## Attributes

| Attribute | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|-----------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `rename` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | 🟡 |
| `rename_all` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | 🟡 | 🟡 |
| `default` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | 🟡 |
| `skip_serializing` | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | ➖ | ➖ | 🟡 |
| `skip_deserializing` | ✅ | ✅ | ✅ | 🟡 | 🟡 | ➖ | ➖ | 🟡 | 🟡 | ➖ |
| `skip_serializing_if` | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | ➖ | ➖ | 🟡 |
| `transparent` | ✅ | ✅ | ✅ | 🟡 | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| `flatten` | ✅ | ✅ | ✅ | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `deny_unknown_fields` | ✅ | ✅ | 🟡 | 🟡 | ✅ | ➖ | ➖ | 🚫 | ✅ | ➖ |
| `deserialize_with` | ✅ | ✅ | 🟡 | 🟡 | 🟡 | ➖ | ➖ | 🟡 | 🟡 | ➖ |
| `serialize_with` | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | ➖ | ➖ | 🟡 |
| `type_tag` (KDL-specific) | ➖ | ✅ | ➖ | ➖ | ➖ | ➖ | ➖ | ➖ | ➖ | ➖ |

## Format-Specific Attributes

### KDL

| Attribute | Support |
|-----------|---------|
| `child` | ✅ |
| `children` | ✅ |
| `argument` | ✅ |
| `property` | ✅ |

### Args

| Attribute | Support |
|-----------|---------|
| `positional` | ✅ |
| `named` | ✅ |
| `short` | ✅ |

## Advanced Features

| Feature | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|---------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `Spanned<T>` wrapper | ✅ | ✅ | ✅ | 🚫 | 🚫 | 🚫 | 🚫 | ✅ | 🚫 | 🚫 |
| Solver integration | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| Nested flatten | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| Multiple flattened enums | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| Value-based disambiguation | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| "Did you mean?" suggestions | ✅ | ✅ | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |

## External Type Support

| Crate | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|-------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `chrono` | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `time` | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `jiff` | ✅ | 🟡 | 🟡 | 🚫 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `uuid` | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `ulid` | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `camino` | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `ordered-float` | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `bytes` | ✅ | 🟡 | 🟡 | 🟡 | ✅ | ✅ | ✅ | 🚫 | 🚫 | 🚫 |

## no_std Support

| Feature | json | kdl | yaml | toml | msgpack | asn1 | xdr | args | urlenc | csv |
|---------|------|-----|------|------|---------|------|-----|------|--------|-----|
| `no_std` + `alloc` | ✅ | ✅ | ✅ (deser) | ✅ | 🟡 | ✅ | ✅ | 🟡 | 🟡 | 🟡 |
| Serialization | ✅ | ✅ | 🚫 (needs std) | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ✅ |
| Deserialization | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | 🟡 | 🟡 | ➖ |

## Test Coverage Summary

| Crate | Test Files | Key Test Areas |
|-------|------------|----------------|
| facet-json | 35+ | enums, flatten, spans, chrono, uuid, bytes, skip, deny_unknown |
| facet-kdl | 12+ | flatten (extensive), enums, type_annotations, spanned, diagnostics, solver |
| facet-yaml | 15+ | datetime, maps, lists, transparent, enums (all repr) |
| facet-toml | 20+ | enums, vec_of_tables, options, scalars, maps |
| facet-msgpack | 12+ | primitives, enums, structs, tuples, deny_unknown |
| facet-asn1 | 1 | ASN.1 encoding |
| facet-xdr | 1 | XDR encoding |
| facet-args | 4 | simple, sequence, errors, subspans |
| facet-urlencoded | 1 | nested bracket notation |
| facet-csv | 1 | basic struct serialization |

## Notes

### Solver Integration

The `facet-solver` crate handles flattened enum disambiguation. Currently integrated with:
- facet-json (full)
- facet-kdl (full, including nested child disambiguation)
- facet-yaml (partial)

Other crates would benefit from solver integration for flatten support.

### Binary Formats

Binary formats (msgpack, asn1, xdr) have fundamentally different constraints:
- No meaningful source spans
- Field ordering matters
- No "unknown fields" concept (extra bytes = error)
- Tag representations may not apply
