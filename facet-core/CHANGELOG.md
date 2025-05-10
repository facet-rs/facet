# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.22.0](https://github.com/facet-rs/facet/compare/facet-core-v0.21.1...facet-core-v0.22.0) - 2025-05-10

### Added

- support core::ops::Range

### Fixed

- Facet impl for core::ops::Range and add a test

### Other

- References don't have default, too (shame but)
- Clippy warnings
- references are not pointers
- Fix ops::Range implementation
- Determine enum variant after default_from_fn
- Uncomment deserialize
- Make variant() getters fallible — we might not know the internal enough to read the discriminant etc.
- Rework type information (Def)

## [0.21.1](https://github.com/facet-rs/facet/compare/facet-core-v0.21.0...facet-core-v0.21.1) - 2025-05-08

### Other

- Fix `get_item_ptr` for arrays

## [0.21.0](https://github.com/facet-rs/facet/compare/facet-core-v0.20.0...facet-core-v0.21.0) - 2025-05-06

### Other

- Make Opaque<T>(T)'s field public, closes #521
- Get started on an event-based approach to facet-deserialize ([#500](https://github.com/facet-rs/facet/pull/500))

## [0.20.0](https://github.com/facet-rs/facet/compare/facet-core-v0.19.0...facet-core-v0.20.0) - 2025-05-02

### Other

- Fix clone_into for collections
- JSON facet-serialize?
- explain more macro stuff
- support camino's &Utf8Path and Utf8PathBuf
- Fix clone_into functions

## [0.19.0](https://github.com/facet-rs/facet/compare/facet-core-v0.18.0...facet-core-v0.19.0) - 2025-04-29

### Other

- Make sure attributes parse correctly
- Post-quote cleanups
- Add support for rename_all and arbitrary attributes on variants
- allow enum variant attributes
- support serialize flattening

## [0.12.0](https://github.com/facet-rs/facet/compare/facet-core-v0.11.0...facet-core-v0.12.0) - 2025-04-23

### Other

- Fix camino implementations
- Clippy fixes
- WIP
- Back to depot runners.
- *(deps)* update dependencies

## [0.11.0](https://github.com/facet-rs/facet/compare/facet-core-v0.10.1...facet-core-v0.11.0) - 2025-04-21

### Fixed

- fix!(core): Fix inconsistent naming

### Other

- Implement `Facet` for (subset of) function pointers
- Support field-level default
- Implement the skip_serializing/skip_serializing_if attribute
- Add tests for `Rc`'s and `Arc`'s smart pointer VTables
- Impl `Facet` for `Rc<T>`
- Fix MSRV test
- Add missing ToOwned import
- Add getters to Shape & Field
- Improve number handling for JSON deserialization
- option and number
- very nice error reporting as it turns out
- Use TryFrom to deserialize NonZero<T>

## [0.10.1](https://github.com/facet-rs/facet/compare/facet-core-v0.10.0...facet-core-v0.10.1) - 2025-04-20

### Other

- Let Utf8PathBuf implement Parse

## [0.10.0](https://github.com/facet-rs/facet/compare/facet-core-v0.9.1...facet-core-v0.10.0) - 2025-04-19

### Added

- *(json)* Support default attribute.
- feat(json) Support default at the container level

## [0.9.1](https://github.com/facet-rs/facet/compare/facet-core-v0.9.0...facet-core-v0.9.1) - 2025-04-19

### Added

- feat(json) Support deny_unknown_fields

## [0.5.3](https://github.com/facet-rs/facet/compare/facet-core-v0.5.2...facet-core-v0.5.3) - 2025-04-12

### Other

- Impl `Facet` for `Arc<T>` ([#180](https://github.com/facet-rs/facet/pull/180))
- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- Split facet-core/types into multiple modules, prepare for Arc<T> etc. ([#174](https://github.com/facet-rs/facet/pull/174))
- Fix minor typo ([#176](https://github.com/facet-rs/facet/pull/176))

## [0.5.2](https://github.com/facet-rs/facet/compare/facet-core-v0.5.1...facet-core-v0.5.2) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.5.1](https://github.com/facet-rs/facet/compare/facet-core-v0.5.0...facet-core-v0.5.1) - 2025-04-11

### Other

- Derive Facet for #[repr(C)] enums (merged) ([#163](https://github.com/facet-rs/facet/pull/163))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- wip reflect ([#155](https://github.com/facet-rs/facet/pull/155))
- Support generic ADTs ([#130](https://github.com/facet-rs/facet/pull/130))
- Return error instead of panicking in set/set_by_name ([#147](https://github.com/facet-rs/facet/pull/147))

## [0.5.0](https://github.com/facet-rs/facet/compare/facet-core-v0.4.2...facet-core-v0.5.0) - 2025-04-11

### Other

- support only primitive repr and make derive stricter. ([#139](https://github.com/facet-rs/facet/pull/139))

## [0.4.2](https://github.com/facet-rs/facet/compare/facet-core-v0.4.1...facet-core-v0.4.2) - 2025-04-11

### Added

- *(core)* Allow use with just alloc

### Fixed

- *(core)* Allow SocketAddr without std

### Other

- Fix docs errors
- Automatically patch generated/expanded code
- Regen code
- Move the template files next to their output and improve the output of the facet-codegen crate.
- Add and commit sample_generated_code, that should build in docsrs
- Implement facet for char
- *(core)* Remove a redundant cfg
- *(core)* Centralize 'extern crate alloc'

## [0.4.1](https://github.com/facet-rs/facet/compare/facet-core-v0.4.0...facet-core-v0.4.1) - 2025-04-11

### Other

- Logo credit

## [0.4.0](https://github.com/facet-rs/facet/compare/facet-core-v0.3.3...facet-core-v0.4.0) - 2025-04-10

### Other

- Re-organize poke tests, add alloc lints, thanks @epage for the hint
- Introduce a PokeValueUninit / PokeValue chasm
- Option manipulation
- option vtable

## [0.3.3](https://github.com/facet-rs/facet/compare/facet-core-v0.3.2...facet-core-v0.3.3) - 2025-04-10

### Other

- Inline macros into derive macros, use gen_struct_field for enums fields as well
- failing tests re: enum doc comments
- Unify unit struct, tuple struct, struct processing
- Capture struct field doc comments
- Process doc comments simply as a slice of stringsl
- Basic doc grabbing but I imagine we're not out of the woods yet

## [0.3.2](https://github.com/facet-rs/facet/compare/facet-core-v0.3.1...facet-core-v0.3.2) - 2025-04-10

### Other

- Make shape & friends repr(C)
- enums are peekable 😎
- Peek for unit structs
- holy ship it works
- Start peeking/poking enums

## [0.3.1](https://github.com/facet-rs/facet/compare/facet-core-v0.3.0...facet-core-v0.3.1) - 2025-04-10

### Fixed

- fix undefined behavior in `Shape::allocate`
- fix debug impl, add missing display impl for arrays

### Other

- Generalize `Facet` array impl to arbitrary lengths
- Add codegen instructions to the template

## [0.3.0](https://github.com/facet-rs/facet/compare/facet-core-v0.2.5...facet-core-v0.3.0) - 2025-04-10

### Other

- Add no_std support
- Add ScalarAffinity type and update implementations
- Use TypeId for every types, not just scalar. Closes #97
- Revert 9b8904f
- Use put rather than write for all users of PokeValue
- introduces put in poke value which is safe

## [0.2.5](https://github.com/facet-rs/facet/releases/tag/facet-core-v0.2.5) - 2025-04-10

### Other

- Impl Facet for ScalarDef
- impl Facet for ScalarId
- Merge branch 'main' into from-ptr
- Replace `ARCHETYPE` const with `SpezEmpty` type wrapper
- Mark unsafe spez methods as unsafe, closes #82
- Merge facet-opaque, facet-spez, facet-types and facet-trait back into facet-core, to allow implementing Facet for Shape
