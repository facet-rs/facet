# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.18](https://github.com/facet-rs/facet/compare/facet-args-v0.19.17...facet-args-v0.19.18) - 2025-07-03

### Other

- Rename SmartPointer to Pointer
- Fix clippy warnings and remove unused code
- Vendor deserialize into facet-args

## [0.19.17](https://github.com/facet-rs/facet/compare/facet-args-v0.19.16...facet-args-v0.19.17) - 2025-06-26

### Other

- Remove ScalarDef
- Remove PartialEq implementations
- Apply modern clippy fixes (mostly format strings)

## [0.19.16](https://github.com/facet-rs/facet/compare/facet-args-v0.19.15...facet-args-v0.19.16) - 2025-06-17

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize

## [0.19.15](https://github.com/facet-rs/facet/compare/facet-args-v0.19.14...facet-args-v0.19.15) - 2025-06-15

### Other

- Add a new QOL function to parse std args.

## [0.19.14](https://github.com/facet-rs/facet/compare/facet-args-v0.19.13...facet-args-v0.19.14) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize

## [0.19.13](https://github.com/facet-rs/facet/compare/facet-args-v0.19.12...facet-args-v0.19.13) - 2025-06-04

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize

## [0.19.12](https://github.com/facet-rs/facet/compare/facet-args-v0.19.11...facet-args-v0.19.12) - 2025-06-03

### Other

- Add discord logo + link

## [0.19.11](https://github.com/facet-rs/facet/compare/facet-args-v0.19.10...facet-args-v0.19.11) - 2025-06-02

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize

## [0.19.10](https://github.com/facet-rs/facet/compare/facet-args-v0.19.9...facet-args-v0.19.10) - 2025-05-31

### Other

- Fix facet-args tests
- More facet-yaml test fixes
- facet-json tests pass
- More facet-json tests
- wow everything typechecks
- Rename some methods

## [0.19.9](https://github.com/facet-rs/facet/compare/facet-args-v0.19.8...facet-args-v0.19.9) - 2025-05-27

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize

## [0.19.8](https://github.com/facet-rs/facet/compare/facet-args-v0.19.7...facet-args-v0.19.8) - 2025-05-26

### Other

- Don't crash when errors straddle EOF

## [0.19.7](https://github.com/facet-rs/facet/compare/facet-args-v0.19.6...facet-args-v0.19.7) - 2025-05-24

### Added

- *(args)* fill Substack via `Outcome::Resegment`

## [0.19.6](https://github.com/facet-rs/facet/compare/facet-args-v0.19.5...facet-args-v0.19.6) - 2025-05-21

### Other

- *(args)* standardise spans in runner and errors
- *(args)* ArgType; helpers; result wrappers

## [0.19.5](https://github.com/facet-rs/facet/compare/facet-args-v0.19.4...facet-args-v0.19.5) - 2025-05-20

### Added

- *(args)* arg-wise spans for reflection errors; ToCooked trait

### Other

- *(args)* TDD for Vec support

## [0.19.4](https://github.com/facet-rs/facet/compare/facet-args-v0.19.3...facet-args-v0.19.4) - 2025-05-18

### Other

- Introduce `'shape` lifetime, allowing non-'static shapes.

## [0.19.3](https://github.com/facet-rs/facet/compare/facet-args-v0.19.2...facet-args-v0.19.3) - 2025-05-16

### Added

- facet-args `Cli` trait impl; deserialize `&str` field
- *(deserialize)* support multiple input types via generic `Format`

### Other

- Modernize facet-csv/facet-kdl/facet-core/facet-args tests

## [0.19.2](https://github.com/facet-rs/facet/compare/facet-args-v0.19.1...facet-args-v0.19.2) - 2025-05-13

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize

## [0.19.1](https://github.com/facet-rs/facet/compare/facet-args-v0.19.0...facet-args-v0.19.1) - 2025-05-12

### Other

- updated the following local packages: facet-core, facet-reflect, facet-deserialize

## [0.19.0](https://github.com/facet-rs/facet/compare/facet-args-v0.18.9...facet-args-v0.19.0) - 2025-05-12

### Added

- *(facet-args)* rely on facet-deserialize StackRunner

### Other

- *(args)* [**breaking**] Remove arg field parsing to non-public

## [0.18.9](https://github.com/facet-rs/facet/compare/facet-args-v0.18.8...facet-args-v0.18.9) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.18.8](https://github.com/facet-rs/facet/compare/facet-args-v0.18.7...facet-args-v0.18.8) - 2025-05-10

### Added

- default and default value, tests

## [0.18.7](https://github.com/facet-rs/facet/compare/facet-args-v0.18.6...facet-args-v0.18.7) - 2025-05-10

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.18.6](https://github.com/facet-rs/facet/compare/facet-args-v0.18.5...facet-args-v0.18.6) - 2025-05-10

### Other

- Release facet-reflect
- Upgrade facet-core
- Fix facet-args test
- Fix one arg test
- references are not pointers

## [0.18.5](https://github.com/facet-rs/facet/compare/facet-args-v0.18.4...facet-args-v0.18.5) - 2025-05-08

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.18.4](https://github.com/facet-rs/facet/compare/facet-args-v0.18.3...facet-args-v0.18.4) - 2025-05-06

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.18.3](https://github.com/facet-rs/facet/compare/facet-args-v0.18.2...facet-args-v0.18.3) - 2025-05-02

### Other

- updated the following local packages: facet-core, facet-reflect

## [0.18.2](https://github.com/facet-rs/facet/compare/facet-args-v0.18.1...facet-args-v0.18.2) - 2025-04-29

### Added

- *(facet-args)* Expand error handling

### Fixed

- fix(args) - unspecified bool arguments should be assumed false instead of being a reflection error

## [0.18.1](https://github.com/facet-rs/facet/compare/facet-args-v0.18.0...facet-args-v0.18.1) - 2025-04-27

### Added

- *(facet-args)* support multi-word argument values

### Other

- Handle multiple positional args
- Resolve #443 - handle short named argument

## [0.15.0](https://github.com/facet-rs/facet/compare/facet-args-v0.14.0...facet-args-v0.15.0) - 2025-04-23

### Other

- WIP
- Back to depot runners.

## [0.3.0](https://github.com/facet-rs/facet/compare/facet-args-v0.2.0...facet-args-v0.3.0) - 2025-04-12

### Other

- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))

## [0.2.0](https://github.com/facet-rs/facet/compare/facet-args-v0.1.12...facet-args-v0.2.0) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-args-v0.1.11...facet-args-v0.1.12) - 2025-04-11

### Other

- Revert to facet-{core,derive,reflect} deps, closes #156 ([#159](https://github.com/facet-rs/facet/pull/159))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- wip reflect ([#155](https://github.com/facet-rs/facet/pull/155))

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-args-v0.1.10...facet-args-v0.1.11) - 2025-04-11

### Other

- Remove workspace dependencies
- Move the template files next to their output and improve the output of the facet-codegen crate.

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-args-v0.1.9...facet-args-v0.1.10) - 2025-04-11

### Other

- Logo credit

## [0.1.9](https://github.com/facet-rs/facet/compare/facet-args-v0.1.8...facet-args-v0.1.9) - 2025-04-11

### Other

- update Cargo.toml dependencies

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-args-v0.1.7...facet-args-v0.1.8) - 2025-04-10

### Other

- PokeUninit / Poke

## [0.1.7](https://github.com/facet-rs/facet/compare/facet-args-v0.1.6...facet-args-v0.1.7) - 2025-04-10

### Other

- Unify unit struct, tuple struct, struct processing

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-args-v0.1.5...facet-args-v0.1.6) - 2025-04-10

### Other

- updated the following local packages: facet-core, facet-reflect, facet

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-args-v0.1.4...facet-args-v0.1.5) - 2025-04-10

### Other

- Move facet-args to typed()
- Use put rather than write for all users of PokeValue
- rename pokevalue:: put into pokevalue:: write and provide a safe alternative
- introduces put in poke value which is safe

## [0.1.4](https://github.com/facet-rs/facet/compare/facet-args-v0.1.3...facet-args-v0.1.4) - 2025-04-10

### Fixed

- fix readmes

### Other

- remove spacing
- no height
- Update Readmes with logos.

## [0.1.3](https://github.com/facet-rs/facet/compare/facet-args-v0.1.2...facet-args-v0.1.3) - 2025-04-10

### Other

- facet args readme with example
- upgrades
- show off CLI parsing example
- argument parsing yus
- Working with flag, just not positional
- haha need positional arguments now
- wip facet-args
- WIP facet-args

## [0.1.2](https://github.com/facet-rs/facet/releases/tag/facet-args-v0.1.2) - 2025-04-10

### Other

- show off CLI parsing example
- argument parsing yus
- Working with flag, just not positional
- haha need positional arguments now
- wip facet-args
- WIP facet-args
