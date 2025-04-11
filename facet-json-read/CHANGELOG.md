# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.14](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.13...facet-json-read-v0.1.14) - 2025-04-11

### Other

- Remove workspace dependencies
- Move the template files next to their output and improve the output of the facet-codegen crate.

## [0.1.13](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.12...facet-json-read-v0.1.13) - 2025-04-11

### Other

- Logo credit

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.11...facet-json-read-v0.1.12) - 2025-04-11

### Other

- update Cargo.toml dependencies

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.10...facet-json-read-v0.1.11) - 2025-04-10

### Other

- Re-organize poke tests, add alloc lints, thanks @epage for the hint
- PokeUninit / Poke
- Introduce a PokeValueUninit / PokeValue chasm

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.9...facet-json-read-v0.1.10) - 2025-04-10

### Other

- Fix signed integer deserialization
- Improve integer deserialization
- NonZero support

## [0.1.9](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.8...facet-json-read-v0.1.9) - 2025-04-10

### Other

- Update documentation for expect_array_start
- Fix deserialization of empty JSON arrays

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.7...facet-json-read-v0.1.8) - 2025-04-10

### Other

- updated the following local packages: facet-core, facet-poke, facet-derive

## [0.1.7](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.6...facet-json-read-v0.1.7) - 2025-04-10

### Other

- Use put rather than write for all users of PokeValue
- rename pokevalue:: put into pokevalue:: write and provide a safe alternative
- introduces put in poke value which is safe

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.5...facet-json-read-v0.1.6) - 2025-04-10

### Fixed

- fix readmes

### Other

- remove spacing
- no height
- Update Readmes with logos.

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.4...facet-json-read-v0.1.5) - 2025-04-10

### Other

- updated the following local packages: facet-trait, facet-derive, facet-poke

## [0.1.4](https://github.com/facet-rs/facet/compare/facet-json-read-v0.1.3...facet-json-read-v0.1.4) - 2025-04-09

### Other

- updated the following local packages: facet-trait, facet-derive, facet-poke

## [0.1.3](https://github.com/facet-rs/facet/releases/tag/facet-json-read-v0.1.3) - 2025-04-08

### Other

- Complete nostd
- More nostd
- Less experimental now
- non-exhaustive enums
- miri fixes
- Update
- Update
- comment out test for the time being
- mhmh
- HashMaps work?
- Fix CI
- Deserializing vecs from json seems to work
- oh mh
- clean up list/map
- No cloning PokeStruct
- woahey it deserializes
- sigabrt is not nothing, y'know
- identify bug
- Show proper error
- Split json-read and json-write

## [0.1.0](https://github.com/facet-rs/facet/releases/tag/facet-json-read-v0.1.0) - 2025-04-08

### Other

- comment out test for the time being
- mhmh
- HashMaps work?
- Fix CI
- Deserializing vecs from json seems to work
- oh mh
- clean up list/map
- No cloning PokeStruct
- woahey it deserializes
- sigabrt is not nothing, y'know
- identify bug
- Show proper error
- Split json-read and json-write
