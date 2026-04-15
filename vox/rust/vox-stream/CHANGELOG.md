# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/bearcove/vox/compare/vox-stream-v0.3.1...vox-stream-v0.4.0) - 2026-04-15

### Other

- Remove link permits and queue outbound sends ([#283](https://github.com/bearcove/vox/pull/283))
- Add ffi transport
- restore mpsc channel + background task, drop all mutex/oneshot debris
- rip out buf_pool, mpsc channel, and background writer task
- Health-check existing server when local lock is held
- Add local:// support to vox::serve() with flock-based exclusivity
- resolve DNS on each connect with configurable timeouts
- tcp_link_source, better types for connect etc.
- fold local transport API and add LocalLinkSource

## [0.3.0](https://github.com/bearcove/vox/compare/vox-stream-v0.2.2...vox-stream-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))
