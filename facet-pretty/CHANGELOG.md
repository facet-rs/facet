# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.23.2](https://github.com/facet-rs/facet/compare/facet-pretty-v0.23.1...facet-pretty-v0.23.2) - 2025-05-10

### Fixed

- *(pretty)* support tuples

## [0.23.1](https://github.com/facet-rs/facet/compare/facet-pretty-v0.23.0...facet-pretty-v0.23.1) - 2025-05-10

### Fixed

- *(pretty)* Fix clippy warnings
- *(pretty)* Fix printer with new Type/Def structure

### Other

- Release facet-reflect
- Release facet-derive-parser
- Upgrade facet-core
- Make variant() getters fallible — we might not know the internal enough to read the discriminant etc.

## [0.22.0](https://github.com/facet-rs/facet/compare/facet-pretty-v0.21.0...facet-pretty-v0.22.0) - 2025-05-06

### Other

- Make facet support NO_COLOR, closes #520

## [0.21.0](https://github.com/facet-rs/facet/compare/facet-pretty-v0.20.0...facet-pretty-v0.21.0) - 2025-05-02

### Other

- Migrate benches to divan, set up codspeed
- support camino's &Utf8Path and Utf8PathBuf

## [0.20.0](https://github.com/facet-rs/facet/compare/facet-pretty-v0.19.0...facet-pretty-v0.20.0) - 2025-04-29

### Other

- Add bench against `serde_json`
- Add bench against Debug

## [0.15.0](https://github.com/facet-rs/facet/compare/facet-pretty-v0.14.0...facet-pretty-v0.15.0) - 2025-04-23

### Other

- WIP
- Back to depot runners.
- remove unneeded value shadow
- Rename pretty print test

## [0.14.0](https://github.com/facet-rs/facet/compare/facet-pretty-v0.13.0...facet-pretty-v0.14.0) - 2025-04-21

### Other

- Implement `Facet` for (subset of) function pointers

## [0.3.0](https://github.com/facet-rs/facet/compare/facet-pretty-v0.2.0...facet-pretty-v0.3.0) - 2025-04-12

### Other

- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))

## [0.2.0](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.12...facet-pretty-v0.2.0) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot
- Fix formatting
- Make facet-pretty use facet-ansi, closes #164 ([#165](https://github.com/facet-rs/facet/pull/165))

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.11...facet-pretty-v0.1.12) - 2025-04-11

### Other

- Revert to facet-{core,derive,reflect} deps, closes #156 ([#159](https://github.com/facet-rs/facet/pull/159))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- wip reflect ([#155](https://github.com/facet-rs/facet/pull/155))

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.10...facet-pretty-v0.1.11) - 2025-04-11

### Other

- Remove workspace dependencies
- Move the template files next to their output and improve the output of the facet-codegen crate.

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.9...facet-pretty-v0.1.10) - 2025-04-11

### Other

- Logo credit

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.7...facet-pretty-v0.1.8) - 2025-04-10

### Other

- Woops, formatting
- Fix formatting indentation
- wip pretty printing
- Add option support to pretty-printer

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.5...facet-pretty-v0.1.6) - 2025-04-10

### Other

- updated the following local packages: facet-core, facet-derive, facet-peek

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.4...facet-pretty-v0.1.5) - 2025-04-10

### Other

- rename pokevalue:: put into pokevalue:: write and provide a safe alternative

## [0.1.4](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.3...facet-pretty-v0.1.4) - 2025-04-10

### Fixed

- fix readmes

### Other

- remove spacing
- no height
- Update Readmes with logos.

## [0.1.3](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.2...facet-pretty-v0.1.3) - 2025-04-09

### Other

- updated the following local packages: facet-trait, facet-derive, facet-peek

## [0.1.1](https://github.com/facet-rs/facet/compare/facet-pretty-v0.1.0...facet-pretty-v0.1.1) - 2025-04-08

### Other

- updated the following local packages: facet-trait, facet-derive, facet-peek

## [0.1.0](https://github.com/facet-rs/facet/releases/tag/facet-pretty-v0.1.0) - 2025-04-08

### Fixed

- fix pretty printing tests with redacted field

### Other

- clippy fixes
- Remove old code
- hurray, the iterative approach works
- wip iterative printer
- Fix indentation etc.
- pretty printer is pretty
- mhmhmh pretty is not doing its job
- Add support for sensitive fields
- WIP pretty
- wip facet-pretty
- not using namespace runners for now
- More READMEs
- quicksave
- more shapely => facet renames
- 0.1.0

## [3.1.0](https://github.com/facet-rs/facet/compare/facet-pretty-v3.0.0...facet-pretty-v3.1.0) - 2025-03-31

### Added

- introduce NameOpts

### Other

- full docs baybee
- Fill in missing docs
- Improve naming
- errors--
- errors--
- Fix all markdown links
- Well, I guess Slot::for_hash_map wasn't working
- arrays.. work?
- Distinguish structs, tuples, and tuple structs
- Add preliminary enum support
- shill for namespace, closes #36
- specific toolchains, reformat code

## [3.0.0](https://github.com/facet-rs/facet/compare/facet-pretty-v2.0.1...facet-pretty-v3.0.0) - 2025-03-11

### Added

- Add sensitive field support

### Other

- warnings--
- Sensitive fields are /actually/ redacted now
- Improve sensitive fields test with actual data
- Fix doc tests in README.md
- wip
- Use ScalarContents API to replace unsafe memory access in PrettyPrinter
- mh
- Introduce facet-pretty
