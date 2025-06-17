# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.24.20](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.19...facet-deserialize-v0.24.20) - 2025-06-17

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.19](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.18...facet-deserialize-v0.24.19) - 2025-06-15

### Added

- support 128-bit integers in facet-deserialize

## [0.24.18](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.17...facet-deserialize-v0.24.18) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.17](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.16...facet-deserialize-v0.24.17) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.16](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.15...facet-deserialize-v0.24.16) - 2025-06-03

### Other

- Add discord logo + link

## [0.24.15](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.14...facet-deserialize-v0.24.15) - 2025-06-02

### Other

- Migrate push_ methods to begin_ convention in facet-reflect
- Allow transparent key types
- remove unnecessary clone

## [0.24.14](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.13...facet-deserialize-v0.24.14) - 2025-05-31

### Fixed

- fix benchmark

### Other

- Simplify code for set_numeric_value
- facet-json is not _currently_ nostd, actually, because of std::io::Write
- Fix facet-args tests
- More facet-yaml test fixes
- facet-json tests pass
- Fix tests
- Tuple handling
- Fix Bytes deserialization through implicit conversion
- More facet-json tests
- Some json fixes
- wow everything typechecks
- facet-deserialize fixes
- Start porting old reflect tests
- begin/end is more intuitive than push/pop
- Rename some methods
- Remove SequenceType::Tuple - tuples are now structs
- Deinitialization is wrong (again)

## [0.24.13](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.12...facet-deserialize-v0.24.13) - 2025-05-27

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.12](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.11...facet-deserialize-v0.24.12) - 2025-05-26

### Other

- Don't crash when errors straddle EOF

## [0.24.11](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.10...facet-deserialize-v0.24.11) - 2025-05-24

### Added

- *(args)* fill Substack via `Outcome::Resegment`

### Other

- Update deserialization to deserialize wrapper types as their inner shape

## [0.24.10](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.9...facet-deserialize-v0.24.10) - 2025-05-21

### Added

- *(deserialize)* give `Span<Raw>` byte-level precision via a `Substack` of `Span<Subspan>`

### Other

- Support deserializing to `Arc<T>`

## [0.24.9](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.8...facet-deserialize-v0.24.9) - 2025-05-20

### Added

- *(args)* arg-wise spans for reflection errors; ToCooked trait

### Other

- Show warning on truncation
- Truncate when showing errors in one long JSON line

## [0.24.8](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.7...facet-deserialize-v0.24.8) - 2025-05-18

### Other

- Introduce `'shape` lifetime, allowing non-'static shapes.

## [0.24.7](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.6...facet-deserialize-v0.24.7) - 2025-05-16

### Added

- facet-args `Cli` trait impl; deserialize `&str` field
- *(deserialize)* support multiple input types via generic `Format`

## [0.24.6](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.5...facet-deserialize-v0.24.6) - 2025-05-13

### Other

- Fix enum tests with a single tuple field
- Well it says the field is not initialized, so.

## [0.24.5](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.4...facet-deserialize-v0.24.5) - 2025-05-12

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.4](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.3...facet-deserialize-v0.24.4) - 2025-05-12

### Added

- *(facet-args)* rely on facet-deserialize StackRunner

## [0.24.3](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.2...facet-deserialize-v0.24.3) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.2](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.1...facet-deserialize-v0.24.2) - 2025-05-10

### Other

- Add support for partially initializing arrays, closes #504

## [0.24.1](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.24.0...facet-deserialize-v0.24.1) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.0](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.23.0...facet-deserialize-v0.24.0) - 2025-05-10

### Other

- Release facet-reflect
- Release facet-derive-parser
- Upgrade facet-core
- Determine enum variant after default_from_fn
- Uncomment deserialize

## [0.23.0](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.22.0...facet-deserialize-v0.23.0) - 2025-05-08

### Other

- *(deserialize)* [**breaking**] make deserialize format stateful

## [0.22.0](https://github.com/facet-rs/facet/compare/facet-deserialize-v0.21.0...facet-deserialize-v0.22.0) - 2025-05-06

### Other

- Get started on an event-based approach to facet-deserialize ([#500](https://github.com/facet-rs/facet/pull/500))
