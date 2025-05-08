### READMEs

Don't edit any `README.md` files directly — edit `README.md.in` and run `just
codegen` to regenerate the READMEs.

Run `just` to make sure that tests pass.

For doc tests, you need to do `just codegen && just doc-tests`. Remember to fix
them by editing the corresponding `README.md.in`, not `README.md`.

All crates have their own readme template, except for the `facet/` crate, which
has it in the top-level `README.md.in`

### Pre-commit Hooks

When committing changes, facet-dev will run to check for code generation and formatting changes.
This presents an interactive menu that requires user input which can be problematic for bots and
automated processes.

To bypass the interactive menu and automatically apply all fixes, you can set the 
`FACET_PRECOMMIT_ACCEPT_ALL=1` environment variable when running git commit:

```bash
FACET_PRECOMMIT_ACCEPT_ALL=1 git commit -m "Your commit message"
```

This is particularly useful for automated systems and bots that cannot provide interactive input.

### Tuple implementations

The file `tuples_impls.rs` in facet-core is generated from `gen_tuples_impls.rs`
in the `facet-codegen` crate. If you see any errors in it, do not correct them,
simply make a note of it and I will take care of it.

### Dependencies

crates like `facet-yaml`, `facet-json`, only have have a _dev_ dependency on
`facet`. For regular dependencies, they only have `facet-core`, `facet-reflect`.
This is to keep `facet-derive` out of them.

### Testing Derive Macros

Tests that exercise the `#[derive(Facet)]` macro cannot live in `facet-core`
because it does not depend on `facet-derive`. Such tests should either be
snapshot tests in `facet-derive-emit` or integration tests in the main `facet`
crate, which brings all the necessary components together.
