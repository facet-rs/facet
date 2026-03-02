# SHM v7 Golden Vectors

These fixtures are generated from the active `rust/roam-shm` implementation
for the current SHM v7 spec (`docs/content/spec/shm.md`).

Regenerate with:

```bash
cargo run -p roam-shm --bin swift_shm_fixtures
```

Outputs:
- `segment_header.bin`
- `segment_layout.bin`
- `frame_header.bin`
- `slot_ref.bin`
- `frame_inline.bin`
- `frame_slot_ref.bin`
- `frame_mmap_ref.bin`
