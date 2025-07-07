# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Add start_some to serializer trait

## [0.24.18](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.17...facet-serialize-v0.24.18) - 2025-07-03

### Other

- Rename SmartPointer to Pointer
- Allow serializing references

## [0.24.17](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.16...facet-serialize-v0.24.17) - 2025-06-30

### Other

- Apply clippy fixes

## [0.24.16](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.15...facet-serialize-v0.24.16) - 2025-06-26

### Other

- impl Facet for Arc<str>, Rc<str>, and Box<str>
- Fix HashSet serializing to null due to Set being an unhandled type
- Remove ScalarDef
- Remove PartialEq implementations
- Apply modern clippy fixes (mostly format strings)

## [0.24.15](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.14...facet-serialize-v0.24.15) - 2025-06-17

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.14](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.13...facet-serialize-v0.24.14) - 2025-06-15

### Other

- actually use the cpeek instead of the main peek when serializing arrays
- Introduced sized/unsized ValueVTable variants

## [0.24.13](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.12...facet-serialize-v0.24.13) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.12](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.11...facet-serialize-v0.24.12) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.11](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.10...facet-serialize-v0.24.11) - 2025-06-03

### Other

- Add discord logo + link
- Fix JSON serialization of &[u8] slices

## [0.24.10](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.9...facet-serialize-v0.24.10) - 2025-06-02

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.9](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.8...facet-serialize-v0.24.9) - 2025-05-31

### Other

- Add serialization for box
- YAML transparent types fixes
- Fix Bytes deserialization through implicit conversion
- Remove SequenceType::Tuple - tuples are now structs

## [0.24.8](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.7...facet-serialize-v0.24.8) - 2025-05-27

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.7](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.6...facet-serialize-v0.24.7) - 2025-05-26

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.6](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.5...facet-serialize-v0.24.6) - 2025-05-24

### Other

- Fix cyclic types with indirection for optional fns in `ValueVTable`

## [0.24.5](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.4...facet-serialize-v0.24.5) - 2025-05-21

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.4](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.3...facet-serialize-v0.24.4) - 2025-05-20

### Other

- Remove leftover comment
- Refactor serialization to not need `DoubleEndedIterator`
- Add custom iterator impl for `HasFields.fields_for_serialize()`

## [0.24.3](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.2...facet-serialize-v0.24.3) - 2025-05-18

### Other

- Introduce `'shape` lifetime, allowing non-'static shapes.

## [0.24.2](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.1...facet-serialize-v0.24.2) - 2025-05-16

### Other

- Rust 1.87 clippy fixes
- almost fix everything
- Fix json tests
- Support OffsetDateTime in YAML

## [0.24.1](https://github.com/facet-rs/facet/compare/facet-serialize-v0.24.0...facet-serialize-v0.24.1) - 2025-05-13

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.23.6](https://github.com/facet-rs/facet/compare/facet-serialize-v0.23.5...facet-serialize-v0.23.6) - 2025-05-12

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.23.5](https://github.com/facet-rs/facet/compare/facet-serialize-v0.23.4...facet-serialize-v0.23.5) - 2025-05-12

### Other

- Update `facet-serialize` to handle a few more affinities
- Add support for time crate's OffsetDateTime and UtcDateTime

## [0.23.4](https://github.com/facet-rs/facet/compare/facet-serialize-v0.23.3...facet-serialize-v0.23.4) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.23.3](https://github.com/facet-rs/facet/compare/facet-serialize-v0.23.2...facet-serialize-v0.23.3) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.23.2](https://github.com/facet-rs/facet/compare/facet-serialize-v0.23.1...facet-serialize-v0.23.2) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.23.1](https://github.com/facet-rs/facet/compare/facet-serialize-v0.23.0...facet-serialize-v0.23.1) - 2025-05-10

### Other

- Release facet-reflect
- Release facet-derive-parser
- Upgrade facet-core
- Determine enum variant after default_from_fn
- Uncomment deserialize
- Fix up enums a bit
- Make variant() getters fallible — we might not know the internal enough to read the discriminant etc.
- Make options work for facet-serialize
- debug facet-serialize
- Fix memory leak, work on facet-serisalize

## [0.23.0](https://github.com/facet-rs/facet/compare/facet-serialize-v0.22.0...facet-serialize-v0.23.0) - 2025-05-08

### Other

- Use `PeekListLike`

## [0.22.0](https://github.com/facet-rs/facet/compare/facet-serialize-v0.21.0...facet-serialize-v0.22.0) - 2025-05-06

### Added

- *(serialize)* add more optional start- and end-serialize calls

### Fixed

- *(reflect)* add missing scalar types

### Other

- *(serialize)* make end_* trait methods optional
- *(serialize)* optionally widen-cast number types to u64 in trait

## [0.21.0](https://github.com/facet-rs/facet/compare/facet-serialize-v0.20.0...facet-serialize-v0.21.0) - 2025-05-02

### Other

- JSON facet-serialize?
- Use facet-serialize in facet-msgpack
- Don't depend on git version of insta anymore
- Introduce facet-serialize
