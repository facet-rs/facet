# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Add support for `Shape::type_tag`

## [0.27.7](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.27.6...facet-derive-emit-v0.27.7) - 2025-05-27

### Other

- More lenient try_from_inner

## [0.27.5](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.27.4...facet-derive-emit-v0.27.5) - 2025-05-24

### Other

- Add `Shape.type_identifier` to access type name in const contexts
- Fix cyclic types with indirection for optional fns in `ValueVTable`

## [0.27.2](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.27.1...facet-derive-emit-v0.27.2) - 2025-05-18

### Other

- Introduce `'shape` lifetime, allowing non-'static shapes.

## [0.27.1](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.27.0...facet-derive-emit-v0.27.1) - 2025-05-16

### Other

- Rust 1.87 clippy fixes
- implement recursive serialize

## [0.27.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.26.1...facet-derive-emit-v0.27.0) - 2025-05-13

### Other

- More tests, which also pass
- Fix more tests
- Support Arc<T> where T: ?Sized

## [0.26.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.25.1...facet-derive-emit-v0.26.0) - 2025-05-12

### Other

- Make default fields with a lifetime work

## [0.25.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.24.0...facet-derive-emit-v0.25.0) - 2025-05-10

### Other

- Better discriminant codegen, closes #563
- Allow macros in enum discriminants

## [0.20.3](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.20.2...facet-derive-emit-v0.20.3) - 2025-05-10

### Added

- Allow empty string rename values

### Fixed

- Empty string rename values are now compile errors

### Other

- Release facet-derive-parser
- Fix slow tests
- Rework type information (Def)

## [0.20.2](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.20.1...facet-derive-emit-v0.20.2) - 2025-05-08

### Other

- updated the following local packages: facet-derive-parse

## [0.20.1](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.20.0...facet-derive-emit-v0.20.1) - 2025-05-06

### Other

- updated the following local packages: facet-derive-parse

## [0.20.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.19.0...facet-derive-emit-v0.20.0) - 2025-05-02

### Fixed

- *(derive)* proc macro generation for enums with empty struct variants or lifetime fields

### Other

- Do compile-time check of default impl
- JSON facet-serialize?
- Don't depend on git version of insta anymore

## [0.19.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.18.2...facet-derive-emit-v0.19.0) - 2025-04-29

### Other

- Reduce duplication in derive macro
- Make sure attributes parse correctly
- Post-quote cleanups
- Fix tests
- Use quote some more
- Start using quote
- Don't confuse repr(transparent) and facet(transparent)
- Parse some more things (struct kind)
- Parse everything about structs/containers
- Used parsed enums
- Introduce EnumParams, parsed structs
- Add support for rename_all and arbitrary attributes on variants
- split massive match statement into several functions
- remove dbgs
- allow enum variant attributes

## [0.18.2](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.18.1...facet-derive-emit-v0.18.2) - 2025-04-27

### Other

- update Cargo.toml dependencies

## [0.18.1](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.18.0...facet-derive-emit-v0.18.1) - 2025-04-26

### Added

- Add support for rename_all on containers

## [0.15.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.14.0...facet-derive-emit-v0.15.0) - 2025-04-23

### Other

- Fix doc tests (by removing them)
- Fix unit test
- Last few cleanups
- Update snapshots
- bgp.as_phantom_data()
- 308 errors new record woo
- Use bgp throughout the codebase
- Add tests for boundedgenericparams
- BoundedGenericParams
- Fix support for C-style enums with the derive macro.
- Fix invariance for lifetime paramters in derive
- Update snapshots
- Update snapshots
- Clippy fixes
- WIP
- Back to depot runners.

## [0.14.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.13.0...facet-derive-emit-v0.14.0) - 2025-04-21

### Other

- replace format! with format_args! where ever possible
- actually run compile test for skip_serializing_if
- Implement the skip_serializing/skip_serializing_if attribute

## [0.13.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.12.0...facet-derive-emit-v0.13.0) - 2025-04-20

### Added

- *(derive)* Support facet(transparent) attr on containers

## [0.12.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.11.0...facet-derive-emit-v0.12.0) - 2025-04-19

### Added

- *(json)* Support default attribute.
- feat(json) Support default at the container level

## [0.11.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.10.1...facet-derive-emit-v0.11.0) - 2025-04-19

### Added

- feat(json) Support deny_unknown_fields

## [0.10.1](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.10.0...facet-derive-emit-v0.10.1) - 2025-04-18

### Other

- Support hex literals

## [0.10.0](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.9.1...facet-derive-emit-v0.10.0) - 2025-04-18

### Other

- Allow manually defining enum discriminants

## [0.9.1](https://github.com/facet-rs/facet/compare/facet-derive-emit-v0.9.0...facet-derive-emit-v0.9.1) - 2025-04-18

### Other

- update Cargo.toml dependencies
