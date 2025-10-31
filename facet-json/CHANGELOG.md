# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.31.0](https://github.com/facet-rs/facet-json/compare/v0.30.0...v0.31.0) - 2025-10-31

### Other

- Update deps, tests pass
- Use facet 0.31.2
- Testing facet 0.31.1

## [0.30.0](https://github.com/facet-rs/facet-json/compare/v0.29.1...v0.30.0) - 2025-10-18

### Other

- Upgrade to facet 0.30.0

## [0.29.1](https://github.com/facet-rs/facet-json/compare/v0.29.0...v0.29.1) - 2025-09-20

### Other

- Demote log::debug! statements to log::trace!

## [0.29.0](https://github.com/facet-rs/facet-json/compare/v0.28.2...v0.29.0) - 2025-09-11

### Other

- Upgrade to facet 0.29

## [0.28.2](https://github.com/facet-rs/facet-json/compare/v0.28.1...v0.28.2) - 2025-08-15

### Other

- Parse surrogate pairs, closes #14

## [0.28.1](https://github.com/facet-rs/facet-json/compare/v0.28.0...v0.28.1) - 2025-07-25

### Other

- Allow building without default-features

## [0.28.0](https://github.com/facet-rs/facet-json/compare/v0.24.18...v0.28.0) - 2025-07-25

### Other

- facet-dev generate
- Set rust-version to 1.87
- facet-dev generate
- Initial import in own repository

## [0.24.18](https://github.com/facet-rs/facet/compare/facet-json-v0.24.17...facet-json-v0.24.18) - 2025-07-03

### Other

- Rename SmartPointer to Pointer
- Allow serializing references
- Revert "Add inner types for reference"
- Add inner types for reference
- Use from-lexical for i64 and u64 as well
- Use lexical-parse-float in parse_number
- Optimize parse_number
- Remove args-specific stuff from deserialize
- Add tests for #609 (deserialize into slices)
- Add HashMap<u32, u32> test, closes #782

## [0.24.17](https://github.com/facet-rs/facet/compare/facet-json-v0.24.16...facet-json-v0.24.17) - 2025-06-30

### Other

- Generate string keys for all numbers (except floats, which can't work as map keys)
- write json key as string for i32 numbers

## [0.24.16](https://github.com/facet-rs/facet/compare/facet-json-v0.24.15...facet-json-v0.24.16) - 2025-06-26

### Other

- Add support for `Arc<[U]>` in facet-core and facet-reflect
- impl Facet for Arc<str>, Rc<str>, and Box<str>
- Fix HashSet serializing to null due to Set being an unhandled type
- Remove ScalarDef
- Apply modern clippy fixes (mostly format strings)

## [0.24.15](https://github.com/facet-rs/facet/compare/facet-json-v0.24.14...facet-json-v0.24.15) - 2025-06-17

### Other

- updated the following local packages: facet-core, facet-core, facet-reflect, facet-deserialize, facet-serialize

## [0.24.14](https://github.com/facet-rs/facet/compare/facet-json-v0.24.13...facet-json-v0.24.14) - 2025-06-15

### Added

- add 128 bit integer support in facet-json and facet-toml

### Other

- actually use the cpeek instead of the main peek when serializing arrays

## [0.24.13](https://github.com/facet-rs/facet/compare/facet-json-v0.24.12...facet-json-v0.24.13) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-core, facet-reflect, facet-deserialize, facet-serialize

## [0.24.12](https://github.com/facet-rs/facet/compare/facet-json-v0.24.11...facet-json-v0.24.12) - 2025-06-04

### Other

- Fix ASCII control character hex escaping bug and add comprehensive tests

## [0.24.11](https://github.com/facet-rs/facet/compare/facet-json-v0.24.10...facet-json-v0.24.11) - 2025-06-03

### Other

- Add discord logo + link
- Fix JSON serialization of &[u8] slices

## [0.24.10](https://github.com/facet-rs/facet/compare/facet-json-v0.24.9...facet-json-v0.24.10) - 2025-06-02

### Other

- add itoa
- no_std facet-json
- move to self-owned write trait
- Use a Vec instead of a Writer for the json serialializer
- Allow transparent key types
- switch to ryu for float formatting
- add tokenizer test, fix tokenizer using said test
- cow tokens
- expand flamegraph using inline never
- apply a windowed approach to the tokenizer
- split out parse_char
- remove copying of whole buffer from tokenizer
- Reduce indexing in `write_json_string`

## [0.24.9](https://github.com/facet-rs/facet/compare/facet-json-v0.24.8...facet-json-v0.24.9) - 2025-05-31

### Fixed

- fix more oopsie
- fix stupid error

### Other

- Simplify code for set_numeric_value
- properly check whether the top three bits are set, indicating that a character is not a control character
- Fix some clippy errors
- Add serialization for box
- Resolve warnings etc.
- Tests pass again
- add chrono support
- opt for more complicated bit fiddling that actually works
- facet-json is not _currently_ nostd, actually, because of std::io::Write
- rename some stuff
- bit mask type can be inferred
- add directors commentary to the freshly-mangled write_json_string function
- more bitwise escaping
- try u128
- respect utf8 char boundaries
- let's actually close parens
- maybe even faster???
- facet-json tests pass
- Fix tests
- Tuple handling
- reorder match arms for speeeed
- inline write_json_string and write_json_escaped_char
- testeroni was bad
- another testeroni
- maybe make write_json_escaped_char faster
- More facet-json tests
- Some json fixes
- wow everything typechecks
- facet-deserialize fixes
- Deinitialization is wrong (again)

## [0.24.8](https://github.com/facet-rs/facet/compare/facet-json-v0.24.7...facet-json-v0.24.8) - 2025-05-27

### Other

- More lenient try_from_inner

## [0.24.7](https://github.com/facet-rs/facet/compare/facet-json-v0.24.6...facet-json-v0.24.7) - 2025-05-26

### Other

- updated the following local packages: facet-core, facet-core, facet-reflect, facet-deserialize, facet-serialize

## [0.24.6](https://github.com/facet-rs/facet/compare/facet-json-v0.24.5...facet-json-v0.24.6) - 2025-05-24

### Other

- Fix cyclic types with indirection for optional fns in `ValueVTable`
- Add test case for deserializing `bytes::Bytes`
- Add test case for JSON deserializing nested options
- Add `bytes` feature with impls for `Bytes`/`BytesMut`

## [0.24.5](https://github.com/facet-rs/facet/compare/facet-json-v0.24.4...facet-json-v0.24.5) - 2025-05-21

### Other

- Support deserializing to `Arc<T>`

## [0.24.4](https://github.com/facet-rs/facet/compare/facet-json-v0.24.3...facet-json-v0.24.4) - 2025-05-20

### Added

- *(args)* arg-wise spans for reflection errors; ToCooked trait

### Other

- cfg(not(miri))
- Show warning on truncation
- Truncate when showing errors in one long JSON line

## [0.24.3](https://github.com/facet-rs/facet/compare/facet-json-v0.24.2...facet-json-v0.24.3) - 2025-05-18

### Other

- Introduce `'shape` lifetime, allowing non-'static shapes.

## [0.24.2](https://github.com/facet-rs/facet/compare/facet-json-v0.24.1...facet-json-v0.24.2) - 2025-05-16

### Added

- facet-args `Cli` trait impl; deserialize `&str` field
- *(deserialize)* support multiple input types via generic `Format`

### Other

- Rust 1.87 clippy fixes
- Relax facet-json lifetime requirements
- Re-export DeserError, DeserErrorKind, etc.
- Fix msrv
- almost fix everything
- implement recursive serialize
- Use test attribute for facet-json tests
- Introduce facet_testhelpers::test attribute
- Fix json tests
- Clean tests up, re-enable json tests
- allow deserializing from number in JSON

## [0.24.1](https://github.com/facet-rs/facet/compare/facet-json-v0.24.0...facet-json-v0.24.1) - 2025-05-13

### Other

- Fix enum tests with a single tuple field
- Well it says the field is not initialized, so.

## [0.23.6](https://github.com/facet-rs/facet/compare/facet-json-v0.23.5...facet-json-v0.23.6) - 2025-05-12

### Other

- updated the following local packages: facet-core, facet-core, facet-reflect, facet-deserialize, facet-serialize

## [0.23.5](https://github.com/facet-rs/facet/compare/facet-json-v0.23.4...facet-json-v0.23.5) - 2025-05-12

### Added

- *(core)* add core implementation for `jiff::civil::DateTime`
- *(core)* add core implementation for `jiff::Timestamp`
- *(core)* add core implementation for `jiff::Zoned`

### Other

- Re-export DeserError
- Disable zoned test under miri
- Rename jiff feature to jiff02 (thanks @BurntSushi)
- Fix memory leaks, add more tests
- Add JSON test cases for Camino/ULID/UUID
- Add support for time crate's OffsetDateTime and UtcDateTime

## [0.23.4](https://github.com/facet-rs/facet/compare/facet-json-v0.23.3...facet-json-v0.23.4) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize, facet-serialize

## [0.23.3](https://github.com/facet-rs/facet/compare/facet-json-v0.23.2...facet-json-v0.23.3) - 2025-05-10

### Other

- Add support for partially initializing arrays, closes #504

## [0.23.2](https://github.com/facet-rs/facet/compare/facet-json-v0.23.1...facet-json-v0.23.2) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize, facet-serialize

## [0.23.1](https://github.com/facet-rs/facet/compare/facet-json-v0.23.0...facet-json-v0.23.1) - 2025-05-10

### Added

- Allow empty string rename values

### Fixed

- Add support for Unicode escape sequences in JSON strings

### Other

- Release facet-reflect
- Release facet-derive-parser
- Upgrade facet-core
- Fix additional tests
- Determine enum variant after default_from_fn
- Uncomment deserialize

## [0.23.0](https://github.com/facet-rs/facet/compare/facet-json-v0.22.0...facet-json-v0.23.0) - 2025-05-08

### Other

- *(deserialize)* [**breaking**] make deserialize format stateful
