# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
