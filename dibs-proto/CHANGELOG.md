# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/bearcove/dibs/compare/dibs-proto-v0.0.0...dibs-proto-v0.1.0) - 2026-06-02

### Added

- *(dibs)* add NULLS FIRST/LAST ordering support for indexes
- add ordered index column support (ASC/DESC)
- *(dibs)* add partial unique index support
- add SQL function call syntax with facet #[facet(other)] fix
- *(tui)* syntax-highlighted migration errors with source location
- add column annotations, enum support, and hash-based routing
- *(admin-ui)* add FK navigation, auto-generated detection, and date/time support
- improve TUI with rich SQL errors, rebuild support, and FK navigation
- generate migration from diff in TUI
- build TUI, jiff::Timestamp support, admin UI improvements
- add query builder and SquelService for dynamic CRUD operations
- migration source viewer with arborium syntax highlighting

### Other

- @jsonb param type → $N::jsonb cast at the binding site
- Upgrade deps to stable releases
- Better migration logging
- extract squel-service package and inject db client
- add README.md.in templates for missing crates
- JSONB operators
- consolidate workspace dependencies and reorganize query files
- extract protocol definitions into dibs-proto crate
