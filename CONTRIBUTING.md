# Contributing to facet

Get yourself just (`brew install just` / `cargo [b]install just`), and run it:

```
just
```

Does it run? Then yay! CI will most likely pass.

```
just miri
```

That one checks for UB, memory unsafety, etc.

## Generated code

Some `README.md` files are generated with `cargo reedme`.
If a README contains `cargo-reedme` markers, edit crate-level rustdoc in `src/lib.rs`
instead of editing the generated section directly.
