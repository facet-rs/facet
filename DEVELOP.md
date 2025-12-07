# facet development guide

## Website

The project's website is [facet.rs](https://facet.rs). The website source files
can be found in the `docs/` directory.

Only @fasterthanlime can deploy the website but anyone can run it locally 
with [dodeca](https://github.com/bearcove/dodeca) just install it and run
`ddc serve` in the root of this repo

## Collaboration and Contribution Guidelines

Try to submit changes as pull requests (PRs) for review and feedback, even if
you're part of the organization. We all champion different things: @fasterthanlime
facet-json, @tversteeg face-toml, @Veykril language stuff, @epage has good
advice when it comes to crate design and no_std — and other stuff!

## Pull Request Best Practices

Prefer smaller, incremental PRs over large, monolithic ones. This helps avoid
stagnation and makes review easier, even though initial setup PRs may be large.

## Staying Up to Date

Expect some churn as APIs evolve. Keep up with changes in core libraries (like
facet-reflect and facet-core) as needed. Coordination during rapid development
is key.

## Version Control & Checks

You’re welcome to use alternative version control tools (like jujutsu/jj), but
always run checks such as `just precommit`, `just prepush`, or `just ci` before
merging to avoid CI failures.

## Regenerating Documentation

Use `just gen` to regenerate the `README.md` whenever documentation needs to be
updated — that's normally part of the precommit hook.

## Precommit/prepush hook

Sometimes the hook just won't pass, and in that case you can just pass
`--no-verify` to either `git commit` or `git push`. Nobody'll get mad at you
except for the CI pipeline.

Those hooks are only here to save you from back-and-forths with CI! They should
serve you, they're a sign, not a cop, etc.

## Shipping

Only @fasterthanlime has publish rights for the crates.

They use [release-plz](https://release-plz.ieni.dev).

## Running tests

Do yourself a favor, run tests with [cargo-nextest](https://nexte.st) — using
`cargo test` is _not officially supported_.

Make sure to check the platform-specific notes:

  * [for macOS](https://nexte.st/docs/installation/macos/)
  * [for Windows](https://nexte.st/docs/installation/windows/)

As of Jul 25, 2025, the 408 tests run in .547 on a MacBook Pro M4.

## Inline string validation workflow

- Use `just test -p facet-value --features bolero-inline-tests` (this wraps `cargo nextest run`) whenever you touch inline string logic so the deterministic + property suites run.
- Run cross-target coverage with `just test-i686` at least once before merging to ensure the inline encoding behaves on 32-bit pointers.
- Nightly tooling:
  - `just miri -p facet-value` already exists; it runs the crate's test suite under strict provenance.
  - `just asan-facet-value` (or the `-ci` variant) exercises the crate with the address sanitizer.
- Fuzzing:
  - `just fuzz-smoke-value` runs a ~60s libFuzzer smoke test for the general dynamic-value target.
  - `just fuzz-smoke-inline` hones in on inline string mutations; wire both into CI smoke stages.
- For long fuzz sessions, prefer `cargo fuzz cmin` + `heaptrack target/debug/fuzz_inline_string ...` or run under `valgrind --tool=memcheck` to confirm no allocator leaks appear when inline/heap transitions churn.

## Rust nightly / MSRV

facet does not use Rust nightly, on purpose. It is "the best of stable". However,
the MSRV will likely bump with every new Rust stable version for the foreseeable
future.
