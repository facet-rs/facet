# facet development guide

## Website

The project's website is [facet.rs](https://facet.rs). The website source files
can be found in the `docs/` directory.

Only @fasterthanlime can deploy the website but anyone can run it locally
because [home](https://home.bearcove.cloud) is now open-source.

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

## Shipping

Only @fasterthanlime has publish rights for the crates.

They run `just ship` locally — which uses [release-plz](https://release-plz.ieni.dev).

## Running tests

Do yourself a favor, run tests with [cargo-nextest](https://nexte.st) — using
`cargo test` is _not officially supported_.

Make sure to check the platform-specific notes:

  * [for macOS](https://nexte.st/docs/installation/macos/)
  * [for Windows](https://nexte.st/docs/installation/windows/)

As of Apr 22, 2025, the 349 tests run in .742s on a MacBook Pro M4.

## Rust nightly / MSRV

facet does not use Rust nightly, on purpose. It is "the best of stable". However,
the MSRV will likely bump with every new Rust stable version for the foreseeable
future.
