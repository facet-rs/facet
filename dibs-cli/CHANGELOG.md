# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/bearcove/dibs/compare/dibs-cli-v0.0.0...dibs-cli-v0.1.0) - 2026-06-02

### Added

- *(lsp)* add support for @insert-many and @upsert-many
- *(lsp)* improve inlay hints and expand test coverage
- *(dibs)* add NULLS FIRST/LAST ordering support for indexes
- add ordered index column support (ASC/DESC)
- *(dibs)* add partial unique index support
- redesign TUI with 2 tabs and auto-rebuild
- add CLI command for generating migrations from schema diff
- *(query-gen)* namespace nested struct names to avoid collisions
- add SQL function call syntax with facet #[facet(other)] fix
- *(lsp)* add large offset warning and empty select detection
- *(lsp)* add FK relationship validation for @rel blocks
- *(lsp)* add literal type vs column type checking
- *(lsp)* add param type vs column type checking
- *(lsp)* add soft delete and relation linting
- *(lsp)* add query linting diagnostics
- *(lsp)* implement go-to-definition for $param references
- *(cli)* add LSP extension for Styx integration
- *(config)* use .config/dibs.styx for configuration
- *(schema)* extract schema types to dedicated crates, embed in binary
- *(query-gen)* add Facet schema types for query DSL
- admin save workflow, ecommerce schema, solver improvements
- *(tui)* migration dialog starts empty with autogenerate hint
- *(tui)* syntax-highlighted migration errors with source location
- *(tui)* exit after migration creation to trigger rebuild
- *(tui)* add 'd' key to delete uncommitted migrations
- improve TUI with rich SQL errors, rebuild support, and FK navigation
- auto-derive migration version from filename
- human-readable migration timestamps (m_2026_01_18_173711)
- smarter migration name suggestions
- load diff on startup, add prominent 'g' hint
- migration name dialog and auto-add to mod.rs
- generate migration from diff in TUI
- build TUI, jiff::Timestamp support, admin UI improvements
- migration source viewer with arborium syntax highlighting
- unified TUI with schema/diff/migrations tabs, dotenvy support
- use dibs.styx config file with facet-styx
- generate Rust migration files instead of SQL
- wire migrate and status commands to roam service
- wire dibs CLI to spawn db crate via roam
- add roam service API for CLI-to-db-crate communication
- add schema diff command and integration tests
- add schema introspection and diff infrastructure
- auto-detect Zed terminal and open in Zed
- open source in editor from TUI
- add source location and doc comments to schema
- implement migration skeleton generation
- add SQL generation from schema
- add indices support and TUI improvements
- add expand/collapse and FK navigation to schema TUI
- add interactive TUI for schema browsing
- phase 001 - schema definition via facet reflection

### Fixed

- *(lsp)* recognize shorthand param refs in unused param detection
- *(lsp)* inlay hints for numeric literals and expand test coverage
- *(dibs-cli)* remove sample tables and use roam for schema command
- respect db.crate config for migrations directory
- update for roam Context parameter changes
- clippy warnings (needless borrows, manual_strip)
- formatting and comment out local styx patch for CI
- *(codegen)* use closure param for first column in relation mapping
- embed schemas separately instead of combined
- *(ci)* exclude dibs-cli from workspace, run cargo fmt
- *(ci)* use git dependencies for arborium instead of local paths
- make Shift+Tab consistent with Tab in Schema view
- 'g' keybinding for migration generation was shadowed by gg navigation
- handle workspace-relative paths from file!() macro
- launch TUI by default when running dibs with no subcommand
- clippy warnings

### Other

- Add @float param type (f64 / DOUBLE PRECISION)
- @jsonb param type → $N::jsonb cast at the binding site
- Use published Styx 4 releases
- Use git-pinned Styx with Vox 0.8
- Upgrade deps to stable releases
- Fix code action for redundant-param. Closes #9
- Upgrade everything, add fixtures 1b regression test
- Remove duplicate AST layer in dibs-query-gen  ([#12](https://github.com/bearcove/dibs/pull/12))
- Improve error messages for unknown columns by listing available columns ([#11](https://github.com/bearcove/dibs/pull/11))
- Refactor LSP diagnostics to use typed schema instead of untyped AST ([#10](https://github.com/bearcove/dibs/pull/10))
- Update to new styx-lsp-ext span-based API
- Fix diagnostic positions: use Range::from_span with content instead of RPC
- Fix inlay hint location
- Fix syntax of completions test
- add missing commas in styx test file
- Fix clippy warnings in lsp_extension.rs
- Add redundant-param lint and inlay hints for where clauses
- Use Diagnostic.data for code action instead of parsing message
- Add lint + code action for redundant param references
- Extend LSP support for insert/update/upsert operations
- Add inlay hint position tests
- Better migration logging
- Add query tracing to dibs
- Refactor dibs-cli to use figue for config, remove local fallbacks
- Migrate to figue
- split SQL safely for dollar quotes
- handle Table.trigger_checks
- add CHECK constraints + blake3 naming
- add README templates for dibs-cli and dibs-config
- Port to facet-args
- Fix build
- consolidate workspace dependencies and reorganize query files
- silence unused variable warnings
- run each migration in its own transaction
- upgrade dependencies
- update facet to git dependency
