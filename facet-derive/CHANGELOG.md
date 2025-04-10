# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
