# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0](https://github.com/facet-rs/facet/compare/facet-poke-v0.5.2...facet-poke-v0.6.0) - 2025-04-11

### Added

- *(poke)* [**breaking**] Future proof for no-alloc support
- *(poke)* Allow use with just alloc

### Other

- Remove workspace dependencies
- Fix docs errors
- Move the template files next to their output and improve the output of the facet-codegen crate.

## [0.5.2](https://github.com/facet-rs/facet/compare/facet-poke-v0.5.1...facet-poke-v0.5.2) - 2025-04-11

### Other

- Logo credit

## [0.5.1](https://github.com/facet-rs/facet/compare/facet-poke-v0.5.0...facet-poke-v0.5.1) - 2025-04-11

### Other

- update Cargo.toml dependencies

## [0.5.0](https://github.com/facet-rs/facet/compare/facet-poke-v0.4.1...facet-poke-v0.5.0) - 2025-04-10

### Other

- Add option support to pretty-printer
- Full option support
- Re-organize poke tests, add alloc lints, thanks @epage for the hint
- PokeUninit / Poke
- Introduce a PokeValueUninit / PokeValue chasm
- replace_with for Option
- Option manipulation
- WIP option support
- option vtable

## [0.4.1](https://github.com/facet-rs/facet/compare/facet-poke-v0.4.0...facet-poke-v0.4.1) - 2025-04-10

### Other

- Basic doc grabbing but I imagine we're not out of the woods yet

## [0.4.0](https://github.com/facet-rs/facet/compare/facet-poke-v0.3.1...facet-poke-v0.4.0) - 2025-04-10

### Other

- Test enums
- Building enums works!
- enums are peekable 😎
- Peek for unit structs
- holy ship it works
- Fix type depth
- Start peeking/poking enums

## [0.3.1](https://github.com/facet-rs/facet/compare/facet-poke-v0.3.0...facet-poke-v0.3.1) - 2025-04-10

### Fixed

- fix undefined behavior in `Shape::allocate`
- fix debug impl, add missing display impl for arrays

### Other

- Generalize `Facet` array impl to arbitrary lengths
- Allow dead code

## [0.3.0](https://github.com/facet-rs/facet/compare/facet-poke-v0.2.3...facet-poke-v0.3.0) - 2025-04-10

### Other

- Move facet-args to typed()
- Introduced TypedPokeValue
- Use put rather than write for all users of PokeValue
- rename pokevalue:: put into pokevalue:: write and provide a safe alternative
- introduces put in poke value which is safe

## [0.2.3](https://github.com/facet-rs/facet/compare/facet-poke-v0.2.2...facet-poke-v0.2.3) - 2025-04-10

### Fixed

- fix readmes

### Other

- remove spacing
- no height
- Update Readmes with logos.

## [0.2.2](https://github.com/facet-rs/facet/compare/facet-poke-v0.2.1...facet-poke-v0.2.2) - 2025-04-10

### Other

- Working with flag, just not positional
- WIP facet-args

## [0.2.1](https://github.com/facet-rs/facet/compare/facet-poke-v0.2.0...facet-poke-v0.2.1) - 2025-04-09

### Other

- updated the following local packages: facet-trait, facet-derive, facet-peek

## [0.2.0](https://github.com/facet-rs/facet/compare/facet-poke-v0.1.2...facet-poke-v0.2.0) - 2025-04-08

### Other

- More nostd
- Less experimental now
- non-exhaustive enums
- Def is nonexhaustive too
- miri fixes

## [0.1.2](https://github.com/facet-rs/facet/compare/facet-poke-v0.1.1...facet-poke-v0.1.2) - 2025-04-08

### Other

- preparing for json tuples
- oh mh
- clean up list/map
- No cloning PokeStruct
- wip json iterative
- Fix indentation etc.
- mhmhmh pretty is not doing its job
- WIP pretty
- not using namespace runners for now

## [3.1.1](https://github.com/facet-rs/facet/compare/facet-poke-v3.1.0...facet-poke-v3.1.1) - 2025-04-05

### Other

- Add .envrc to try removing rebuilds
- C demo
- C example
- shapedebug bye
- woo everything builds
- getting there
- extract poke
- Extract peek
