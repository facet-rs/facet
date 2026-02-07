# `in_process_bench`

VFS-shaped in-process RPC benchmark for `roam-session`.

It models:
- frequent `read(item_id, offset, len)` returning large `Vec<u8>` blobs
- interleaved `get_attributes(item_id)` calls

This is intended for profiling dispatch/serialization/type-plan costs without network noise.

## Run

```bash
cargo run -p roam-session --example in_process_bench -- --iterations 120000 --warmup 2000
```

Optional tuning:
- `--iterations N`
- `--warmup N`
- `--file-size BYTES` (default: `33554432`, i.e. 32 MiB)

## Profile with samply

Cached type-plan path:

```bash
cargo samply -p roam-session --example in_process_bench -- --iterations 200000 --warmup 2000
```

## Notes

- `cargo samply` in this setup is invoked as `cargo samply ...` (not `cargo samply record`).
- `cargo samply` already uses its own profile by default, so you typically don't pass `--release`.
