+++
title = "Format crates comparison"
weight = 2
+++

This document tracks feature parity across all facet format crates.

Legend:
- ✅ = Fully supported with tests
- 🟡 = Partial support or untested
- 🚫 = Not supported
- ➖ = Not applicable to this format

Note: `msgp` = `facet-msgpack`, `pcrd` = `facet-postcard` (shortened for column width)

## Overview

Note: S = Serialization, D = Deserialization

| Crate | Direction | Format Type | Parser | Showcase |
|-------|-----------|-------------|--------|----------|
| [facet-json](https://docs.rs/facet-json) | SD | Text | Event-based (custom) | [View](/guide/showcases/json) |
| [facet-kdl](https://docs.rs/facet-kdl) | SD | Text (node-based) | DOM ([kdl-rs](https://docs.rs/kdl)) | [View](/guide/showcases/kdl) |
| [facet-yaml](https://docs.rs/facet-yaml) | SD | Text | Event-based ([saphyr](https://docs.rs/saphyr)) | [View](/guide/showcases/yaml) |
| [facet-toml](https://docs.rs/facet-toml) | SD | Text | DOM ([toml_edit](https://docs.rs/toml_edit)) | [View](/guide/showcases/toml) |
| [facet-msgpack](https://docs.rs/facet-msgpack) | SD | Binary | Event-based (custom) | 🚫 |
| [facet-postcard](https://docs.rs/facet-postcard) | SD | Binary | Event-based (custom) | 🚫 |
| [facet-asn1](https://docs.rs/facet-asn1) | S | Binary | (Custom) | 🚫 |
| [facet-xdr](https://docs.rs/facet-xdr) | S | Binary | (Custom) | 🚫 |
| [facet-args](https://docs.rs/facet-args) | D | CLI | (Custom) | [View](/guide/showcases/args) |
| [facet-urlencoded](https://docs.rs/facet-urlencoded) | D | Text | (Custom) | 🚫 |
| [facet-csv](https://docs.rs/facet-csv) | S | Text | (Custom) | 🚫 |

## Scalar types

| Type | json | kdl | yaml | toml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|------|-----|------|--------|-----|
| `bool` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `u8..u64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `i8..i64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `u128/i128` | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| `f32/f64` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `char` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | ✅ |
| NonZero integers | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 | 🚫 | 🚫 | 🟡 | 🟡 | 🟡 |

## String types

All formats support `String`, `&str` (with best-effort borrowing), and `Cow<str>`.

## Lists, sets, and maps

| Type | json | kdl | yaml | toml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|------|-----|------|--------|-----|
| `Vec<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 |
| `[T; N]` (arrays) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🟡 |
| `HashSet<T>` | ✅ | ✅ | ✅ | 🟡 | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `BTreeSet<T>` | ✅ | ✅ | ✅ | 🟡 | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `HashMap<K, V>` | ✅ | 🟡 | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| `BTreeMap<K, V>` | ✅ | 🟡 | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🚫 | 🚫 | 🚫 |
| Non-string map keys | ✅ | 🚫 | ✅ | 🚫 | ✅ | ✅ | 🚫 | 🚫 | ➖ | ➖ | ➖ |

## Compound types

| Type | json | kdl | yaml | toml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|------|-----|------|--------|-----|
| `Option<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | ✅ | ✅ | 🟡 |
| `Result<T, E>` | ✅ | 🟡 | 🟡 | 🟡 | ✅ | ✅ | 🚫 | 🚫 | 🚫 | 🚫 | 🚫 |

## Smart pointers

| Type | json | kdl | yaml | toml | msgp | pcrd | asn1 | xdr | args | urlenc | csv |
|------|------|-----|------|------|------|------|------|-----|------|--------|-----|
| `Box<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| `Rc<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| `Arc<T>` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
