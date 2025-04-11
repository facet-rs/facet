# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
