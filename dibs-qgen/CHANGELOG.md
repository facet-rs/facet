# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/bearcove/dibs/compare/dibs-qgen-v0.0.0...dibs-qgen-v0.1.0) - 2026-06-02

### Other

- Add @float param type (f64 / DOUBLE PRECISION)
- @jsonb cast becomes \$N::text::jsonb (was \$N::jsonb)
- TraceErr emits structured tracing on QueryError
- @jsonb param type → $N::jsonb cast at the binding site
- Upgrade deps to stable releases
- Wire up FunctionSpec filter validation with proper error handling ([#14](https://github.com/bearcove/dibs/pull/14))
- Remove duplicate AST layer in dibs-query-gen  ([#12](https://github.com/bearcove/dibs/pull/12))
