# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Remove the implicit `From<DriverCaller> for ()` conversion. Use `NoopCaller` with `establish::<NoopCaller>(...)` when you want root liveness without a root client API.
