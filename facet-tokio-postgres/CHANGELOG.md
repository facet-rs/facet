# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.42.0](https://github.com/facet-rs/facet/compare/facet-tokio-postgres-v0.41.0...facet-tokio-postgres-v0.42.0) - 2026-01-06

### Other

- *(deps)* update rust dependencies
- Add rust_decimal::Decimal support + fix XML type inference
- Add UUID, jiff, chrono, and time support to facet-tokio-postgres
- Gate postgres tests behind test-postgres feature
- Add CI job for facet-tokio-postgres with postgres service container
- Add integration tests with testcontainers
- Add facet-tokio-postgres crate for deserializing postgres rows
