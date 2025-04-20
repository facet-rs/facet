# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.6](https://github.com/facet-rs/facet/compare/facet-derive-v0.9.5...facet-derive-v0.9.6) - 2025-04-20

### Other

- updated the following local packages: facet-core, facet-derive-emit

## [0.9.5](https://github.com/facet-rs/facet/compare/facet-derive-v0.9.4...facet-derive-v0.9.5) - 2025-04-19

### Other

- updated the following local packages: facet-core, facet-derive-emit

## [0.9.4](https://github.com/facet-rs/facet/compare/facet-derive-v0.9.3...facet-derive-v0.9.4) - 2025-04-19

### Other

- updated the following local packages: facet-core, facet-derive-emit

## [0.9.3](https://github.com/facet-rs/facet/compare/facet-derive-v0.9.2...facet-derive-v0.9.3) - 2025-04-18

### Other

- updated the following local packages: facet-derive-emit

## [0.9.2](https://github.com/facet-rs/facet/compare/facet-derive-v0.9.1...facet-derive-v0.9.2) - 2025-04-18

### Other

- updated the following local packages: facet-derive-emit

## [0.9.1](https://github.com/facet-rs/facet/compare/facet-derive-v0.9.0...facet-derive-v0.9.1) - 2025-04-18

### Other

- updated the following local packages: facet-derive-emit

## [0.1.20](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.19...facet-derive-v0.1.20) - 2025-04-12

### Other

- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- Add most CI improvements from #166 ([#172](https://github.com/facet-rs/facet/pull/172))

## [0.1.19](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.18...facet-derive-v0.1.19) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.1.18](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.17...facet-derive-v0.1.18) - 2025-04-11

### Other

- Derive Facet for #[repr(C)] enums (merged) ([#163](https://github.com/facet-rs/facet/pull/163))
- Clean code generation ([#161](https://github.com/facet-rs/facet/pull/161))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- Support generic ADTs ([#130](https://github.com/facet-rs/facet/pull/130))

## [0.1.17](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.16...facet-derive-v0.1.17) - 2025-04-11

### Other

- Remove workspace dependencies
- Properly parse `pub(in path)` visibilities
- Do not attempt to parse field types, that's not really our job
- Move the template files next to their output and improve the output of the facet-codegen crate.
- Support array fields in structs and whatnot.
- Error on incomplete parse in facet-derive
- Properly deal with macroe'd types

## [0.1.16](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.15...facet-derive-v0.1.16) - 2025-04-11

### Other

- Allow json serialization of static string types
- Logo credit

## [0.1.15](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.14...facet-derive-v0.1.15) - 2025-04-10

### Other

- updated the following local packages: facet-core

## [0.1.14](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.13...facet-derive-v0.1.14) - 2025-04-10

### Other

- All tests work
- Close — just missing variant docs afaict
- Inline macros into derive macros, use gen_struct_field for enums fields as well
- Unify unit struct, tuple struct, struct processing
- Capture struct field doc comments
- Process doc comments simply as a slice of stringsl
- uhh why join them actually
- Doc extraction works well
- Basic doc grabbing but I imagine we're not out of the woods yet

## [0.1.13](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.12...facet-derive-v0.1.13) - 2025-04-10

### Other

- Export shape info as statics
- enums are peekable 😎
- holy ship it works
- Start peeking/poking enums

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.11...facet-derive-v0.1.12) - 2025-04-10

### Other

- updated the following local packages: facet-core

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.10...facet-derive-v0.1.11) - 2025-04-10

### Other

- Parse unit structs, closes #96
- Use TypeId for every types, not just scalar. Closes #97

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.9...facet-derive-v0.1.10) - 2025-04-10

### Other

- Replace `ARCHETYPE` const with `SpezEmpty` type wrapper
- Merge facet-opaque, facet-spez, facet-types and facet-trait back into facet-core, to allow implementing Facet for Shape

### Other

- Removed `Facet::ARCHETYPE`

## [0.1.9](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.8...facet-derive-v0.1.9) - 2025-04-10

### Other

- middle ground
- and some spacing
- ok 30
- ok no ems
- image height
- use logo only for readmes
- logo v2

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.7...facet-derive-v0.1.8) - 2025-04-10

### Fixed

- fix readmes

### Other

- remove spacing
- no height
- Update Readmes with logos.

## [0.1.7](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.6...facet-derive-v0.1.7) - 2025-04-10

### Other

- Working with flag, just not positional
- Arbitrary attributes
- special attributes

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.5...facet-derive-v0.1.6) - 2025-04-10

### Other

- updated the following local packages: facet-trait

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.4...facet-derive-v0.1.5) - 2025-04-09

### Other

- updated the following local packages: facet-trait

## [0.1.4](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.3...facet-derive-v0.1.4) - 2025-04-09

### Other

- DUMMY => ARCHETYPE

## [0.1.3](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.2...facet-derive-v0.1.3) - 2025-04-08

### Other

- More nostd
- Less experimental now
- woo almost everything is non-exhaustive
- tuple struct fix
- Okay, Shape is nonexhaustive

## [0.1.2](https://github.com/facet-rs/facet/compare/facet-derive-v0.1.1...facet-derive-v0.1.2) - 2025-04-08

### Other

- preparing for json tuples
- Add support for sensitive fields
- WIP pretty
- not using namespace runners for now

## [3.1.1](https://github.com/facet-rs/facet/compare/facet-derive-v3.1.0...facet-derive-v3.1.1) - 2025-04-05

### Fixed

- fix errors now
- fix derive probably

### Other

- Facet is unsafe
- json tests almost passing
- errors gone
- restore json test a little?? not much
- everything... works?
- ARCHETYPE
- 29 tests passed aw yiss
- clone in place => clone into
- mhhh getting somewhere
- fun
- Add more specializations
- nice nice
- I think it works????
- getting somewhere maybe? but only in macros, they weren't joking.
- ahhhhh
- so far so god
- make everything const??
- Move tests to facet proper
- whoa hey down to 111 errors

## [3.1.0](https://github.com/facet-rs/facet/compare/facet-derive-v3.0.0...facet-derive-v3.1.0) - 2025-03-31

### Added

- add support for tuple fields in Facet derive macro

### Fixed

- Correct unsynn parser definitions and field handling

### Other

- full docs baybee
- Fill in missing docs
- errors--
- Fix all markdown links
- Mhh it derives _something_
- Distinguish structs, tuples, and tuple structs
- allow type complexity (no choice..)
- more tuple support
- More complex derives work
- Support more attribute
- more derive
- Add preliminary enum support
- shill for namespace, closes #36

## [2.0.0](https://github.com/facet-rs/facet/compare/facet-derive-v1.0.0...facet-derive-v2.0.0) - 2025-03-11

### Other

- Stub out facet-yaml
- Simplify Shape name function signature
- Change Shape.name from static str to NameFn
- Stability notes
- link to 'free of syn' campaign
- Ensure no syn dependency (and badge about it), closes #9
- Introduce core crate
- Get rid of debug/display, closes #4
