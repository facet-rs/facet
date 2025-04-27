# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.2](https://github.com/facet-rs/facet/compare/facet-json-v0.18.1...facet-json-v0.18.2) - 2025-04-27

### Other

- update Cargo.toml dependencies

## [0.18.1](https://github.com/facet-rs/facet/compare/facet-json-v0.18.0...facet-json-v0.18.1) - 2025-04-26

### Added

- Add support for rename_all on containers

## [0.12.1](https://github.com/facet-rs/facet/compare/facet-json-v0.12.0...facet-json-v0.12.1) - 2025-04-23

### Other

- Last few cleanups
- BoundedGenericParams
- WIP
- Back to depot runners.

## [0.12.0](https://github.com/facet-rs/facet/compare/facet-json-v0.11.1...facet-json-v0.12.0) - 2025-04-21

### Other

- Support tuple-enums in JSON
- Maybe calling put on tuple variants, tuples, arrays, etc. — should just automatically do push?
- Implement naive JSON variant
- Support field-level default
- Implement the skip_serializing/skip_serializing_if attribute
- Only do struct-level default if facet(default) is set
- Yay container-level default alright
- WIP struct-level default
- fewer asserts
- Better test for struct with missing field
- *(facet-json)* more test coverage for de/serialisation
- Respect deny_unknown_fields (once again)
- add ignored deserialization test
- make tuples serialize into lists rather than objects
- msrv/nostd fixes
- Uncomment rest of JSON tests but ignore them
- Add getters to Shape & Field
- Map key/value
- More colors, update snapshots
- Improve number handling for JSON deserialization
- option and number
- very nice error reporting as it turns out
- Use TryFrom to deserialize NonZero<T>
- nested arrays
- add docs, fix snapshots
- ooh spicy
- Save value/vec, etc.
- Next up: empty vecs
- Booleans
- Works for structs
- Ignore test for now
- err tests
- Introduce JSON tokenizer

## [0.11.1](https://github.com/facet-rs/facet/compare/facet-json-v0.11.0...facet-json-v0.11.1) - 2025-04-20

### Other

- Don't allocate strings in facet-json deserialization unless necessary
- Refactor JSON number deserialization to use Wip::try_put_f64

## [0.11.0](https://github.com/facet-rs/facet/compare/facet-json-v0.10.0...facet-json-v0.11.0) - 2025-04-19

### Added

- *(json)* Support default attribute.
- feat(json) Support default at the container level
- feat(json) Better error messages when a field is missing
- feat(json) Add support for json booleans

## [0.10.0](https://github.com/facet-rs/facet/compare/facet-json-v0.9.3...facet-json-v0.10.0) - 2025-04-19

### Added

- feat(json) Support deny_unknown_fields
- feat(json) Fix array parsing

## [0.9.3](https://github.com/facet-rs/facet/compare/facet-json-v0.9.2...facet-json-v0.9.3) - 2025-04-18

### Other

- update Cargo.toml dependencies

## [0.9.2](https://github.com/facet-rs/facet/compare/facet-json-v0.9.1...facet-json-v0.9.2) - 2025-04-18

### Other

- update Cargo.toml dependencies

## [0.9.1](https://github.com/facet-rs/facet/compare/facet-json-v0.9.0...facet-json-v0.9.1) - 2025-04-18

### Other

- update Cargo.toml dependencies

## [0.2.2](https://github.com/facet-rs/facet/compare/facet-json-v0.2.1...facet-json-v0.2.2) - 2025-04-12

### Added

- *(reflect)* add `ScalarType` enum ([#173](https://github.com/facet-rs/facet/pull/173))

### Other

- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- Opaque initialization of Some ([#169](https://github.com/facet-rs/facet/pull/169))

## [0.2.1](https://github.com/facet-rs/facet/compare/facet-json-v0.2.0...facet-json-v0.2.1) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.2.0](https://github.com/facet-rs/facet/compare/facet-json-v0.1.15...facet-json-v0.2.0) - 2025-04-11

### Other

- Revert to facet-{core,derive,reflect} deps, closes #156 ([#159](https://github.com/facet-rs/facet/pull/159))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- wip reflect ([#155](https://github.com/facet-rs/facet/pull/155))
- get facet-json back together ([#154](https://github.com/facet-rs/facet/pull/154))

## [0.1.15](https://github.com/facet-rs/facet/compare/facet-json-v0.1.14...facet-json-v0.1.15) - 2025-04-11

### Other

- Move the template files next to their output and improve the output of the facet-codegen crate.

## [0.1.14](https://github.com/facet-rs/facet/compare/facet-json-v0.1.13...facet-json-v0.1.14) - 2025-04-11

### Other

- Logo credit

## [0.1.13](https://github.com/facet-rs/facet/compare/facet-json-v0.1.12...facet-json-v0.1.13) - 2025-04-11

### Other

- updated the following local packages: facet-json-read, facet-json-write

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-json-v0.1.11...facet-json-v0.1.12) - 2025-04-10

### Other

- updated the following local packages: facet-json-read, facet-json-write

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-json-v0.1.10...facet-json-v0.1.11) - 2025-04-10

### Other

- updated the following local packages: facet-json-read, facet-json-write

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-json-v0.1.9...facet-json-v0.1.10) - 2025-04-10

### Other

- updated the following local packages: facet-json-read, facet-json-write

## [0.1.9](https://github.com/facet-rs/facet/compare/facet-json-v0.1.8...facet-json-v0.1.9) - 2025-04-10

### Other

- updated the following local packages: facet-json-read, facet-json-write

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-json-v0.1.7...facet-json-v0.1.8) - 2025-04-10

### Other

- updated the following local packages: facet-json-read, facet-json-write

## [0.1.7](https://github.com/facet-rs/facet/compare/facet-json-v0.1.6...facet-json-v0.1.7) - 2025-04-10

### Fixed

- fix readmes

### Other

- remove spacing
- no height
- Update Readmes with logos.

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-json-v0.1.5...facet-json-v0.1.6) - 2025-04-10

### Other

- Include the README in facet-json

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-json-v0.1.4...facet-json-v0.1.5) - 2025-04-10

### Other

- updated the following local packages: facet-json-read, facet-json-write

## [0.1.4](https://github.com/facet-rs/facet/compare/facet-json-v0.1.3...facet-json-v0.1.4) - 2025-04-09

### Other

- updated the following local packages: facet-json-read, facet-json-write
