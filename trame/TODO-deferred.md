# trame Deferred Mode TODO

Deferred mode support for out-of-order construction. This is a separate phase of work.

## Deferred Mode

No deferred mode support exists. All frames must be complete before `End`.

**Goal:** Allow incomplete frames to be stored for later completion.

**Implementation:**
1. Add `FrameFlags::DEFERRED` flag
2. Add `deferred_boundary: Option<Idx<Frame>>` to `Partial`
3. When entering deferred mode, set the boundary
4. On `End` of deferred frame: store incomplete, don't error
5. On `End` of deferred boundary: validate entire subtree
6. Path navigation should find existing child frames for re-entry

```rust
// Example: build struct incrementally
partial.apply(&[
    Op::set().at(0).stage(),  // Enter field 0, mark deferred
    Op::set().at(0).imm(&mut x),
    Op::End,  // Stores incomplete, pops to parent
    // ... do other things ...
    Op::set().at(0).at(1).imm(&mut y),  // Re-enter field 0, complete it
    Op::End,
])?;
```

## Re-entry Support

Multi-level paths need to support re-entering existing child frames (not just creating new ones).

When navigating `[Field(0), Field(1)]`:
- Check if field 0 already has a child frame in `IndexedFields`
- If valid frame index exists, navigate to it instead of creating new
- This enables completing partially-built nested structures

## Deferred Validation

When exiting a deferred subtree (climbing past the deferred boundary):
1. Validate the entire subtree is complete
2. If incomplete nodes remain, error
3. Clear deferred flags
