# Changelog

All notable changes to the facet project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
