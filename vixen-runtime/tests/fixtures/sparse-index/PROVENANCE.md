Captured crates.io sparse-index rows for vix sparse-index ingestion tests.

Source cache:

`~/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f/.cache`

The files under `snapshot-2025-03-04/` are newline-delimited JSON objects
extracted from Cargo's binary sparse cache format and filtered to rows with
`pubtime <= 2025-03-04T00:00:00Z`.

The snapshot is committed because a live crates.io sparse-index read is an
effect over a moving read set. The demo pins a snapshot and serves it through
the fake fetch backend instead of depending on the current online index.
