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

Do not edit `README.md` files directly, edit `README.md.in` in the respective
folders.

Similarly, don't edit `tuples_impls.rs`, edit the `tuples_impls.rs.j2` next to
it, and then run `just codegen` — the code is in `facet-codegen/`
