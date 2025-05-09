### READMEs

Don't edit any `README.md` files directly — edit `README.md.in` and run `just
codegen` to regenerate the READMEs.

Run `just` to make sure that tests pass.

For doc tests, you need to do `just codegen && just doc-tests`. Remember to fix
them by editing the corresponding `README.md.in`, not `README.md`.

All crates have their own readme template, except for the `facet/` crate, which
has it in the top-level `README.md.in`

### Testing

Always use `cargo nextest run` instead of `cargo test` to run tests. Nextest provides better isolation between tests and avoids issues with shared test environments.

For example:
- Run a specific test: `cargo nextest run test_name`
- Run tests in a specific module: `cargo nextest run module_name`
- Run tests with debug output: `cargo nextest run --no-capture test_name`

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

### Def and Type enums

In the facet system, there are two separate enum types that describe types:

- `Type`: Represents the Rust type system classification. This includes:
  - `Type::User`: User-defined types like structs, enums, and unions
  - `Type::Sequence`: Sequence types like tuples, arrays, etc.
  - `Type::Primitive`: Built-in primitive types
  - `Type::Pointer`: Reference and pointer types

- `Def`: Represents common, well-known data structures for interacting with values:
  - `Def::Map`: Dictionary or map-like structures
  - `Def::List`: Ordered list or sequence of homogeneous values
  - `Def::Array`: Fixed-size homogeneous arrays
  - `Def::Option`: Optional values
  - `Def::Scalar`: Simple scalar values
  - `Def::Undefined`: Used when no specific `Def` applies; in this case, check `Type`

When working with type information:
1. First check `Def` for common collection types like maps, lists, etc.
2. For user-defined types, use `Type::User` and check for `UserType::Struct`, `UserType::Enum`, etc.
3. For tuples, use `Type::Sequence(SequenceType::Tuple)`.

This design lets facet handle both generic data structures (`Def`) and Rust's specific type system (`Type`).
