# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.44.4](https://github.com/facet-rs/facet/compare/facet-reflect-v0.44.3...facet-reflect-v0.44.4) - 2026-04-13

### Fixed

- *(reflect)* drain rope before dropping Vec in List frame deinit
- *(reflect)* prevent double-free in List deinit when Vec is initialized
- *(reflect)* drop map key when severing failed MapValue pending entry
- *(reflect)* prevent double-free in deferred-mode map/option/smartptr cleanup
- *(deps)* update rust dependencies ([#2158](https://github.com/facet-rs/facet/pull/2158))

### Other

- *(reflect)* drop start_depth from finish_deferred parent lookups
