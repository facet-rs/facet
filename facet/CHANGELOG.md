# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.7](https://github.com/facet-rs/facet/compare/facet-v0.18.6...facet-v0.18.7) - 2025-05-10

### Fixed

- Facet impl for core::ops::Range and add a test

### Other

- Release facet-reflect
- Release facet-derive
- Upgrade facet-core
- Fix slow tests
- Rework type information (Def)

## [0.18.6](https://github.com/facet-rs/facet/compare/facet-v0.18.5...facet-v0.18.6) - 2025-05-08

### Other

- updated the following local packages: facet-core, facet-reflect, facet-derive

## [0.18.5](https://github.com/facet-rs/facet/compare/facet-v0.18.4...facet-v0.18.5) - 2025-05-06

### Other

- updated the following local packages: facet-core, facet-reflect, facet-derive

## [0.18.4](https://github.com/facet-rs/facet/compare/facet-v0.18.3...facet-v0.18.4) - 2025-05-02

### Other

- Do compile-time check of default impl

## [0.18.3](https://github.com/facet-rs/facet/compare/facet-v0.18.2...facet-v0.18.3) - 2025-04-29

### Other

- Reduce duplication in derive macro
- Make sure attributes parse correctly
- Fix tests
- Start using quote
- Used parsed enums
- Add support for rename_all and arbitrary attributes on variants
- allow enum variant attributes

## [0.18.2](https://github.com/facet-rs/facet/compare/facet-v0.18.1...facet-v0.18.2) - 2025-04-27

### Other

- updated the following local packages: facet-derive

## [0.18.1](https://github.com/facet-rs/facet/compare/facet-v0.18.0...facet-v0.18.1) - 2025-04-26

### Added

- Add support for rename_all on containers

## [0.9.8](https://github.com/facet-rs/facet/compare/facet-v0.9.7...facet-v0.9.8) - 2025-04-23

### Other

- Uncomment enum tests
- bgp.as_phantom_data()
- Use bgp throughout the codebase
- Fix support for C-style enums with the derive macro.
- WIP
- Document known issues & add a DEVELOP.md
- Back to depot runners.
- Enable reflect and camino features on docs.rs

## [0.9.7](https://github.com/facet-rs/facet/compare/facet-v0.9.6...facet-v0.9.7) - 2025-04-21

### Other

- Implement the skip_serializing/skip_serializing_if attribute

## [0.9.6](https://github.com/facet-rs/facet/compare/facet-v0.9.5...facet-v0.9.6) - 2025-04-20

### Other

- updated the following local packages: facet-core, facet-reflect, facet-derive

## [0.9.5](https://github.com/facet-rs/facet/compare/facet-v0.9.4...facet-v0.9.5) - 2025-04-19

### Added

- *(json)* Support default attribute.

## [0.9.4](https://github.com/facet-rs/facet/compare/facet-v0.9.3...facet-v0.9.4) - 2025-04-19

### Other

- updated the following local packages: facet-core, facet-reflect, facet-derive

## [0.9.3](https://github.com/facet-rs/facet/compare/facet-v0.9.2...facet-v0.9.3) - 2025-04-18

### Other

- updated the following local packages: facet-derive

## [0.9.2](https://github.com/facet-rs/facet/compare/facet-v0.9.1...facet-v0.9.2) - 2025-04-18

### Other

- updated the following local packages: facet-reflect, facet-derive

## [0.9.1](https://github.com/facet-rs/facet/compare/facet-v0.9.0...facet-v0.9.1) - 2025-04-18

### Other

- updated the following local packages: facet-reflect, facet-derive

## [0.2.0](https://github.com/facet-rs/facet/compare/facet-v0.1.20...facet-v0.2.0) - 2025-04-12

### Other

- Fix docs.rs build failure, closes #178 ([#179](https://github.com/facet-rs/facet/pull/179))
- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- Add most CI improvements from #166 ([#172](https://github.com/facet-rs/facet/pull/172))

## [0.1.20](https://github.com/facet-rs/facet/compare/facet-v0.1.19...facet-v0.1.20) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.1.19](https://github.com/facet-rs/facet/compare/facet-v0.1.18...facet-v0.1.19) - 2025-04-11

### Other

- Derive Facet for #[repr(C)] enums (merged) ([#163](https://github.com/facet-rs/facet/pull/163))
- Clean code generation ([#161](https://github.com/facet-rs/facet/pull/161))
- Revert to facet-{core,derive,reflect} deps, closes #156 ([#159](https://github.com/facet-rs/facet/pull/159))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- wip reflect ([#155](https://github.com/facet-rs/facet/pull/155))
- Support generic ADTs ([#130](https://github.com/facet-rs/facet/pull/130))

## [0.1.18](https://github.com/facet-rs/facet/compare/facet-v0.1.17...facet-v0.1.18) - 2025-04-11

### Added

- *(core)* Allow use with just alloc

### Fixed

- *(facet)* Add no_std

### Other

- Remove workspace dependencies
- Fix docs errors
- Properly parse `pub(in path)` visibilities
- Move the template files next to their output and improve the output of the facet-codegen crate.
- Support array fields in structs and whatnot.
- Add a sample project which will be code-generated, cf. #88
- *(facet)* Lint against std
- Properly deal with macroe'd types

## [0.1.17](https://github.com/facet-rs/facet/compare/facet-v0.1.16...facet-v0.1.17) - 2025-04-11

### Other

- Logo credit

## [0.1.16](https://github.com/facet-rs/facet/compare/facet-v0.1.15...facet-v0.1.16) - 2025-04-11

### Other

- Fix up #114

## [0.1.15](https://github.com/facet-rs/facet/compare/facet-v0.1.14...facet-v0.1.15) - 2025-04-10

### Other

- Full option support

## [0.1.14](https://github.com/facet-rs/facet/compare/facet-v0.1.13...facet-v0.1.14) - 2025-04-10

### Other

- failing tests re: enum doc comments
- Unify unit struct, tuple struct, struct processing
- Capture struct field doc comments
- Process doc comments simply as a slice of stringsl
- uhh why join them actually
- Doc extraction works well
- Basic doc grabbing but I imagine we're not out of the woods yet
- Fix wrong poke example, closes #111

## [0.1.13](https://github.com/facet-rs/facet/compare/facet-v0.1.12...facet-v0.1.13) - 2025-04-10

### Other

- updated the following local packages: facet-core, facet-derive

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-v0.1.11...facet-v0.1.12) - 2025-04-10

### Other

- updated the following local packages: facet-core, facet-derive

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-v0.1.10...facet-v0.1.11) - 2025-04-10

### Other

- Parse unit structs, closes #96

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-v0.1.9...facet-v0.1.10) - 2025-04-10

### Fixed

- typo in README.md

### Other

- Merge branch 'main' into from-ptr
- Update doc tests, too
- peek & poke are not reexported by the main crate
- Make tests pass on stable for now...
- Add an empty test (failing on rust stable)
- Merge branch 'main' into patch-1
- Fixed doc tests
- Merge facet-opaque, facet-spez, facet-types and facet-trait back into facet-core, to allow implementing Facet for Shape

### Other

- Removed `Facet::ARCHETYPE` in favor of new `SpezEmpty` type wrapper

## [0.1.9](https://github.com/facet-rs/facet/compare/facet-v0.1.8...facet-v0.1.9) - 2025-04-10

### Other

- middle ground
- and some spacing
- ok 30
- ok no ems
- image height
- use logo only for readmes
- logo v2

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-v0.1.7...facet-v0.1.8) - 2025-04-10

### Fixed

- fix readmes

### Other

- remove spacing
- no height
- Update Readmes with logos.

## [0.1.7](https://github.com/facet-rs/facet/compare/facet-v0.1.6...facet-v0.1.7) - 2025-04-10

### Other

- doctests
- Link facet-args
- show off CLI parsing example
- shaep -> shape

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-v0.1.5...facet-v0.1.6) - 2025-04-10

### Other

- Am excited
- Show it's actually runtime
- Specialization demo re: pretty
- Unify top-level README
- WIP readmes
- Fix link

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-v0.1.4...facet-v0.1.5) - 2025-04-09

### Other

- upgrades
- Fix YAML link in README

## [0.1.4](https://github.com/facet-rs/facet/compare/facet-v0.1.3...facet-v0.1.4) - 2025-04-09

### Other

- DUMMY => ARCHETYPE

## [0.1.3](https://github.com/facet-rs/facet/compare/facet-v0.1.2...facet-v0.1.3) - 2025-04-08

### Other

- More nostd
- nostd facet-trait
- facet-types is now no_std friendly :)
- Less experimental now
- woo almost everything is non-exhaustive

## [0.1.2](https://github.com/facet-rs/facet/compare/facet-v0.1.1...facet-v0.1.2) - 2025-04-08

### Other

- preparing for json tuples
- Add support for sensitive fields
- not using namespace runners for now
- Fix links to other repos

## [3.1.1](https://github.com/facet-rs/facet/compare/facet-v3.1.0...facet-v3.1.1) - 2025-04-05

### Fixed

- fix errors now
- fix miri/memory problems
- fix derive probably

### Other

- Add .envrc to try removing rebuilds
- Facet is unsafe
- Fix tests etc.
- woo everything builds
- getting there
- The great split
- start fixing doc tests
- Cool, the hacking guide is in
- 29 tests passed aw yiss
- clone stuff
- clone in place => clone into
- mhmh
- mhhh getting somewhere
- fun
- bigger and bigger
- switch to btparse
- time to fix those tests
- mhmhmh
- color backtrace in tests please?
- tests are made to fail
- mhmh tests are failing huh
- maps, slowlyl
- peeking a lotta things
- more vec stuff
- okay, debug and default, it's something
- mhh we regressed
- Uhhh slices work?
- well this weirdly works?
- mhkay
- traits tests look better
- more spez is going well
- mh
- mhhhmhh
- mhhhhh it's probably the uninit thing, ngl
- I'm confused now
- uhhhh
- uhhh what
- weird
- more tests
- More tests
- rename spez to traits
- mhmhmh
- Uncomment a bunch of tests
- Unreasonably happy with that tbh
- nice nice
- Use spez-like ideas to set Debug if it's set on the type
- Well that's not really const
- yessssss
- getting somewhere maybe? but only in macros, they weren't joking.
- mhhh
- ahhhhh
- alright, will this work?
- Mhh doesn't work yet
- Well the tests do pass
- welp
- don't compare strings
- uncomment some derives
- so far so god
- new structure works
- introduce init_in_place_with_capacity
- Rname more things for more consistency
- Move tests to facet proper
- whoa hey down to 111 errors
- Innards => Def

## [3.1.0](https://github.com/facet-rs/facet/compare/facet-v3.0.0...facet-v3.1.0) - 2025-03-31

### Other

- Fill in missing docs
- Document more stuff
- Improve naming
- More notes
- Fix all markdown links
- More docs
- unfuck docs
- Mhh it derives _something_
- Tuple layouts
- more tuple support
- pre-commit hook with cargo-fmt
- More complex derives work
- Real-life derive, which fails
- Support more attribute
- more derive
- shill for namespace, closes #36
- just pre-commit, just fmt
- set up cargo-husky to run cargo fmt
- specific toolchains, reformat code

## [3.0.0](https://github.com/facet-rs/facet/compare/facet-v2.0.1...facet-v3.0.0) - 2025-03-11

### Other

- Fix doc tests in README.md
- Document how to write your deserializer a little better

## [2.0.0](https://github.com/facet-rs/facet/compare/facet-v1.0.0...facet-v2.0.0) - 2025-03-11

### Other

- Change Shape.name from static str to NameFn
- Looking good!
- clean up drop impls
- Stability notes
- link to 'free of syn' campaign
- Make derive unconditional, closes #8
- Get rid of insta, closes #10
- Ensure no syn dependency (and badge about it), closes #9
- Introduce core crate
- Get rid of debug/display, closes #4
- Start implementing transparent
