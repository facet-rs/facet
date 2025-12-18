+++
title = "Keys and Kind IDs"
weight = 14
+++

This page documents how picante identifies “what is being queried” in a stable, persistable way.

Primary implementation: `crates/picante/src/key.rs`.

## `QueryKindId`

Every ingredient has a `QueryKindId(u32)`.

Macros generate stable ids using `QueryKindId::from_str(...)`, which is a 32-bit FNV-1a hash over UTF-8 bytes.

Stability assumptions:

- if the string passed to `from_str` changes (module path, function name, etc.), the id changes
- cache files rely on these ids being stable across runs
- collisions are theoretically possible (32-bit hash), but treated as “should not happen”; within a single db instance, kind ids must be unique (persistence rejects duplicates at save/load time)

## `Key`

For dependency graphs, invalidation, and in-flight registries, picante uses an erased `Key`:

- `bytes`: `facet-postcard` encoded representation of the concrete key
- `hash`: a deterministic hash of `bytes` used for diagnostics/tracing

Equality for `Key` is exact byte equality (not hash equality).

### Hashing details

`Key::hash()` is a deterministic hash of the encoded bytes intended for diagnostics/tracing only. It is not used as a correctness boundary (correctness uses full byte equality).

## `DynKey` and `Dep`

- `DynKey { kind, key }` identifies one specific query/input record
- `Dep { kind, key }` is the recorded edge format used in dependency lists
