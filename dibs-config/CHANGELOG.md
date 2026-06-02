# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/bearcove/dibs/compare/dibs-config-v0.0.0...dibs-config-v0.1.0) - 2026-06-02

### Added

- add SQL function call syntax with facet #[facet(other)] fix
- *(schema)* extract schema types to dedicated crates, embed in binary

### Fixed

- respect db.crate config for migrations directory

### Other

- add README templates for dibs-cli and dibs-config
- consolidate workspace dependencies and reorganize query files
