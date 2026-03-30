# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1](https://github.com/bearcove/vox/compare/vox-core-v0.3.0...vox-core-v0.3.1) - 2026-03-30

### Other

- Driver inflight cleanup and idem store (+ reflective server middleware tracing etc.) ([#271](https://github.com/bearcove/vox/pull/271))

## [0.3.0](https://github.com/bearcove/vox/compare/vox-core-v0.2.2...vox-core-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))

### Changed

- Remove the implicit `From<DriverCaller> for ()` conversion and add `NoopCaller` for liveness-only root handles.

## [7.0.0-alpha.3](https://github.com/bearcove/vox/compare/vox-core-v7.0.0-alpha.2...vox-core-v7.0.0-alpha.3) - 2026-03-03

### Other

- Add MaybeSend bound on erased caller
