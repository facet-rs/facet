# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Remove the implicit `From<DriverCaller> for ()` conversion and add `NoopCaller` for liveness-only root handles.

## [7.0.0-alpha.3](https://github.com/bearcove/roam/compare/roam-core-v7.0.0-alpha.2...roam-core-v7.0.0-alpha.3) - 2026-03-03

### Other

- Add MaybeSend bound on erased caller
