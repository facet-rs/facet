Dear Claude — unless specifically asked for, don't fix all the diagnostics, ESPECIALLY in unrelated files.

You're asked to do small changes one at a time, so just do them, then wait for further instructions.
Don't go off too far on your own. If you get yourself thinking for too long, stop. Just ask for more
context or ask to make the task smaller. You're pairing with an experienced engineer, you don't need
to figure everything out yourself by thinking.

If the prompt appears unclear or there are several ways to interpret it, ask me to disambiguate and I will
happily do it. I will be happier clarifying ahead of time than undoing your stuff.

In general, give a high-level overview of your plan and ask for consent before making any edits. Thanks.

Don't edit any `README.md` files directly — edit `README.md.in` and run `just codegen` to
regenerate the README.

Run `just quickcheck` to make sure that tests pass.

DON'T FIX DIAGNOSTICS BY YOURSELF. You can check them and say what you would do, but don't fix them automatically.

Don't keep going. Ask for the next thing to do.

In crates like `facet-yaml`, `facet-json`, etc. — we cannot
depend on `facet` directly. For tests to pass, we have to do:

```rust
use facet_derive::Facet;
use facet_core as facet;
```

On top. This makes the derive macro work. This is only for tests.
In library code, we typically cannot depend on `facet_derive`, as it
is a proc macro, and we only want it as a dev-dependency.
