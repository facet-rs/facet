# Tracking

How does Partial track what's initialized?

## The problem

When building a struct incrementally, we need to know:
1. Which fields have been set
2. Which fields still need values (or defaults)
3. When it's safe to "finalize" the value

## Current approach (apiv1)

The `Tracker` enum has variants for each container type:

```rust
enum Tracker {
    Scalar,
    Struct { iset: ISet, current_child: Option<usize> },
    Enum { variant: &'static Variant, variant_idx: usize, data: ISet, current_child: Option<usize> },
    Array { iset: ISet, current_child: Option<usize> },
    List { current_child: Option<usize>, element_count: usize },
    Map { insert_state: MapInsertState },
    Set { current_child: bool },
    Option { building_inner: bool },
    // ... etc
}
```

`ISet` is a bitset tracking which fields/elements are initialized (up to 63).

## Questions for apiv2

1. **Do we still need per-type tracker variants?**

   With the new ops, the "what kind of thing" is implicit in the Shape. Do we need `Tracker::Struct` vs `Tracker::List` etc, or can we unify?

2. **current_child tracking**

   In apiv1, `current_child` tracks "which field/element we're currently building" for path derivation in deferred mode. With apiv2's explicit paths in `Set`, do we still need this?

3. **element_count for lists**

   Lists track `element_count` separately from the Vec's actual len (because in deferred mode, `set_len` is called later). This seems still necessary.

4. **MapInsertState complexity**

   Maps have a state machine for key/value insertion. With apiv2's `Insert { key: Move, value: Source }`, the key is always complete. Does this simplify the state machine?

5. **Frame storage in deferred mode**

   Currently frames are stored by `Path` (derived from the frame stack). With explicit paths in ops, can we simplify this?

## TODO

- [ ] Decide on unified vs per-type tracking
- [ ] Determine what state is actually needed per frame
- [ ] Design the deferred frame storage mechanism
