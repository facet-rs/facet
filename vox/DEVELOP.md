# Development Guide

## Quick Reference

### Running Tests

```bash
# Run all Rust tests
cargo nextest run

# Run spec tests with Rust subject
just rust

# Run spec tests with TypeScript subject
just ts

# Run all language subjects
just all
```

### TypeScript Development

```bash
# Type-check TypeScript
cd typescript && pnpm check

# Or from repo root
just ts-typecheck

# Regenerate TypeScript bindings from spec-proto
cargo xtask codegen --typescript

# Or
just ts-codegen
```

### Code Generation

```bash
# Generate all language bindings
cargo xtask codegen

# Generate specific language
cargo xtask codegen --typescript
cargo xtask codegen --swift
cargo xtask codegen --go
cargo xtask codegen --java
cargo xtask codegen --python
```

### CI Checks

```bash
# Run all CI checks locally
cargo xtask ci

# Individual checks
cargo xtask test
cargo xtask clippy
cargo xtask fmt
cargo xtask doc
```

## Project Structure

- `rust/` - Rust implementation (roam, roam-session, roam-codegen, etc.)
- `typescript/` - TypeScript implementation
  - `packages/roam-core/` - Core runtime
  - `packages/roam-tcp/` - TCP transport
  - `packages/roam-ws/` - WebSocket transport
  - `generated/` - Generated bindings (don't edit manually)
  - `subject/` - Test subject for compliance suite
- `spec/` - Protocol specification
  - `spec-proto/` - Service definitions for testing
  - `spec-tests/` - Compliance test suite
- `xtask/` - Development task runner

## Test Architecture

The compliance suite (`spec-tests`) tests protocol implementations by:
1. Starting a "subject" (server implementation in any language)
2. Acting as a client to verify protocol compliance
3. Testing both server-mode and client-mode scenarios

Subject selection is via `SUBJECT_CMD` environment variable.
