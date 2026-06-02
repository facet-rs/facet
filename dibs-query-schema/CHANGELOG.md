# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/bearcove/dibs/compare/dibs-query-schema-v0.0.0...dibs-query-schema-v0.1.0) - 2026-06-02

### Added

- add bulk insert/upsert support (@insert-many, @upsert-many)
- add SQL function call syntax with facet #[facet(other)] fix
- *(query)* add DISTINCT and DISTINCT ON support
- add more filter operators
- implement relation-level ORDER BY with LATERAL joins
- *(query-gen)* add INSERT, UPSERT, UPDATE, DELETE mutations to styx DSL
- *(schema)* extract schema types to dedicated crates, embed in binary

### Fixed

- address clippy warnings and normalize query schema
- use kebab-case for @not-null filter operator
- use IndexMap instead of HashMap to preserve field order

### Other

- Add @float param type (f64 / DOUBLE PRECISION)
- @jsonb param type → $N::jsonb cast at the binding site
- Upgrade deps to stable releases
- Upgrade everything, add fixtures 1b regression test
- Remove duplicate AST layer in dibs-query-gen  ([#12](https://github.com/bearcove/dibs/pull/12))
- Refactor LSP diagnostics to use typed schema instead of untyped AST ([#10](https://github.com/bearcove/dibs/pull/10))
- Add @bytes param type support for bytea columns
- Support doc comments on queries in styx files
- add README.md.in templates for crates
- JSONB operators
- change upsert syntax to on-conflict with target/update
- consolidate workspace dependencies and reorganize query files
