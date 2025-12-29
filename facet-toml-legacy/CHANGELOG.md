# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.30.0](https://github.com/facet-rs/facet-toml/compare/v0.29.0...v0.30.0) - 2025-10-18

### Other

- Upgrade to facet 0.30.0

## [0.29.0](https://github.com/facet-rs/facet-toml/compare/v0.25.18...v0.29.0) - 2025-09-20

### Fixed

- Steal field from default struct when struct has default attr

### Other

- Update facet to 0.29
- Remove unused dependencies
- Update README
- Initial import in own repository

## [0.25.18](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.17...facet-toml-v0.25.18) - 2025-07-03

### Other

- Rename SmartPointer to Pointer

## [0.25.17](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.16...facet-toml-v0.25.17) - 2025-06-30

### Other

- Bump to codspeed-divan-compat 3.0.1 (for windows build)
- No rust-cache for limpid run + upgrade codspeed

## [0.25.16](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.15...facet-toml-v0.25.16) - 2025-06-26

### Other

- Fix `#[facet(default = ...)]` in facet-toml
- Remove ScalarDef
- Remove PartialEq implementations
- Apply modern clippy fixes (mostly format strings)

## [0.25.15](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.14...facet-toml-v0.25.15) - 2025-06-17

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.14](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.13...facet-toml-v0.25.14) - 2025-06-15

### Added

- add 128 bit integer support in facet-json and facet-toml

### Other

- s/Wip/Partial
- ser: KeyStack: display root with guillemet chars
- ser: impl Display for KeyStack
- ser: init newtype KeyStack

## [0.25.13](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.12...facet-toml-v0.25.13) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.12](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.11...facet-toml-v0.25.12) - 2025-06-04

### Other

- tests: deser: add missing vec_struct test

## [0.25.11](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.10...facet-toml-v0.25.11) - 2025-06-03

### Other

- Add discord logo + link
- ser: TomlSerializer: push_key: replace unwrap with expect
- ser: remove needless import of Borrow trait
- ser: TomlSerializer: push_key can coerce an argument
- ser: TomlSerializer: move traces to avoid debug-only args

## [0.25.10](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.9...facet-toml-v0.25.10) - 2025-06-02

### Other

- Support (de)serializing arrays of tables
- Migrate push_ methods to begin_ convention in facet-reflect

## [0.25.9](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.8...facet-toml-v0.25.9) - 2025-05-31

### Other

- Fix facet-toml and facet-urlencoded tests
- YAML transparent types fixes
- More facet-yaml test fixes
- Fix facet-toml benchmark
- wow everything typechecks
- wip toml
- Remove yansi
- Start porting old reflect tests
- begin/end is more intuitive than push/pop
- Rename some methods
- Partially uninit struct

## [0.25.8](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.7...facet-toml-v0.25.8) - 2025-05-27

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.7](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.6...facet-toml-v0.25.7) - 2025-05-26

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.6](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.5...facet-toml-v0.25.6) - 2025-05-24

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.5](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.4...facet-toml-v0.25.5) - 2025-05-21

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.4](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.3...facet-toml-v0.25.4) - 2025-05-20

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.3](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.2...facet-toml-v0.25.3) - 2025-05-18

### Other

- Introduce `'shape` lifetime, allowing non-'static shapes.

## [0.25.2](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.1...facet-toml-v0.25.2) - 2025-05-16

### Other

- Port facet-toml tests to the test attribute

## [0.25.1](https://github.com/facet-rs/facet/compare/facet-toml-v0.25.0...facet-toml-v0.25.1) - 2025-05-13

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.24.5](https://github.com/facet-rs/facet/compare/facet-toml-v0.24.4...facet-toml-v0.24.5) - 2025-05-12

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.24.4](https://github.com/facet-rs/facet/compare/facet-toml-v0.24.3...facet-toml-v0.24.4) - 2025-05-12

### Other

- *(toml)* replace loop with an iterator and remove unneeded clone

## [0.24.3](https://github.com/facet-rs/facet/compare/facet-toml-v0.24.2...facet-toml-v0.24.3) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.24.2](https://github.com/facet-rs/facet/compare/facet-toml-v0.24.1...facet-toml-v0.24.2) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.24.1](https://github.com/facet-rs/facet/compare/facet-toml-v0.24.0...facet-toml-v0.24.1) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.22.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.21.0...facet-toml-v0.22.0) - 2025-05-06

### Added

- *(toml)* implement most of serialization
- *(toml)* add `facet_toml::to_string` serialize short form
- *(serialize)* add more optional start- and end-serialize calls

### Fixed

- *(toml)* support char in deserialize
- *(toml)* support u128 and i128 in deserialize
- *(toml)* support unit type
- *(toml)* [**breaking**] error handling for serialize
- *(toml)* implement none handling

### Other

- *(toml)* cleanup serialize implementation
- *(toml)* add wide benchmark
- *(deps)* cleanup unused dependecies with cargo-machete
- *(serialize)* make end_* trait methods optional
- *(serialize)* optionally widen-cast number types to u64 in trait
- Upgrade dependencies

## [0.21.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.20.0...facet-toml-v0.21.0) - 2025-05-02

### Other

- Do compile-time check of default impl
- JSON facet-serialize?
- Make msrv pass
- *(toml)* use facet-serialize

## [0.20.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.19.0...facet-toml-v0.20.0) - 2025-04-29

### Other

- Post-quote cleanups

## [0.19.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.18.1...facet-toml-v0.19.0) - 2025-04-27

### Added

- *(toml)* implement 'default' attribute

### Other

- *(toml)* [**breaking**] setup structure for serialize implementation

## [0.15.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.14.0...facet-toml-v0.15.0) - 2025-04-23

### Added

- *(toml)* include reflect path in error

### Fixed

- *(toml)* ensure alloc is properly used and deny unsafe code

### Other

- WIP
- Back to depot runners.
- *(toml)* set authors to me and @fasterthanlime

## [0.14.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.13.0...facet-toml-v0.14.0) - 2025-04-21

### Added

- *(toml)* pretty error handling with a nice report
- *(toml)* start properly handling errors

### Fixed

- *(toml)* parse all types implementing FromStr and improve errors
- *(toml)* handle option cases in enums

### Other

- *(toml)* use `Facet` display implementation instead of creating wrapper type
- *(toml)* improve coverage

## [0.3.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.2.0...facet-toml-v0.3.0) - 2025-04-12

### Other

- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- TOML enum with unit variant implementation ([#168](https://github.com/facet-rs/facet/pull/168))

## [0.2.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.12...facet-toml-v0.2.0) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.11...facet-toml-v0.1.12) - 2025-04-11

### Other

- Revert to facet-{core,derive,reflect} deps, closes #156 ([#159](https://github.com/facet-rs/facet/pull/159))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- wip reflect ([#155](https://github.com/facet-rs/facet/pull/155))

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.10...facet-toml-v0.1.11) - 2025-04-11

### Other

- Remove workspace dependencies
- Move the template files next to their output and improve the output of the facet-codegen crate.

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.9...facet-toml-v0.1.10) - 2025-04-11

### Other

- Logo credit

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.7...facet-toml-v0.1.8) - 2025-04-10

### Other

- PokeUninit / Poke

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.5...facet-toml-v0.1.6) - 2025-04-10

### Other

- updated the following local packages: facet-core, facet-poke, facet-derive

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.4...facet-toml-v0.1.5) - 2025-04-10

### Other

- Use put rather than write for all users of PokeValue
- rename pokevalue:: put into pokevalue:: write and provide a safe alternative
- introduces put in poke value which is safe

## [0.1.4](https://github.com/facet-rs/facet/releases/tag/facet-toml-v0.1.4) - 2025-04-10

### Added

- *(toml)* Add facet-toml crate

### Other

- Woops, use new everywhere
- A few fixups
