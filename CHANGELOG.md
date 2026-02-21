# Changelog

All notable changes to the facet project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.44.0](https://github.com/facet-rs/facet/compare/facet-core-v0.43.2...facet-core-v0.44.0) - 2026-02-21

### Added

- *(facet-args)* layered config with provenance tracking and beautiful config dump ([#1907](https://github.com/facet-rs/facet/pull/1907))
- *(facet)* add #[facet(cow)] attribute for cow-like enum semantics ([#1898](https://github.com/facet-rs/facet/pull/1898))

### Fixed

- *(core)* print generic params in Shape display for Option/Result ([#2038](https://github.com/facet-rs/facet/pull/2038))
- *(facet)* make cow enums serialize/deserialize transparently ([#1901](https://github.com/facet-rs/facet/pull/1901))
- remove unnecessary `T: 'static` bound from Vec impl ([#1894](https://github.com/facet-rs/facet/pull/1894))

### Other

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
