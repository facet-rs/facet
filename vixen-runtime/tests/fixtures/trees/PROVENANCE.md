# Tree fixtures

Small file trees shipped with the ratchet suite (`fixture_tree(name)`),
authored by hand for the tree band (rungs 071+):

- `small-crate` — a minimal crate shape: `Cargo.toml` (contains `[package]`),
  `src/lib.rs`, `src/main.rs`. Rung 071 projects `Cargo.toml` and asserts
  `src/lib.rs` is never read; rung 072 globs `src/*.rs` (two files, `lib` first
  in canonical order).
- `locked-crate` — the same minimal crate shape WITH a `Cargo.lock`: a v4 lock
  whose three `[[package]]` blocks cover both spellings a lock uses (two
  registry packages carrying `source` + `checksum`, one local root carrying
  neither and listing `dependencies`). It exists separately from `small-crate`
  because that tree's contents are pinned by rungs 071–072; the sequence-decode
  band needs an array-of-tables document and must not perturb them.
- `touched-fixture` — a tiny tree for `rerun_with` fixture mutations
  (receipts band).
- `readme-changed` — a tiny tree whose README is the mutation target
  (receipts band).

Nothing here is fetched from the network; contents are stable bytes the
runner reads through the recording fixture store.
