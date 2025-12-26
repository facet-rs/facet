# Releasing rapace

This repository uses [release-plz](https://release-plz.dev) to automate version
bumping, changelog generation, and publishing to crates.io. The workflow lives
in `.github/workflows/release-plz.yml` and runs on every push to `main` as well
as on manual `workflow_dispatch` invocations.

## Crates that ship to crates.io

Only a subset of workspace members are published:

- `rapace`
- `rapace-core`
- `rapace-macros`
- `rapace-registry`
- `rapace-tracing`
- `rapace-transport-mem`
- `rapace-transport-stream`
- `rapace-transport-websocket`
- `rapace-transport-shm`

Everything else (`rapace-http`, `rapace-testkit`, `rapace-explorer`,
`rapace-wasm-client`, demos, xtask helpers, etc.) is marked `publish = false`
so release-plz will ignore it.

## Tokens and secrets

Trusted Publishing is enabled through `rust-lang/crates-io-auth-action@v1`,
which exchanges a short-lived crates.io token using GitHub’s OIDC identity. To
finish setup:

1. On crates.io, for each published crate, go to **Settings → Owners → Trusted
   publishing**, add this repository, and grant publish+manage permissions.
2. No long-lived `CARGO_REGISTRY_TOKEN` secret is required; the workflow step
   writes the ephemeral token to the `CARGO_REGISTRY_TOKEN` env var right before
   calling `release-plz release`.
3. The default `GITHUB_TOKEN` continues to power release-plz PR updates.

## Typical workflow

1. Land feature work on `main`.
2. Wait for the `Release / Release PR` job to run. If a release is warranted,
   release-plz will either create or update a `release` pull request that bumps
   versions and changelog entries.
3. Review and merge the release PR.
4. The `Release / Publish` job runs automatically on the merge commit and
   publishes tagged crates using release-plz.
5. Monitor the workflow logs to ensure the release succeeded.

You can also trigger the workflow manually: `gh workflow run release-plz.yml`.
This is useful when you need to force a release after cherry-picking fixes.
