+++
title = "Format crates comparison"
+++

This document tracks feature parity across all facet format crates.

Legend:
- ✅ = Fully supported with tests
- 🟡 = Partial support or untested
- 🚫 = Not supported
- ➖ = Not applicable to this format

Note: `msgp` = `facet-msgpack` (shortened for column width)

## Overview

Note: S = Serialization, D = Deserialization

| Crate | Direction | Format Type | Parser | Showcase |
|-------|-----------|-------------|--------|----------|
| [facet-json](https://docs.rs/facet-json) | SD | Text | Event-based (custom) | [View](/learn/showcases/json) |
| [facet-kdl](https://docs.rs/facet-kdl) | SD | Text (node-based) | DOM ([kdl-rs](https://docs.rs/kdl)) | [View](/learn/showcases/kdl) |
| [facet-yaml](https://docs.rs/facet-yaml) | SD | Text | Event-based ([saphyr](https://docs.rs/saphyr)) | [View](/learn/showcases/yaml) |
| [facet-toml](https://docs.rs/facet-toml) | SD | Text | DOM ([toml_edit](https://docs.rs/toml_edit)) | 🚫 |
| [facet-msgpack](https://docs.rs/facet-msgpack) | SD | Binary | Event-based (custom) | 🚫 |
| [facet-asn1](https://docs.rs/facet-asn1) | S | Binary | (Custom) | 🚫 |
| [facet-xdr](https://docs.rs/facet-xdr) | S | Binary | (Custom) | 🚫 |
| [facet-args](https://docs.rs/facet-args) | D | CLI | (Custom) | 🚫 |
| [facet-urlencoded](https://docs.rs/facet-urlencoded) | D | Text | (Custom) | 🚫 |
| [facet-csv](https://docs.rs/facet-csv) | S | Text | (Custom) | 🚫 |

## Scalar Types

| Type | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|-----|------|--------|-----|
| `bool` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `u8..u64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `i8..i64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `u128/i128` | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| `f32/f64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `char` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | ✅ |
| NonZero integers | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🟡 | 🟡 | 🟡 |

## String Types

All formats support `String`, `&str` (with best-effort borrowing), and `Cow<str>`.

## Lists, Sets, and Maps

| Type | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|-----|------|--------|-----|
| `Vec<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 |
| `[T; N]` (arrays) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🟡 |
| `HashSet<T>` | ✅ | ✅ | ✅ | 🟡 | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `BTreeSet<T>` | ✅ | ✅ | ✅ | 🟡 | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `HashMap<K, V>` | ✅ | 🟡 | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `BTreeMap<K, V>` | ✅ | 🟡 | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| Non-string map keys | ✅ | 🚫 | ✅ | 🚫 | ✅ | 🚫 | 🚫 | ➖ | ➖ | ➖ |

## Compound Types

| Type | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|-----|------|--------|-----|
| `Option<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | 🟡 |
| `Result<T, E>` | ✅ | 🟡 | 🟡 | 🟡 | ✅ | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |

## Smart Pointers

| Type | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|-----|------|--------|-----|
| `Box<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 |
| `Rc<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `Arc<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `Arc<str>` | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| `Arc<[T]>` | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |

## External Types

| Type | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|-----|------|--------|-----|
| [`chrono`](https://docs.rs/chrono) | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| [`time`](https://docs.rs/time) | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| [`jiff`](https://docs.rs/jiff) | ✅ | 🟡 | 🟡 | 🚫 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| [`uuid`](https://docs.rs/uuid) | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| [`ulid`](https://docs.rs/ulid) | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| [`camino`](https://docs.rs/camino) | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| [`ordered-float`](https://docs.rs/ordered-float) | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| [`bytes`](https://docs.rs/bytes) | ✅ | 🟡 | 🟡 | 🟡 | ✅ | ✅ | ✅ | 🚫 | 🚫 | 🚫 |

## Struct Types

| Type | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|-----|------|--------|-----|
| Named structs | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Tuple structs | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ |
| Unit structs | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 |

## Enum Representations

### Tagging Strategies

| Representation | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|----------------|------|-----|------|------|------|------|-----|------|--------|-----|
| Externally tagged (default) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ➖ |
| Internally tagged (`tag=`) | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | ➖ | ➖ | ➖ |
| Adjacently tagged (`tag+content`) | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | ➖ | ➖ | ➖ |
| Untagged | ✅ | 🟡 | ✅ | 🚫 | 🟡 | 🚫 | 🚫 | ➖ | ➖ | ➖ |

**Examples (JSON):**
```json
// Externally tagged: { "Variant": "value" }
// Internally tagged: { "type": "Variant", "data": "value" }  
// Adjacently tagged: { "tag": "Variant", "content": "value" }
```

## Attributes

| Attribute | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|-----------|------|-----|------|------|------|------|-----|------|--------|-----|
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

## Diagnostics

| Feature | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|---------|------|-----|------|------|------|------|-----|------|--------|-----|
| `miette::Diagnostic` | ✅ | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | ✅ | 🚫 | 🚫 |
| `Spanned<T>` wrapper | ✅ | ✅ | ✅ | 🚫 | 🚫 | 🚫 | 🚫 | ✅ | 🚫 | 🚫 |
| Solver integration | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| "Did you mean?" suggestions | ✅ | ✅ | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |

## Advanced Features

| Feature | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|---------|------|-----|------|------|------|------|-----|------|--------|-----|
| Nested flatten | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| Multiple flattened enums | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |
| Value-based disambiguation | ✅ | ✅ | 🟡 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |

## no_std Support

| Feature | json | kdl | yaml | toml | msgp | asn1 | xdr | args | urlenc | csv |
|---------|------|-----|------|------|------|------|-----|------|--------|-----|
| `no_std` + `alloc` | ✅ | ✅ | ✅ (deser) | ✅ | 🟡 | ✅ | ✅ | 🟡 | 🟡 | 🟡 |
| Serialization | ✅ | ✅ | 🚫 (needs std) | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | ✅ |
| Deserialization | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | 🟡 | 🟡 | ➖ |
