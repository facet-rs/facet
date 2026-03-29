# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/bearcove/vox/compare/vox-v0.2.2...vox-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))

### Changed

- Remove the implicit `From<DriverCaller> for ()` conversion. Use `NoopCaller` with `establish::<NoopCaller>(...)` when you want root liveness without a root client API.
