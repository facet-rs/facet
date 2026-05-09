# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [4.0.0](https://github.com/bearcove/figue/compare/figue-v3.0.1...figue-v4.0.0) - 2026-05-09

### Other

- Add interactive HTML help ([#93](https://github.com/bearcove/figue/pull/93))
- Add built-in JSON Schema export ([#91](https://github.com/bearcove/figue/pull/91))

## [3.0.1](https://github.com/bearcove/figue/compare/figue-v3.0.0...figue-v3.0.1) - 2026-05-09

### Other

- Support multiple config roots ([#88](https://github.com/bearcove/figue/pull/88))
- fix #59 invalid shell error, fix #61 auto-detect shell

## [3.0.0](https://github.com/bearcove/figue/compare/figue-v2.0.6...figue-v3.0.0) - 2026-05-07

### Added

- add --[no-] negation support for boolean flags

### Other

- Fix nested config discovery and bool env coercion
- show help + Ariadne suggestion for missing CLI argument errors
- show subcommand-level help on missing subcommand/argument errors
- wrap long doc comments to respect HelpConfig::width

## [2.0.6](https://github.com/bearcove/figue/compare/figue-v2.0.5...figue-v2.0.6) - 2026-05-07

### Other

- upgrade facet-json to 0.46.1, add JsoncFormat

## [2.0.5](https://github.com/bearcove/figue/compare/figue-v2.0.4...figue-v2.0.5) - 2026-05-06

### Fixed

- resolve irrefutable let pattern warning in test
- propagate Vec coercion through flatten boundaries in enum variants

## [2.0.3](https://github.com/bearcove/figue/compare/figue-v2.0.2...figue-v2.0.3) - 2026-04-15

### Other

- update Cargo.toml dependencies

## [2.0.2](https://github.com/bearcove/figue/compare/figue-v2.0.1...figue-v2.0.2) - 2026-04-14

### Other

- update Cargo.toml dependencies

## [2.0.1](https://github.com/bearcove/figue/compare/figue-v2.0.0...figue-v2.0.1) - 2026-04-01

### Other

- Use kebab-case for flag names in help text ([#76](https://github.com/bearcove/figue/pull/76))
- Support short subcommand aliases as positional tokens ([#78](https://github.com/bearcove/figue/pull/78))
