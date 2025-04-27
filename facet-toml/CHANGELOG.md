# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.18.1...facet-toml-v0.19.0) - 2025-04-27

### Added

- *(toml)* implement 'default' attribute

### Other

- *(toml)* [**breaking**] setup structure for serialize implementation

## [0.15.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.14.0...facet-toml-v0.15.0) - 2025-04-23

### Added

- *(toml)* include reflect path in error

### Fixed

- *(toml)* ensure alloc is properly used and deny unsafe code

### Other

- WIP
- Back to depot runners.
- *(toml)* set authors to me and @fasterthanlime

## [0.14.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.13.0...facet-toml-v0.14.0) - 2025-04-21

### Added

- *(toml)* pretty error handling with a nice report
- *(toml)* start properly handling errors

### Fixed

- *(toml)* parse all types implementing FromStr and improve errors
- *(toml)* handle option cases in enums

### Other

- *(toml)* use `Facet` display implementation instead of creating wrapper type
- *(toml)* improve coverage

## [0.3.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.2.0...facet-toml-v0.3.0) - 2025-04-12

### Other

- Install cargo-tarpaulin in Docker, and collect + report coverage in CI ([#177](https://github.com/facet-rs/facet/pull/177))
- TOML enum with unit variant implementation ([#168](https://github.com/facet-rs/facet/pull/168))

## [0.2.0](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.12...facet-toml-v0.2.0) - 2025-04-12

### Other

- different place in readme
- Sponsored by depot

## [0.1.12](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.11...facet-toml-v0.1.12) - 2025-04-11

### Other

- Revert to facet-{core,derive,reflect} deps, closes #156 ([#159](https://github.com/facet-rs/facet/pull/159))
- Light deps ([#158](https://github.com/facet-rs/facet/pull/158))
- wip reflect ([#155](https://github.com/facet-rs/facet/pull/155))

## [0.1.11](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.10...facet-toml-v0.1.11) - 2025-04-11

### Other

- Remove workspace dependencies
- Move the template files next to their output and improve the output of the facet-codegen crate.

## [0.1.10](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.9...facet-toml-v0.1.10) - 2025-04-11

### Other

- Logo credit

## [0.1.8](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.7...facet-toml-v0.1.8) - 2025-04-10

### Other

- PokeUninit / Poke

## [0.1.6](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.5...facet-toml-v0.1.6) - 2025-04-10

### Other

- updated the following local packages: facet-core, facet-poke, facet-derive

## [0.1.5](https://github.com/facet-rs/facet/compare/facet-toml-v0.1.4...facet-toml-v0.1.5) - 2025-04-10

### Other

- Use put rather than write for all users of PokeValue
- rename pokevalue:: put into pokevalue:: write and provide a safe alternative
- introduces put in poke value which is safe

## [0.1.4](https://github.com/facet-rs/facet/releases/tag/facet-toml-v0.1.4) - 2025-04-10

### Added

- *(toml)* Add facet-toml crate

### Other

- Woops, use new everywhere
- A few fixups
