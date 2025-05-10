# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.23.1](https://github.com/facet-rs/facet/compare/facet-json-v0.23.0...facet-json-v0.23.1) - 2025-05-10

### Added

- Allow empty string rename values

### Fixed

- Add support for Unicode escape sequences in JSON strings

### Other

- Release facet-reflect
- Release facet-derive-parser
- Upgrade facet-core
- Fix additional tests
- Determine enum variant after default_from_fn
- Uncomment deserialize

## [0.23.0](https://github.com/facet-rs/facet/compare/facet-json-v0.22.0...facet-json-v0.23.0) - 2025-05-08

### Other

- *(deserialize)* [**breaking**] make deserialize format stateful
