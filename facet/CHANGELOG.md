# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
