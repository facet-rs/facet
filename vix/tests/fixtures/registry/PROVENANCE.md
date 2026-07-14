# Fixture registry

An offline package registry for the fetch/extract rungs (075–077). Nothing
here comes from crates.io.

- `tokio-1.52.3.crate` — a FABRICATED plain (uncompressed) ustar archive,
  exactly 4096 bytes (blocking factor 8), containing a single root-level
  `Cargo.toml` with `name = "tokio"`. It is not the real tokio crate; the
  name and the 4096-byte length are pinned by rungs 075–077.
- `manifest` — the lock-time registry index: artifact name, provenance URL
  (`fixture://` scheme, resolved against this directory, never the network),
  and the vix blake3 ContentHash naming the archive Blob. The
  `fixture_registry_manifest_is_locked_to_the_archives` test in
  `vix/tests/fixture_registry.rs` recomputes every row and keeps the manifest
  honest; on mismatch it prints the expected row.
