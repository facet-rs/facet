# Format Crate Migration Plan

This document tracks what needs to happen before we can rename:
- `facet-xxx` crates to `facet-xxx-legacy`
- `facet-format-xxx` crates to `facet-xxx`

## Current Status Overview

| Format | facet-xxx | facet-format-xxx | Ready? |
|--------|-----------|------------------|--------|
| JSON | 692 tests, Cranelift JIT, streaming | 86+ tests, Tier-2 JIT, streaming, `Json<T>`, axum | **Close** |
| TOML | 230 tests, `Toml<T>`, axum | 55 tests, `Toml<T>`, axum | **Close** |
| YAML | 122 tests, `Yaml<T>`, axum | 28+ dedicated tests, `Yaml<T>`, axum | **Close** |
| KDL | 114 tests, `Kdl<T>`, axum | 7 tests, `Kdl<T>`, axum | Close |
| MsgPack | 46 tests, `MsgPack<T>`, axum | 59 tests, full JIT, `MsgPack<T>`, axum | **Yes** |
| Postcard | 377 tests, `Postcard<T>`, axum | 152 tests, full JIT, `Postcard<T>`, axum | **Yes** |
| XML | 127 tests, `Xml<T>`, axum, diff | 64 tests, streaming, `Xml<T>`, axum | **Close** |
| CSV | 1 test, **no deser** | 8 tests, full deser | **Yes** |
| ASN.1 | 24 tests | 40+ tests | **Close** |
| XDR | 4 tests, full impl | 26 tests, full impl | **Yes** |

## Detailed Feature Comparison

### JSON: facet-json vs facet-format-json

Both crates are fairly mature with different strengths.

| Feature | facet-json | facet-format-json |
|---------|------------|-------------------|
| **JIT** | Cranelift (sophisticated, ~3000 LOC) | Tier-2 JIT (~4000 LOC in jit/) |
| **Streaming** | Yes (corosensei, tokio, futures-io) | Yes (corosensei, tokio, futures-io) |
| **Tests** | 692 | 86 #[test] + ~90 format_suite |
| **RawJson** | Yes | Yes (but not wired up for capture_raw) |
| **Json\<T\>** | Yes | No |
| **Axum** | Yes (FromRequest, IntoResponse) | No (uses facet-json via facet-axum) |
| **Borrowed deser** | Yes | Yes |
| **TODOs** | 2 real (transparent wrapper, Box\<Option\<T\>\>) | 2 (flatten Option, SWAR optimize) |

**Migration tasks:**
- [ ] Wire up `capture_raw` for RawJson in FormatDeserializer
- [ ] Add `Json<T>` wrapper or decide on facet-axum strategy
- [ ] Port remaining tests (many are in generated_benchmark_tests.rs)
- [ ] Document JIT tier differences (Cranelift vs Tier-2)

### TOML: facet-toml vs facet-format-toml

| Feature | facet-toml | facet-format-toml |
|---------|------------|-------------------|
| **Tests** | 230 | 55 |
| **Toml\<T\>** | Yes | No |
| **Axum** | Yes | No |
| **JIT** | No | Feature flag only (no impl) |
| **to_string_pretty** | `todo!()` | No |
| **Borrowed deser** | No | No |
| **Serializer state** | Complete | Has `#![allow(dead_code)]` - WIP |

**Migration tasks:**
- [ ] Increase test coverage (230 -> 55 gap)
- [ ] Add `Toml<T>` wrapper type
- [ ] Add axum integration
- [ ] Implement pretty printing (both crates lack it)
- [ ] Finish serializer (remove dead_code allows)

### YAML: facet-yaml vs facet-format-yaml

| Feature | facet-yaml | facet-format-yaml |
|---------|------------|-------------------|
| **Tests** | 122 | **0** (only format_suite) |
| **Yaml\<T\>** | Yes | No |
| **Axum** | Yes | No |
| **JIT** | No | No |
| **Borrowed deser** | Yes | Yes |
| **Serde attr grammar** | Yes | No |

**Migration tasks:**
- [ ] **Critical: Add dedicated tests** (currently 0)
- [ ] Add `Yaml<T>` wrapper type
- [ ] Add axum integration
- [ ] Port serde attribute grammar if needed

### KDL: facet-kdl vs facet-format-kdl

| Feature | facet-kdl | facet-format-kdl |
|---------|-----------|------------------|
| **Tests** | 114 | 7 |
| **Kdl\<T\>** | Yes | No |
| **Axum** | Yes | No |
| **JIT** | No | No (uses FormatDeserializer) |
| **Borrowed deser** | No | Yes |
| **Attr grammar** | Same | Same |

**Migration tasks:**
- [ ] Increase test coverage significantly (114 -> 7 gap)
- [ ] Add `Kdl<T>` wrapper type
- [ ] Add axum integration
- [ ] Verify attribute grammar compatibility

### MsgPack: facet-msgpack vs facet-format-msgpack

facet-format-msgpack is actually more complete.

| Feature | facet-msgpack | facet-format-msgpack |
|---------|---------------|----------------------|
| **Tests** | 46 | 59 |
| **JIT** | No | **Full Tier-2 JIT** |
| **MsgPack\<T\>** | Yes | No |
| **Axum** | Yes | No |
| **Borrowed deser** | No | Yes |
| **Signed int** | Incomplete for negatives | Complete |

**Migration tasks:**
- [x] More tests than original
- [x] Full JIT support
- [x] Borrowed deserialization
- [ ] Add `MsgPack<T>` wrapper type
- [ ] Add axum integration

### Postcard: facet-postcard vs facet-format-postcard

Both are mature and feature-complete.

| Feature | facet-postcard | facet-format-postcard |
|---------|----------------|----------------------|
| **Tests** | 377 | 152 |
| **JIT** | No | **Full Tier-2 JIT** |
| **Postcard\<T\>** | Yes | Yes (in axum module) |
| **Axum** | Yes | Yes (more complete) |
| **Borrowed deser** | No | Yes |
| **Third-party types** | Extensive | Extensive |

**Migration tasks:**
- [x] Full JIT support
- [x] Has axum integration
- [x] Has wrapper type
- [ ] Verify all third-party type features work
- [ ] Consider increasing test count

### XML: facet-xml vs facet-format-xml

| Feature | facet-xml | facet-format-xml |
|---------|-----------|------------------|
| **Tests** | 127 | 64 |
| **Streaming** | No | **Yes** (std, tokio) |
| **JIT** | No | Has JIT tests |
| **Xml\<T\>** | Yes | No |
| **Axum** | Yes | No |
| **Diff** | Yes | No |
| **Attr grammar** | Same | Same |

**Migration tasks:**
- [ ] Increase test coverage
- [x] Streaming support (format version is better here)
- [ ] Add `Xml<T>` wrapper type
- [ ] Add axum integration
- [ ] Port diff feature

### CSV: facet-csv vs facet-format-csv

facet-format-csv is more complete.

| Feature | facet-csv | facet-format-csv |
|---------|-----------|------------------|
| **Serialization** | Yes | Yes |
| **Deserialization** | **No** (commented out) | Yes |
| **Tests** | 1 | 8 |

**Migration tasks:**
- [x] Has deserialization (original doesn't)
- [ ] Add more tests
- [ ] Document limitations

### ASN.1: facet-asn1 vs facet-format-asn1

| Feature | facet-asn1 | facet-format-asn1 |
|---------|-----------|-------------------|
| **Tests** | 24 | 40+ (roundtrip macro) |
| **Implementation** | Monolithic (~1000 LOC) | Clean separation |
| **Borrowed deser** | No | Yes |

**Migration tasks:**
- [x] Similar or better test coverage
- [x] Cleaner architecture
- [ ] Verify DER encoding compatibility
- [ ] Port type_tag attribute support

### XDR: facet-xdr vs facet-format-xdr (BLOCKER)

| Feature | facet-xdr | facet-format-xdr |
|---------|-----------|------------------|
| **Serialization** | Yes (~700 LOC) | **NO** |
| **Deserialization** | Yes | **NO** |
| **Tests** | 4 | 0 |
| **Status** | Complete | **Placeholder only** |

**facet-format-xdr/src/lib.rs contents:**
```rust
// TODO: Implement XDR parser and serializer
pub fn placeholder() {}
```

**Migration tasks:**
- [ ] Implement `XdrParser` (FormatParser trait)
- [ ] Implement `XdrSerializer` (FormatSerializer trait)
- [ ] Implement `from_slice`, `to_vec`
- [ ] Port all tests from facet-xdr
- [ ] Verify RFC 4506 compliance

## Cross-Cutting Concerns

### Wrapper Types Strategy

Original crates have wrapper types (`Json<T>`, `Toml<T>`, etc.) for:
- Axum extractors/responses
- Type-safe format markers

Options:
1. **Port to each facet-format-xxx crate** - Current approach for facet-format-postcard
2. **Centralize in facet-axum** - Currently re-exports from facet-json
3. **Keep in legacy crates** - Users depend on both old and new

Recommendation: Option 1 (port to format crates) for consistency.

### Axum Integration

Current state:
- facet-json, facet-toml, facet-yaml, facet-kdl, facet-msgpack, facet-postcard, facet-xml all have axum
- facet-format-postcard has axum
- facet-axum re-exports from facet-json

### Attribute Grammars

XML and KDL have attribute grammars (`#[facet(xml::element)]`, `#[facet(kdl::child)]`).
Both old and new crates define identical `Attr` enums.

Users import like:
```rust
use facet_xml as xml;  // or facet_kdl as kdl
```

After rename, they'd need:
```rust
use facet_xml as xml;  // now points to facet-format-xml
```

This should be seamless if we do the rename correctly.

### Documentation Updates

- [ ] Update format-crate-matrix/_index.md
- [ ] Update README.md ecosystem section
- [ ] Update docs.rs links
- [ ] Update facet.rs website

## Migration Strategy

1. **Phase 1: Achieve parity** - IN PROGRESS
   - [x] Implement facet-format-xdr (blocker) - DONE
   - [x] Standardize `from_*` APIs across all format crates - DONE
   - [x] Add wrapper types and axum integration to format crates - DONE
   - [ ] Add tests to facet-format-yaml
   - [ ] Increase test coverage for TOML, KDL

2. **Phase 2: Deprecation**
   - Mark facet-xxx crates as deprecated
   - Update docs to point to facet-format-xxx

3. **Phase 3: Rename**
   - Publish facet-format-xxx as new facet-xxx versions
   - Keep old crates available but deprecated

4. **Phase 4: Cleanup**
   - Eventually yank old versions or mark as legacy

## Completed Work

### API Standardization

All format crates now have consistent deserialization APIs:

**Text formats** (JSON, TOML, YAML, KDL, XML, CSV):
- `from_str<T>(input: &str) -> Result<T, DeserializeError<E>>`
- `from_str_borrowed<'i, 'f, T>(input: &'i str) -> Result<T, DeserializeError<E>>`
- `from_slice<T>(input: &[u8]) -> Result<T, DeserializeError<E>>`
- `from_slice_borrowed<'i, 'f, T>(input: &'i [u8]) -> Result<T, DeserializeError<E>>`

**Binary formats** (MsgPack, Postcard, ASN.1, XDR):
- `from_slice<T>(input: &[u8]) -> Result<T, DeserializeError<E>>`
- `from_slice_borrowed<'i, 'f, T>(input: &'i [u8]) -> Result<T, DeserializeError<E>>`

### Axum Integration

All major format crates now have `axum` feature with wrapper types:

| Format | Wrapper Type | Feature Flag |
|--------|-------------|--------------|
| JSON | `Json<T>` | `axum` |
| TOML | `Toml<T>` | `axum` |
| YAML | `Yaml<T>` | `axum` |
| KDL | `Kdl<T>` | `axum` |
| MsgPack | `MsgPack<T>` | `axum` |
| Postcard | `Postcard<T>` | `axum` |
| XML | `Xml<T>` | `axum` |

Usage example:
```rust
use facet_format_json::Json;

async fn handler(Json(payload): Json<MyRequest>) -> Json<MyResponse> {
    Json(MyResponse { ... })
}
```

## Open Questions

1. **Rename vs promote?** Should we actually rename, or just promote facet-format-xxx as primary and deprecate facet-xxx?

2. **crates.io naming** - Can't actually rename crates. Options:
   - Publish new major versions of facet-xxx that re-export facet-format-xxx
   - Keep both naming schemes forever

3. **Test coverage threshold** - What's the minimum before migration?

4. **JIT differences** - Cranelift (facet-json) vs Tier-2 (facet-format-*) - are they equivalent? Document differences?

5. ~~**Wrapper type location** - In format crates or centralized in facet-axum?~~ **RESOLVED**: Each format crate has its own wrapper type.
