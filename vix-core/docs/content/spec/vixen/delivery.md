+++
title = "Delivery"
weight = 3
+++

A program describes values and writes nothing. `place` is placement of a demand
subgraph, not export (`machine.primitive.exec-is-placement-agnostic`). So the
last step of a build — the artifact appearing where a person can run it — is a
CLI act over a demanded root, and it needs no language surface at all.

> r[vixen.delivery.result]
>
> [DESIGN] `vx build <root>` demands the root and materializes it at
> `./result`, overridable with `--out <path>`. A `Tree` root becomes a directory;
> a `Blob` root becomes a file. Any other value is a typed failure — rendering a
> record to a filesystem is not delivery, it is a guess.
>
> Materialization is atomic from a reader's side, and the requirement is the
> property, not a syscall sequence: **a failed build leaves the previous
> `result` exactly as it was, and no reader ever observes a half-written one.**
> An earlier draft prescribed "build into a sibling temporary, then rename over
> the destination", which does not implement it — `rename(2)` is not atomic
> across filesystems, and it fails `ENOTEMPTY` when the destination is a
> non-empty directory, which is the `Tree` case exactly. How the property is
> delivered is the implementation's problem; that it holds is this rule's.

> r[vixen.delivery.result-is-a-copy-not-a-link]
>
> [DESIGN] `result` is materialized content, not a symlink into the store.
> Nix links because a store path *is* the artifact's home; vix's store is
> content-addressed and chunked, with no per-artifact directory to point at.
> Materializing is therefore the honest MVP, and the executable bit and symlinks
> of `machine.identity.tree-model` are what make it faithful.

This is explicitly the smallest thing that finishes the loop, chosen so the 0.1
demo can end with running the binary. It is expected to change: a store with
addressable artifact roots, GC roots, multi-root builds, and `--json` receipts
all argue for a different surface. None of them are 0.1, and none of them are
harder to reach from here than from nowhere.

## Deliberately not

Not deferred — refused. Each names the system that does the thing and the
reason we will not.

- **Nothing runs at delivery.** `npm install` runs a package's `preinstall`,
  `install`, and `postinstall` scripts on the installing machine as the
  invoking user; `--ignore-scripts` is the opt-out and it is not the default.
  cargo compiles and runs every dependency's `build.rs` on your machine, with
  no sandbox. Both make acquiring a dependency and executing its author's code
  the same act. `vx build` materializes a value that was already computed:
  there is no hook, no lifecycle script, no `postinstall`, and no place to put
  one. Everything that ran, ran during evaluation under the machine's effect
  discipline, and is in a receipt. Delivery is a copy.

- **Nix's `result` symlink and the GC root behind it.** `nix-build` leaves
  `./result` as a symlink into `/nix/store`, and it also writes an *indirect
  root* — an entry under `/nix/var/nix/gcroots/auto/` pointing back at your
  `./result` — so the store path survives `nix-collect-garbage` for exactly as
  long as that link exists. `--no-out-link` (`--no-link` under `nix build`)
  declines it. Ours is a copy (`r[vixen.delivery.result-is-a-copy-not-a-link]`),
  and the consequence deserves stating rather than discovering: **`result` is
  detached.** Nothing roots the store on your behalf, deleting `result` frees
  nothing, and nothing in the store knows a `result` was ever produced. A GC
  root is a store-lifetime contract, and the store has no addressable artifact
  root to hang one on — inventing one here would be inventing it in the wrong
  page.

- **`target/` as cache and output at once.** cargo puts the deliverable at
  `target/release/<bin>`, inside the same directory as every incremental
  artifact, fingerprint, and build-script output — which is why `cargo clean`
  destroys the cache and the output together, and why there is no stable way
  to ask for only the output: `--out-dir` no longer exists and its replacement
  `--artifact-dir` is nightly-only (rust-lang/cargo#6790). `result` is delivery
  and nothing else. It holds no cache, no later build consults it, and deleting
  it costs a re-materialization and nothing more. The store is the cache, it
  lives elsewhere, and the two have separate lifetimes because they answer
  separate questions.

- **A symlink forest and a runfiles tree.** `bazel build` leaves `bazel-bin`
  and its siblings in the workspace root as symlinks into an output base under
  the user's cache, and the executable found there does not stand alone:
  "During the execution phase, Bazel creates a directory tree containing
  symlinks pointing to the runfiles", rooted at the binary's own path plus
  `.runfiles`, and a binary that cannot find that tree cannot find its data.
  The artifact is therefore a view of a cache plus a directory the build
  materialized around it — copy the binary somewhere and you have moved the
  smaller half. A delivered `result` must be a thing you can hand to somebody:
  `machine.identity.tree-model`'s executable bit and symlinks are carried
  precisely so that what lands is what was built, complete.

- **Owning your environment.** `nix profile install` mutates a profile — a
  generation chain under `/nix/var/nix/profiles` that `~/.nix-profile` points
  into, with `nix profile history`, `rollback`, and `wipe-history` to manage it
  — so that a binary shows up on `PATH`. `cargo install` copies into
  `~/.cargo/bin`, where names collide and `--force` is the answer. direnv swaps
  your environment when you `cd`. All of it is useful; none of it is a build
  system's business. `r[vixen.delivery.result]` ends at a path — a file or a
  directory, where you asked for it. vixen will not put anything on your
  `PATH`, install a shell hook, keep generations, or hold an opinion about
  which build is "current".
