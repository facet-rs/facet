# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [3.0.0-rc.4](https://github.com/facet-rs/facet/compare/picante-v3.0.0-rc.3...picante-v3.0.0-rc.4) - 2026-06-26

### Other

- cover legacy key-byte cache loads
- use typed runtime keys in hot paths
- add opt-in native key hashing

## [3.0.0-rc.3](https://github.com/facet-rs/facet/compare/picante-v3.0.0-rc.2...picante-v3.0.0-rc.3) - 2026-06-26

### Other

- slim runtime key lookups
- use typed facet runtime keys
- *(facet-hash)* add concrete FNV byte hashing
- *(facet-hash)* bulk hash byte sequences
- *(picante)* keep key hashing on existing byte hash
- *(picante)* cheapen key hashing and dep recording
- Speed up stable Picante dependency updates
- Speed up Picante structural equality
- Expand Picante runtime benchmarks

## [3.0.0-rc.2](https://github.com/facet-rs/facet/compare/picante-v3.0.0-rc.1...picante-v3.0.0-rc.2) - 2026-06-25

### Other

- Relax Picante stress test timeout

## [2.0.1](https://github.com/bearcove/picante/compare/picante-v2.0.0...picante-v2.0.1) - 2026-06-01

### Fixed

- *(snapshot)* isolate mutated snapshots from the cross-snapshot result cache

### Other

- snapshot override of registry-entity invalidates outer query (dodeca shape)
- snapshot input override invalidates derived + singleton queries
- snapshot input override invalidates derived queries

## [2.0.0](https://github.com/bearcove/picante/compare/picante-v1.0.0...picante-v2.0.0) - 2026-05-19

### Other

- Upgrade to facet 0.46
