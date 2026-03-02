# Zero-copy TODO

## Scatter/gather serialization in facet-postcard

Spec: `zerocopy.scatter.*` in `docs/content/spec/zerocopy.md`.

facet-postcard needs a new API that walks a `Facet` value and produces a
scatter plan (staging buffer + segment list) instead of writing directly
to a `Vec`. See GitHub issue on facet-rs/facet.

Blocked on: https://github.com/facet-rs/facet/issues/2065

## `'static` constraint on conduit send path

Both conduits require `T: Facet<'static> + 'static`. The spec
(`zerocopy.send.borrowed`, `zerocopy.send.borrowed-in-struct`) says the
send path should accept borrowed data like `&[u8]` or structs containing
`&str`. The trait hierarchy (`Conduit<T>`, `ConduitTx<T>`,
`ConduitTxPermit<T>`) needs lifetime parameters.

Prerequisite for the vertical slice: `#[roam::service]` with a method
that takes `&[u8]`, through codegen → RPC machinery → conduit → link.

## Replay buffer copy path in StableConduit

After scatter/gather lands, StableConduit should:
1. Build scatter plan from `Frame<T>`
2. `alloc(plan.total_size())` → write slot
3. Write plan into slot
4. `memcpy` slot bytes → replay buffer
5. `commit()`

Instead of the current: `to_vec` → `clone` → `copy_from_slice`.
