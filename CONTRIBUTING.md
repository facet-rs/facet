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

Do not edit `README.md` files directly, edit `templates/README.md.j2` in the
respective folders.
