# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/bearcove/dibs/compare/dibs-sql-v0.0.0...dibs-sql-v0.1.0) - 2026-06-02

### Added

- add SQL function call syntax with facet #[facet(other)] fix
- *(dibs-sql)* add AST-based SQL builder with param deduplication

### Other

- Wire up FunctionSpec filter validation with proper error handling ([#14](https://github.com/bearcove/dibs/pull/14))
- Remove duplicate AST layer in dibs-query-gen  ([#12](https://github.com/bearcove/dibs/pull/12))
- add README.md.in templates for missing crates
