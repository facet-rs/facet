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
> [DESIGN, MVP] `vx build <root>` demands the root and materializes it at
> `./result`, overridable with `--out <path>`. A `Tree` root becomes a directory;
> a `Blob` root becomes a file. Any other value is a typed failure — rendering a
> record to a filesystem is not delivery, it is a guess.
>
> Materialization is atomic from the caller's side: build into a sibling
> temporary, then rename over the destination, replacing any previous `result`.
> A failed build leaves the previous `result` untouched.

> r[vixen.delivery.result-is-a-copy-not-a-link]
>
> [DESIGN, MVP] `result` is materialized content, not a symlink into the store.
> Nix links because a store path *is* the artifact's home; vix's store is
> content-addressed and chunked, with no per-artifact directory to point at.
> Materializing is therefore the honest MVP, and the executable bit and symlinks
> of `machine.identity.tree-model` are what make it faithful.

This is explicitly the smallest thing that finishes the loop, chosen so the 0.1
demo can end with running the binary. It is expected to change: a store with
addressable artifact roots, GC roots, multi-root builds, and `--json` receipts
all argue for a different surface. None of them are 0.1, and none of them are
harder to reach from here than from nowhere.
