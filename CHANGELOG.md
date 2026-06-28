# Changelog

All notable changes to the facet project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2](https://github.com/facet-rs/facet/compare/weavy-v0.2.1...weavy-v0.2.2) - 2026-06-28

### Other

- updated the following local packages: copypatch, copypatch

## [0.50.0-rc.4](https://github.com/facet-rs/facet/compare/facet-hash-v0.50.0-rc.3...facet-hash-v0.50.0-rc.4) - 2026-06-26

### Other

- add opt-in native key hashing

## [0.50.0-rc.4](https://github.com/facet-rs/facet/compare/facet-format-v0.50.0-rc.3...facet-format-v0.50.0-rc.4) - 2026-06-26

### Other

- add `omit_none` serialize option

## [0.50.0-rc.3](https://github.com/facet-rs/facet/compare/facet-hash-v0.50.0-rc.2...facet-hash-v0.50.0-rc.3) - 2026-06-26

### Fixed

- *(facet-hash)* write byte lengths through hasher

### Other

- *(facet-hash)* add concrete FNV byte hashing
- *(facet-hash)* bulk hash byte sequences
- Share facet-hash Weavy analysis

## [0.50.0-rc.3](https://github.com/facet-rs/facet/compare/facet-postcard-v0.50.0-rc.2...facet-postcard-v0.50.0-rc.3) - 2026-06-26

### Other

- Fix Dibs string parameter signatures

## [0.50.0-rc.3](https://github.com/facet-rs/facet/compare/facet-json-v0.50.0-rc.2...facet-json-v0.50.0-rc.3) - 2026-06-26

### Fixed

- fix #2342 but it uses unwrap so kinda ugly

### Other

- Match JSON map-key proxy repro opacity
- Add test for #[facet(content)] on a #[facet(other)]
- add tests related to #2342
- clean up tests for #2341
- Implement fix for #2341
- Add tests for #2341
- Fix #2363
- add failing tests for #2363

## [0.2.1](https://github.com/facet-rs/facet/compare/weavy-v0.2.0...weavy-v0.2.1) - 2026-06-26

### Other

- Share facet-hash Weavy analysis
- Bundle Weavy IR analysis results
- Share intrinsic child traversal in Weavy

## [0.50.0-rc.3](https://github.com/facet-rs/facet/compare/facet-format-v0.50.0-rc.2...facet-format-v0.50.0-rc.3) - 2026-06-26

### Fixed

- fix #2342 but it uses unwrap so kinda ugly

### Other

- replace unwrap with actual error message
- Potential fix for pull request finding
- Implement fix for #2341

## [0.50.0-rc.3](https://github.com/facet-rs/facet/compare/facet-reflect-v0.50.0-rc.2...facet-reflect-v0.50.0-rc.3) - 2026-06-26

### Other

- Fix #2363

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-cargo-toml-v0.50.0-rc.1...facet-cargo-toml-v0.50.0-rc.2) - 2026-06-25

### Other

- extract ecosystem crate sections into their own subsites
- Remove stale Cranelift JIT remnants

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-hash-v0.0.0...facet-hash-v0.50.0-rc.2) - 2026-06-25

### Other

- add READMEs for fable, weavy, copypatch, facet-hash; fable+weavy subsites include theirs
- Add native JIT hash plans
- Plan scalar field runs in facet-hash
- Fast-path scalar children in facet-hash
- Inline acyclic facet-hash plans
- Run facet-hash on canonical Weavy IR
- Add derived Hash benchmark comparisons
- Add Weavy-backed facet-hash crate

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-validate-v0.50.0-rc.1...facet-validate-v0.50.0-rc.2) - 2026-06-25

### Other

- content-quality pass across the ecosystem (consistency, structure, friendliness)
- migrate facet-default/error/validate guides into their own subsites

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/rediff-v0.50.0-rc.1...rediff-v0.50.0-rc.2) - 2026-06-25

### Other

- content-quality pass across the ecosystem (consistency, structure, friendliness)
- extract ecosystem crate sections into their own subsites
- give the vendored products their own subsites

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-json-schema-v0.50.0-rc.1...facet-json-schema-v0.50.0-rc.2) - 2026-06-25

### Other

- Fix facet-json-schema proxy test clippy
- check for field-level proxy before resolving shape

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-asn1-v0.50.0-rc.1...facet-asn1-v0.50.0-rc.2) - 2026-06-25

### Other

- Fix facet-format CI regressions

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-axum-v0.50.0-rc.1...facet-axum-v0.50.0-rc.2) - 2026-06-25

### Other

- extract ecosystem crate sections into their own subsites

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-yaml-v0.50.0-rc.1...facet-yaml-v0.50.0-rc.2) - 2026-06-25

### Other

- update Cargo.toml dependencies

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-singularize-v0.50.0-rc.1...facet-singularize-v0.50.0-rc.2) - 2026-06-25

### Other

- update Cargo.toml dependencies

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-toml-v0.50.0-rc.1...facet-toml-v0.50.0-rc.2) - 2026-06-25

### Other

- update Cargo.toml dependencies

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-postcard-v0.50.0-rc.1...facet-postcard-v0.50.0-rc.2) - 2026-06-25

### Other

- Remove stale Cranelift JIT remnants

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-value-v0.50.0-rc.1...facet-value-v0.50.0-rc.2) - 2026-06-25

### Other

- update Cargo.toml dependencies

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-msgpack-v0.50.0-rc.1...facet-msgpack-v0.50.0-rc.2) - 2026-06-25

### Other

- Remove stale Cranelift JIT remnants

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-json-v0.50.0-rc.1...facet-json-v0.50.0-rc.2) - 2026-06-25

### Added

- *(facet-json)* support maps in weavy deser
- *(facet-json)* specialize scalar struct list JIT roots
- *(facet-json)* specialize ordered scalar JIT roots
- *(facet-json)* run scalar structs through Weavy JIT
- *(facet-json)* expose Weavy JIT mode
- *(facet-json)* add opt-in weavy deserializer

### Fixed

- *(facet-json)* cfg-gate native parser probes
- *(facet-json)* unwind direct list structs before buffers

### Other

- Trim facet-json Weavy parity cold paths
- Trim facet-json Weavy hot frames
- Recover facet-json Weavy fast paths
- Close facet-json Weavy parity gaps
- Parameterize facet-json integration backend tests
- Support flattened fields in facet-json Weavy
- Expand facet-json Weavy parity
- Run facet-json format suite against Weavy
- Support tuple structs in Weavy deserialization
- Support Weavy transparent and proxy deser
- Support numeric and untagged enums in facet-json Weavy
- Benchmark Weavy tagged enum deserialization
- Expand Weavy tagged enum deserialization
- Expand Weavy external enum deserialization
- Add Weavy external enum deserialization
- Expand facet-json Weavy parity
- Document facet-json Weavy parity grid
- Speed up strict skipped JSON values
- Recover fast strict JSON skip path
- Fix more facet-json Weavy fuzz parity gaps
- Fix facet-json Weavy oracle parity gaps
- Add facet-json Weavy fuzz oracle
- Move JSON root scanner step into native JIT
- Remove inactive Weavy map bulk path
- Speed up Weavy map insertion
- content-quality pass across the ecosystem (consistency, structure, friendliness)
- fix cross-references between migrated guides (point at new subsite routes)
- stand up facet-json + facet-pretty subsites, migrate their guides
- *(facet-json)* streamline native scalar struct lists
- Speed up Weavy f64 cursor parsing
- Add Linux x86_64 Weavy native JIT support
- *(facet-json)* precompute native scalar readers
- *(facet-json)* specialize native cursor scalars
- *(facet-json)* scan ordered native roots with a cursor
- *(facet-json)* speed up ordered native JIT probes
- Add raw field dispatch to JSON Weavy deser
- Fuse tiny i32 Weavy struct decode
- Fast-path Weavy container starts
- Fast-path ordered i32 scalar structs
- Fast-path tiny Weavy scalar structs
- Preselect Weavy scalar input writers
- Speed up Weavy scalar token handling
- Fast-path ordered Weavy scalar structs
- Fuse Weavy scalar struct decoding
- Speed up Weavy container starts
- Specialize Weavy struct field tracking
- Remove fake streaming state from JSON scanner
- Speed up Weavy JSON field dispatch
- *(facet-json)* preselect Weavy scalar field writers
- *(facet-json)* specialize Weavy integer list scalars
- *(facet-json)* write Weavy scalars from raw tokens
- *(facet-json)* fuse Weavy scalar list parsing
- *(facet-json)* adopt Weavy list buffers directly
- *(facet-json)* drain scalar list elements in Weavy
- *(facet-json)* decode scalar options inline in Weavy
- *(facet-json)* skip duplicate check on ordered Weavy fields
- *(facet-json)* drain Weavy scalar fields in object loop
- *(facet-json)* fast path ordered Weavy fields
- *(facet-json)* read Weavy field keys directly
- *(facet-json)* let weavy read scalar tokens directly
- *(weavy)* tighten json runner bookkeeping
- *(facet-json)* fuse weavy scalar child paths
- *(facet-json)* read weavy inputs directly from parser
- *(facet-json)* avoid event peeks in weavy loops
- *(weavy)* run dense block programs
- *(facet-json)* reuse weavy scratch slots
- *(weavy)* share typed-memory runtime guards
- Lower recursive stack test default depth
- Remove facet-json VM experiment
- Speed up facet-json VM parsing
- Reduce facet-json VM dispatch overhead
- Speed up facet-json VM struct hot path
- Add serde_json VM benchmark comparisons
- Cache reusable facet-json VM plans
- Add facet-json VM deserialization benchmarks
- Add opt-in facet-json deserialization VM
- Lower facet-json type plans to bytecode
- Add weavy lowered program substrate
- Relax recursive stack CI threshold
- Fix recursive stack test clippy warnings
- Measure recursive facet-json stack usage

## [0.2.0](https://github.com/facet-rs/facet/compare/weavy-v0.1.0...weavy-v0.2.0) - 2026-06-25

### Added

- *(facet-json)* run scalar structs through Weavy JIT
- add opt-in Weavy runner stats
- *(weavy)* group scalar record copies

### Fixed

- *(weavy)* avoid cfg-gated rustdoc link

### Other

- Move Fable plans onto canonical Weavy IR
- Support Phon reference pointer shapes
- Remove inactive Weavy map bulk path
- Speed up Weavy map insertion
- content-quality pass across the ecosystem (consistency, structure, friendliness)
- add READMEs for fable, weavy, copypatch, facet-hash; fable+weavy subsites include theirs
- give the vendored products their own subsites
- Add Linux x86_64 Weavy native JIT support
- Inline native JIT program accessors
- Extract native JIT program substrate to weavy
- Run facet-hash on canonical Weavy IR
- Add Weavy intrinsic effect contracts
- Route PHON typed memory through canonical Weavy IR
- Add canonical Weavy IR skeleton
- Extract copy-patch JIT substrate into Weavy
- *(facet-json)* adopt Weavy list buffers directly
- *(weavy)* cut runner frame overhead
- *(weavy)* tighten json runner bookkeeping
- *(weavy)* run dense block programs
- *(facet-json)* reuse weavy scratch slots
- *(weavy)* share typed-memory runtime guards
- Aggregate PHON JIT shape reports by surface
- Report PHON JIT program shape stats
- Add record byte ownership metadata
- Move PHON memory lowering helpers into weavy

## [0.50.0-rc.2](https://github.com/facet-rs/facet/compare/facet-core-v0.50.0-rc.1...facet-core-v0.50.0-rc.2) - 2026-06-25

### Other

- Speed up Weavy map insertion
- Remove stale Cranelift JIT remnants

## [0.50.0-rc.1](https://github.com/facet-rs/facet/compare/facet-urlencoded-v0.46.4...facet-urlencoded-v0.50.0-rc.1) - 2026-05-26

### Added

- *(facet-urlencoded)* Option<T> + i64 + bool scalars

## [0.46.4](https://github.com/facet-rs/facet/compare/facet-pretty-v0.46.3...facet-pretty-v0.46.4) - 2026-05-19

### Fixed

- *(facet-pretty)* gate terminal-light behind non-wasm target

## [0.46.3](https://github.com/facet-rs/facet/compare/facet-pretty-v0.46.2...facet-pretty-v0.46.3) - 2026-05-19

### Added

- *(facet-pretty)* keep tokyo_night module exported for downstream
- *(facet-pretty)* Melange palette with terminal theme detection

### Fixed

- *(facet-pretty)* fix rustdoc intra-doc links in colors module
- *(facet-pretty)* heal 0.47 semver breakage with const + deprecated shims

## [0.46.2](https://github.com/facet-rs/facet/compare/facet-macros-impl-v0.46.1...facet-macros-impl-v0.46.2) - 2026-05-11

### Fixed

- *(facet-default)* auto-convert default values via TryFrom

## [0.46.1](https://github.com/facet-rs/facet/compare/facet-core-v0.46.0...facet-core-v0.46.1) - 2026-05-10

### Other

- char, unit: expose clone impl

## [0.46.0](https://github.com/facet-rs/facet/compare/facet-core-v0.45.0...facet-core-v0.46.0) - 2026-04-15

### Other

- sync all companion crates to v0.46.0 in one version group

## [0.45.0](https://github.com/facet-rs/facet/compare/facet-core-v0.44.4...facet-core-v0.45.0) - 2026-04-14

### Added

- *(core)* add pop + swap list vtable entries

### Fixed

- *(core)* drop Result via typed drop_in_place to satisfy Stacked Borrows

## [0.44.5](https://github.com/facet-rs/facet/compare/facet-v0.44.4...facet-v0.44.5) - 2026-04-13

### Other

- updated the following local packages: facet-core, facet-macros, facet-reflect

## [0.44.4](https://github.com/facet-rs/facet/compare/facet-v0.44.3...facet-v0.44.4) - 2026-04-13

### Other

- updated the following local packages: facet-reflect

## [0.44.3](https://github.com/facet-rs/facet/compare/facet-default-v0.44.2...facet-default-v0.44.3) - 2026-03-16

### Other

- updated the following local packages: facet

## [0.44.2](https://github.com/facet-rs/facet/compare/facet-core-v0.44.1...facet-core-v0.44.2) - 2026-03-12

### Added

- *(crates)* address Copilot's review comments
- *(crates)* add support for `semver`

### Other

- remove captain readme templates and README.md.in files
- drop duplicate crate title headings in reedme docs
- template shared readme footer with reedme
- migrate crate docs/readmes to cargo-reedme

## [0.44.1](https://github.com/facet-rs/facet/compare/facet-core-v0.44.0...facet-core-v0.44.1) - 2026-03-03

### Other

- Make postcard opaque passthrough API backwards compatible
- Add postcard opaque passthrough encoded-bytes path

## [0.44.0](https://github.com/facet-rs/facet/compare/facet-core-v0.43.2...facet-core-v0.44.0) - 2026-03-01

### Added

- add plan-agnostic opaque adapter MVP ([#2068](https://github.com/facet-rs/facet/pull/2068)) ([#2074](https://github.com/facet-rs/facet/pull/2074))
- *(facet-args)* layered config with provenance tracking and beautiful config dump ([#1907](https://github.com/facet-rs/facet/pull/1907))
- *(facet)* add #[facet(cow)] attribute for cow-like enum semantics ([#1898](https://github.com/facet-rs/facet/pull/1898))

### Fixed

- *(core)* make ownership transfer explicit in list/slice push vtables ([#2088](https://github.com/facet-rs/facet/pull/2088))
- *(core)* print generic params in Shape display for Option/Result ([#2038](https://github.com/facet-rs/facet/pull/2038))
- *(facet)* make cow enums serialize/deserialize transparently ([#1901](https://github.com/facet-rs/facet/pull/1901))
- remove unnecessary `T: 'static` bound from Vec impl ([#1894](https://github.com/facet-rs/facet/pull/1894))

### Other

- Make OptionVTable callbacks extern "C" and FFI-safe ([#2106](https://github.com/facet-rs/facet/pull/2106))
- Allow borrowed #[facet(opaque)] fields safely ([#2087](https://github.com/facet-rs/facet/pull/2087))
- Make OxPtrConst/OxPtrMut constructor invariants explicit ([#2080](https://github.com/facet-rs/facet/pull/2080))
- Expose const generic parameters via Shape reflection ([#2061](https://github.com/facet-rs/facet/pull/2061))
- don't allocate ZSTs ([#2013](https://github.com/facet-rs/facet/pull/2013))
- use UserType::Enum always ([#2006](https://github.com/facet-rs/facet/pull/2006))
- Consolidate trame design (née facet-reflect2) ([#1992](https://github.com/facet-rs/facet/pull/1992))
- More ops, more fuzzing (+ a drive-by serialization fix) ([#1984](https://github.com/facet-rs/facet/pull/1984))
- Refactor TypePlan to 32-bit arena indices, eliminate Box::leak, add benchmark infrastructure ([#1968](https://github.com/facet-rs/facet/pull/1968))
- eliminate type_identifier usage ([#1965](https://github.com/facet-rs/facet/pull/1965))
- Re-enable specialization-based auto-detection as default ([#1919](https://github.com/facet-rs/facet/pull/1919))
- Add Facet implementations for jiff::civil::Date and jiff::civil::Time ([#1911](https://github.com/facet-rs/facet/pull/1911))

## [0.43.2](https://github.com/facet-rs/facet/compare/facet-core-v0.43.1...facet-core-v0.43.2) - 2026-01-23

### Added

- *(facet-core)* Add SmallVec support ([#1884](https://github.com/facet-rs/facet/pull/1884))

### Other

- *(tests)* consolidate integration test binaries ([#1887](https://github.com/facet-rs/facet/pull/1887))

## [0.43.1](https://github.com/facet-rs/facet/compare/facet-core-v0.43.0...facet-core-v0.43.1) - 2026-01-23

### Added

- add Facet implementation for tendril crate ([#1870](https://github.com/facet-rs/facet/pull/1870))

## [0.42.0](https://github.com/facet-rs/facet/compare/facet-core-v0.41.0...facet-core-v0.42.0) - 2026-01-06

### Added

- implement Facet for core::convert::Infallible

### Fixed

- mark function pointers as invariant to prevent lifetime UB
- *(soundness)* make OxRef::new and OxMut::new unsafe

### Other

- *(bytestring)* simplify ByteString impl with vtable_direct! macro
- Fix #1629: Preserve custom HTML elements during parse/serialize roundtrip
- Add facet-validate crate for field validation during deserialization
- Add rust_decimal::Decimal support + fix XML type inference
- Add rust_decimal::Decimal support to facet-core
- Add Facet implementation for smol_str::SmolStr
- Set up release-plz with synchronized versions and trusted publishing
- Add `facet_no_doc` cfg for global doc string stripping
- Fix facet-pretty to respect skip_serializing_if and add HTML roundtrip tests
- Add html::text attribute for enum variants and comprehensive roundtrip test
- Fix inconsistent Shape hash (issue #1574)
- Fix soundness issue: Attr can contain non-Sync data
- Require 'static for Opaque Facet impl
- *(facet-core)* simplify Ox API by requiring T: Facet
- fix broken intra-doc link to Peek in facet-core
- Improve AGENTS.md, closes #1551
