# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.30.0](https://github.com/facet-rs/facet-yaml/compare/v0.29.1...v0.30.0) - 2025-10-18

### Other

- Upgrade to facet 0.30.0

## [0.29.1](https://github.com/facet-rs/facet-yaml/compare/v0.29.0...v0.29.1) - 2025-09-18

### Fixed

- Re-implement 'get field values from Default struct'

## [0.29.0](https://github.com/facet-rs/facet-yaml/compare/v0.28.0...v0.29.0) - 2025-09-11

### Breaking changes

- Regression: `#[default]` on a struct will not allow filling missing fields
  with the corresponding field from `Default::default()` of the struct anymore.
  The previous implementation was unsound. This functionality will be restored
  in a future version.

### Other

- Upgrade to facet 0.29

## [0.28.0](https://github.com/facet-rs/facet-yaml/compare/v0.25.16...v0.28.0) - 2025-07-25

### Other

- Initial import in its own repository

## [0.25.16](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.15...facet-yaml-v0.25.16) - 2025-07-03

### Other

- Rename SmartPointer to Pointer
- Fix some clippy warnings

## [0.25.15](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.14...facet-yaml-v0.25.15) - 2025-06-30

### Other

- Apply clippy fixes

## [0.25.14](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.13...facet-yaml-v0.25.14) - 2025-06-26

### Other

- Add support for `Arc<[U]>` in facet-core and facet-reflect
- Remove ScalarDef
- Apply modern clippy fixes (mostly format strings)

## [0.25.13](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.12...facet-yaml-v0.25.13) - 2025-06-17

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.12](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.11...facet-yaml-v0.25.12) - 2025-06-15

### Added

- add 128 bit integer support in facet-json and facet-toml

## [0.25.11](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.10...facet-yaml-v0.25.11) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.10](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.9...facet-yaml-v0.25.10) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.9](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.8...facet-yaml-v0.25.9) - 2025-06-03

### Other

- Add discord logo + link

## [0.25.8](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.7...facet-yaml-v0.25.8) - 2025-06-02

### Other

- Migrate push_ methods to begin_ convention in facet-reflect

## [0.25.7](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.6...facet-yaml-v0.25.7) - 2025-05-31

### Other

- YAML transparent types fixes
- More facet-yaml test fixes
- facet-json tests pass
- wow everything typechecks
- Remove yansi
- Start porting old reflect tests
- begin/end is more intuitive than push/pop
- Rename some methods

## [0.25.6](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.5...facet-yaml-v0.25.6) - 2025-05-27

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.5](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.4...facet-yaml-v0.25.5) - 2025-05-26

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.4](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.3...facet-yaml-v0.25.4) - 2025-05-24

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.3](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.2...facet-yaml-v0.25.3) - 2025-05-21

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.2](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.1...facet-yaml-v0.25.2) - 2025-05-20

### Other

- updated the following local packages: facet-core, facet-reflect, facet-serialize

## [0.25.1](https://github.com/facet-rs/facet/compare/facet-yaml-v0.25.0...facet-yaml-v0.25.1) - 2025-05-18

### Other

- Introduce `'shape` lifetime, allowing non-'static shapes.

## [0.25.0](https://github.com/facet-rs/facet/compare/facet-yaml-v0.24.6...facet-yaml-v0.25.0) - 2025-05-16

### Added

- *(yaml)* [**breaking**] implement serialize

### Other

- Make some dependencies dev & optional
- Use the test attribute for facet-urlencoded,xdr,yaml
- Add support for default values in YAML deserializer
- Add support for transparent newtypes in YAML deserializer
- Add support for YAML maps and lists
- Support OffsetDateTime in YAML

## [0.24.6](https://github.com/facet-rs/facet/compare/facet-yaml-v0.24.5...facet-yaml-v0.24.6) - 2025-05-13

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.5](https://github.com/facet-rs/facet/compare/facet-yaml-v0.24.4...facet-yaml-v0.24.5) - 2025-05-12

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.4](https://github.com/facet-rs/facet/compare/facet-yaml-v0.24.3...facet-yaml-v0.24.4) - 2025-05-12

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.3](https://github.com/facet-rs/facet/compare/facet-yaml-v0.24.2...facet-yaml-v0.24.3) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.2](https://github.com/facet-rs/facet/compare/facet-yaml-v0.24.1...facet-yaml-v0.24.2) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.1](https://github.com/facet-rs/facet/compare/facet-yaml-v0.24.0...facet-yaml-v0.24.1) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.24.0](https://github.com/facet-rs/facet/compare/facet-yaml-v0.18.3...facet-yaml-v0.24.0) - 2025-05-08

### Other

- *(yaml)* prepare for serialization implementation

## [0.18.3](https://github.com/facet-rs/facet/compare/facet-yaml-v0.18.2...facet-yaml-v0.18.3) - 2025-05-06

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.18.2](https://github.com/facet-rs/facet/compare/facet-yaml-v0.18.1...facet-yaml-v0.18.2) - 2025-05-02

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.18.1](https://github.com/facet-rs/facet/compare/facet-yaml-v0.18.0...facet-yaml-v0.18.1) - 2025-04-29

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.15.0](https://github.com/facet-rs/facet/compare/facet-yaml-v0.14.0...facet-yaml-v0.15.0) - 2025-04-23

### Other

- WIP
- Back to depot runners.
- *(deps)* update dependencies

## [0.3.0](https://github.com/facet-rs/facet/compare/facet-yaml-v0.2.0...facet-yaml-v0.3.0) - 2025-04-12

### Added

- *(reflect)* add `ScalarType` enum ([#173](https://github.com/facet-rs/facet/pull/173))

### Other

- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- Add most CI improvements from #166 ([#172](https://github.com/facet-rs/facet/pull/172))

## [0.2.0](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.12...facet-yaml-v0.2.0) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.11...facet-yaml-v0.1.12) - 2025-04-11

### Other

- Revert to facet-{core,derive,reflect} deps, closes #156 ([#159](https://github.com/facet-rs/facet/pull/159))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- wip reflect ([#155](https://github.com/facet-rs/facet/pull/155))

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.10...facet-yaml-v0.1.11) - 2025-04-11

### Other

- Remove workspace dependencies
- Move the template files next to their output and improve the output of the facet-codegen crate.

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.9...facet-yaml-v0.1.10) - 2025-04-11

### Other

- Logo credit

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.7...facet-yaml-v0.1.8) - 2025-04-10

### Other

- PokeUninit / Poke

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.5...facet-yaml-v0.1.6) - 2025-04-10

### Other

- updated the following local packages: facet-core, facet-poke, facet-derive

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.4...facet-yaml-v0.1.5) - 2025-04-10

### Other

- Use put rather than write for all users of PokeValue
- rename pokevalue:: put into pokevalue:: write and provide a safe alternative
- introduces put in poke value which is safe

## [0.1.4](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.3...facet-yaml-v0.1.4) - 2025-04-10

### Fixed

- fix readmes

### Other

- remove spacing
- no height
- Update Readmes with logos.

## [0.1.3](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.2...facet-yaml-v0.1.3) - 2025-04-09

### Other

- updated the following local packages: facet-trait, facet-derive

## [0.1.1](https://github.com/facet-rs/facet/compare/facet-yaml-v0.1.0...facet-yaml-v0.1.1) - 2025-04-08

### Other

- updated the following local packages: facet-trait, facet-derive, facet-poke

## [0.1.0](https://github.com/facet-rs/facet/releases/tag/facet-yaml-v0.1.0) - 2025-04-08

### Other

- WIP pretty
- Basic YAML support
- wip YAML
- not using namespace runners for now
- Wow, msgpack works again
- More READMEs
- quicksave
- more shapely => facet renames
- 0.1.0

## [3.1.0](https://github.com/facet-rs/facet/compare/facet-yaml-v3.0.0...facet-yaml-v3.1.0) - 2025-03-31

### Other

- Fill in missing docs
- Fix all markdown links
- shill for namespace, closes #36

## [2.0.1](https://github.com/facet-rs/facet/compare/facet-yaml-v2.0.0...facet-yaml-v2.0.1) - 2025-03-11

### Other

- Add examples to YAML and MessagePack crates, simplify READMEs, and fix doc tests
- Clippy fixes

## [2.0.0](https://github.com/facet-rs/facet/compare/facet-yaml-v1.0.0...facet-yaml-v2.0.0) - 2025-03-11

### Other

- Stub out facet-yaml
