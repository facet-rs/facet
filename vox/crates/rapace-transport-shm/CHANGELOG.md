# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/bearcove/rapace/compare/rapace-transport-shm-v0.3.0...rapace-transport-shm-v0.4.0) - 2025-12-14

### Added

- *(transport-shm)* add hub architecture for multi-peer SHM
- *(transport-shm)* store config in header, add open_file_auto

### Fixed

- make futex and doorbell tests portable to macOS
- make doorbell portable to macOS, add macOS CI

### Other

- Fix test suite
- update SHM doorbell/hub notes ([#38](https://github.com/bearcove/rapace/pull/38))
