# Trame Verification with Kani

## Why Proofs?

We already write unit tests and fuzz tests. But Trame contains unsafe code that
tracks whether memory is initialized, whether memory is allocated, calls vtables,
etc. This is tricky to get right.

With Kani, we want to **prove** that there is no sequence of operations that
brings us into an invalid state. Not just test some sequences - prove it for all
possible sequences.

## Goals and Non-Goals

There are two levels we could verify at:

1. **The entire tree** - frames connected by parent links, arena allocation,
   the works. This is what we'd ideally do, but it's very hard with Kani because
   the complexity runs away.

2. **Individual frames** - we have a frame, we tell it what its context is
   (parent info, children info), we apply an operation, and we verify the frame
   does the right thing. This is more tractable.

We're going with approach #2 for now.

### Non-Goals

- Testing the arena in isolation (it's trivial, not where bugs are)
- Testing the tree structure in isolation (also not where bugs are)
- Having two implementations of business logic

The bugs are in tracking memory initialization, allocation, vtable calls. That's
what we verify.

## Frame Kinds

Trame has these frame kinds:

1. **StructFrame** - struct with N fields, tracks which are initialized
2. **EnumFrame** - enum, tracks selected variant
3. **VariantFrame** - the data inside an enum variant (like a struct)
4. **PointerFrame** - Box/Rc/Arc, tracks the inner value
5. **ListFrame** - Vec, tracks elements
6. **MapFrame** - HashMap/BTreeMap, tracks entries
7. **SetFrame** - HashSet/BTreeSet, tracks elements
8. **OptionFrame** - Option, tracks Some/None and inner value
9. **ResultFrame** - Result, tracks Ok/Err and inner value
10. **MapEntryFrame** - a single map entry (key + value)

## One Implementation, Two Backends

We want ONE implementation of the business logic. The proofs run through the
actual business logic, not a simplified model.

What changes is the **backend**. The backend handles:
- Memory writes
- Tracking which memory is initialized
- Calling vtables (default_in_place, drop_in_place, etc.)
- Allocation

Two backends:
- **Real backend** - actually does the unsafe operations
- **Verified backend** - tracks state without doing unsafe ops, for Kani

## Shapes

We want to test with actual Shapes from Facet.

A Shape is the schema for reflection of a type. It tells Trame what we're
building: is it a struct with N fields at these offsets? An enum with these
variants? A Vec? The Shape contains:

- Layout (size, alignment)
- Vtables for operations (default_in_place, drop_in_place, etc.)
- Type information (struct fields, enum variants, etc.)
- Everything needed to manipulate values at runtime

**Problem:** Shapes are statics - they're part of the compiled code, full of
`&'static` references, potentially cyclic. We can't derive `kani::Arbitrary`
for Shape directly.

**Solution:** We have a `ShapeDesc` abstraction - a small, Copy handle that
wraps either a static Shape or a dynamically created Tuple2Shape (for map
entries). Because we have this abstraction, we could generate shapes dynamically
for Kani - create arbitrary shape configurations at runtime and verify that
everything still works.

This is better than what we currently do with AFL fuzzing, which uses a fixed
set of real Rust types declared in the harness. With ShapeDesc, we can go
beyond that and verify properties for arbitrary shape configurations.

## The State Machine

Each "slot" (field, element, pointee, etc.) is in one of three states:

```
Unallocated  --alloc-->  Allocated  --init-->  Initialized
                              ^                     |
                              |----drop_in_place----|
                              |
Unallocated  <--dealloc-------+
```

**Bad transitions (the bugs we want to catch):**
- `init` when already `Initialized` - double init, overwrites without drop
- `drop_in_place` when `Allocated` - drop uninitialized memory (UB)
- `dealloc` when `Initialized` - leak, didn't drop first
- `dealloc` when `Unallocated` - double free
- read/use when not `Initialized`
- any operation when `Unallocated` (except alloc)

**The invariant:** what the tracking says == what the actual state is.

## Proposed Abstraction (Draft)

The actual unsafe operations in Trame:

1. `alloc` - allocate memory (std::alloc::alloc)
2. `dealloc` - deallocate memory (Frame::dealloc_if_owned)
3. `copy_from` - copy bytes into uninitialized memory (init)
4. `call_default_in_place` - construct default value (init)
5. `call_drop_in_place` - drop an initialized value

These happen in:
- `partial.rs` - Partial::alloc_shape
- `partial/set.rs`, `partial/option.rs`, `partial/result.rs` - inner allocations
- `slab.rs` - map/set entry staging
- `frame.rs` - Frame::copy_from, Frame::uninit, Frame::dealloc_if_owned
- `shape_desc.rs` - call_drop_in_place, call_default_in_place

### Rough Interface

```rust
/// Identifies a slot within the partial value being built.
/// A slot is a single "cell" that can be unallocated/allocated/initialized.
/// Examples: a struct field, an array element, a pointer's pointee.
struct SlotId(u32);

/// Identifies an allocation (a contiguous region of memory).
/// An allocation contains one or more slots.
struct AllocId(u32);

/// Operations on individual slots (init/drop state tracking).
trait SlotOps {
    /// Transition: Allocated -> Initialized
    /// Panics (in verified backend) if slot is not in Allocated state.
    fn mark_init(&mut self, slot: SlotId);
    
    /// Transition: Initialized -> Allocated  
    /// Panics (in verified backend) if slot is not in Initialized state.
    fn mark_uninit(&mut self, slot: SlotId);
    
    /// Query current state.
    fn is_init(&self, slot: SlotId) -> bool;
}

/// Operations on allocations (alloc/dealloc).
trait AllocOps {
    /// Transition: Unallocated -> Allocated
    /// Returns an AllocId for the new allocation.
    fn alloc(&mut self, layout: Layout) -> AllocId;
    
    /// Transition: Allocated -> Unallocated
    /// Panics (in verified backend) if any slot in this allocation is Initialized.
    fn dealloc(&mut self, alloc: AllocId);
    
    /// Get a slot within an allocation.
    fn slot(&self, alloc: AllocId, offset: usize) -> SlotId;
}
```

### Open Questions

1. **How do slots relate to allocations?** A struct allocation has N slots (one
   per field). A Vec allocation has dynamic slots. How do we model this?

2. **Nested structures:** A struct field might itself be a struct. Is that one
   slot or multiple? Probably one slot from the parent's perspective - the
   child's internal structure is tracked separately if we Stage into it.

3. **Threading through the code:** Currently `shape.call_drop_in_place(ptr)` is
   called directly. We'd need to change this to something like:
   ```rust
   self.ops.mark_uninit(slot_id);
   shape.call_drop_in_place(ptr);
   ```
   This touches a lot of code.

4. **What's a slot, really?** Is it per-field? Per-byte? Per "thing we track"?
   The current code tracks at field granularity (IndexedFields), so probably
   slots = fields/elements.

5. **The Shape problem:** In the real backend, we call `shape.call_drop_in_place`.
   In the verified backend, we... don't? But we still need to know WHICH slot
   we're operating on. The slot ID needs to be derivable from the pointer/offset
   somehow.
