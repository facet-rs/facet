# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/bearcove/dibs/compare/dibs-runtime-v0.0.0...dibs-runtime-v0.1.0) - 2026-06-02

### Added

- add SQL function call syntax with facet #[facet(other)] fix
- dibs-runtime crate and facet-based codegen

### Fixed

- *(ci)* fix clippy warnings and doc issues

### Other

- TraceErr emits structured tracing on QueryError
- Remove duplicate AST layer in dibs-query-gen  ([#12](https://github.com/bearcove/dibs/pull/12))
- add README.md.in templates for missing crates
- consolidate workspace dependencies and reorganize query files
- disable default features on rust_decimal to avoid serde
