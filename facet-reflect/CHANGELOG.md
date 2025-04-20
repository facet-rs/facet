# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.10.3](https://github.com/facet-rs/facet/compare/facet-reflect-v0.10.2...facet-reflect-v0.10.3) - 2025-04-20

### Other

- Don't allocate strings in facet-json deserialization unless necessary
- Refactor JSON number deserialization to use Wip::try_put_f64

## [0.10.2](https://github.com/facet-rs/facet/compare/facet-reflect-v0.10.1...facet-reflect-v0.10.2) - 2025-04-19

### Added

- *(json)* Support default attribute.
- feat(json) Support default at the container level
- feat(json) Better error messages when a field is missing

## [0.10.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.10.0...facet-reflect-v0.10.1) - 2025-04-19

### Added

- feat(json) Support deny_unknown_fields

## [0.10.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.9.1...facet-reflect-v0.10.0) - 2025-04-18

### Other

- Never restore state when pushing a map key and also attempt not to track them.

## [0.9.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.9.0...facet-reflect-v0.9.1) - 2025-04-18

### Other

- Attempt to set up release-plz again

## [0.6.2](https://github.com/facet-rs/facet/compare/facet-reflect-v0.6.1...facet-reflect-v0.6.2) - 2025-04-12

### Added

- *(reflect)* add `ScalarType` enum ([#173](https://github.com/facet-rs/facet/pull/173))

### Other

- Impl `Facet` for `Arc<T>` ([#180](https://github.com/facet-rs/facet/pull/180))
- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- Use anstyle ([#170](https://github.com/facet-rs/facet/pull/170))
- Opaque initialization of Some ([#169](https://github.com/facet-rs/facet/pull/169))
- TOML enum with unit variant implementation ([#168](https://github.com/facet-rs/facet/pull/168))

## [0.6.1](https://github.com/facet-rs/facet/compare/facet-reflect-v0.6.0...facet-reflect-v0.6.1) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.6.0](https://github.com/facet-rs/facet/compare/facet-reflect-v0.5.0...facet-reflect-v0.6.0) - 2025-04-11

### Changed
- Merged `facet-peek` and `facet-poke` into `facet-reflect`
- Combined functionality for reading and writing Facet types