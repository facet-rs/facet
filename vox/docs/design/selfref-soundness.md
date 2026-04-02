# SelfRef Soundness Fix

## The Problem

`SelfRef<T>` is a self-referential container: it owns backing storage (heap
buffer or Arc) and a decoded value `T` that may borrow from that storage.

To make this work, `try_new` launders the backing's lifetime to `'static`:

```rust
let bytes: &'static [u8] = unsafe {
    std::slice::from_raw_parts(b.as_ptr(), b.len())
};
let value = builder(bytes)?; // value borrows from bytes with 'static lifetime
```

The self-referential pattern itself is sound (confirmed by Miri under Tree
Borrows). The backing is heap-allocated (stable address) and dropped after
the value.

The unsoundness is in the `Deref` impl:

```rust
impl<T: 'static> Deref for SelfRef<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.value }
}
```

`Deref::Target` is an associated type, not a GAT, so it can't vary with the
borrow lifetime. It always returns `&'a Message<'static>`, leaking the fake
`'static`. Safe code can then clone borrowed data out:

```rust
let reject: SelfRef<ConnectionReject<'static>> = ...;
let metadata = reject.metadata.to_vec(); // Vec<MetadataEntry<'static>>
drop(reject);                            // backing freed
// metadata now holds dangling Cow::Borrowed(&'static str) pointers
```

This is confirmed UB (Miri Stacked Borrows) and caused real corruption
(null bytes in ConnectionReject metadata).

## The Fix

### 1. `unsafe trait Reborrow`

```rust
/// # Safety
/// The implementing type must be covariant in its lifetime parameter,
/// and `Self` and `Ref<'a>` must have the same layout for all `'a`.
pub unsafe trait Reborrow: 'static {
    type Ref<'a>;
}
```

Implemented for every type stored in `SelfRef`:

```rust
unsafe impl Reborrow for Message<'static> {
    type Ref<'a> = Message<'a>;
}
unsafe impl Reborrow for ConnectionReject<'static> {
    type Ref<'a> = ConnectionReject<'a>;
}
// ... etc
```

### 2. Replace `Deref` with `get()`

```rust
impl<T: Reborrow> SelfRef<T> {
    pub fn get(&self) -> &T::Ref<'_> {
        // SAFETY: T is covariant (guaranteed by unsafe trait),
        // same layout, and 'static: 'a.
        unsafe { std::mem::transmute(&*self.value) }
    }
}
```

This returns `&'a Message<'a>` instead of `&'a Message<'static>`. The inner
lifetime is now tied to the borrow, so borrowed data can't escape.

### 3. Usage pattern

```rust
// Before (Deref leaks 'static):
fn handle(msg: SelfRef<Message<'static>>) {
    let id = msg.connection_id;   // implicit deref
    msg.metadata.to_vec();        // UNSOUND: produces MetadataEntry<'static>
}

// After (get() shortens lifetime):
fn handle(msg: SelfRef<Message<'static>>) {
    let m = msg.get();            // m: &'a Message<'a>
    let id = m.connection_id;     // normal field access
    m.metadata.to_vec();          // produces MetadataEntry<'a>, can't escape
    metadata_into_owned(m.metadata.to_vec()); // forced to own if needed
}
```

### 4. `selfref_match!`

Update the macro to use `.get()` instead of implicit deref for the
discriminant check.

## Why Not Just Fix Clone?

Custom `Clone` that always produces `Cow::Owned` would prevent this specific
bug, but:
- `Clone` is supposed to produce identical copies
- Other borrowing patterns could still leak (any `&'static` reference)
- Doesn't fix the root cause: `Deref` exposing fake lifetimes

## Concrete Types Needing `Reborrow` Impls

- `Message<'static>`
- `RequestMessage<'static>`, `RequestCall<'static>`, `RequestResponse<'static>`
- `ConnectionMessage<'static>`
- `ConnectionOpen<'static>`, `ConnectionAccept<'static>`, `ConnectionReject<'static>`
- `ConnectionClose<'static>`
- `Frame<'static>` (stable conduit)
- `ChannelItem<'static>`, `ChannelClose<'static>`, `ChannelReset<'static>`
- `ChannelMessage<'static>`

## Ecosystem Precedent

Surveyed six self-referential crates. The unanimous pattern: **no `Deref`
for lifetime-bearing payloads**. Access is always via explicit accessor or
closure.

- **yoke** — closest match. Stores `Foo<'static>` with erased lifetime,
  exposes via `get<'a>(&'a self) -> &'a <Y as Yokeable<'a>>::Output`.
  `Yokeable` is exactly our `Reborrow` trait: it encodes "same type,
  different lifetime." No `Deref`.

- **self_cell** — advertises "no leaking internal lifetime." API:
  `borrow_dependent<'a>(&'a self) -> &'a Dependent<'a>`. No `Deref`.

- **ouroboros** — proc-macro framework, generates `borrow_*` accessors +
  `with()`/`with_mut()`. Surfaces covariance via `#[covariant]` /
  `#[not_covariant]` annotations. No direct field access.

- **rental** (archived) — the cautionary tale. Only allowed `deref_suffix`
  when the deref target *could not leak* the hidden lifetime. Our
  `Message<'static>` carries the erased lifetime in its public type, so
  it's exactly the kind of thing rental would forbid via `Deref`.

- **selfref** — uses GATs (`Kind<'this>`) as a workaround for missing HKTs.
  Close in spirit to our `Reborrow` trait.

- **owning_ref** — adjacent but different problem. Known unsound.

**Conclusion:** our design is a tiny yoke/self_cell hybrid. `Reborrow` ≈
`Yokeable`, `.get()` ≈ yoke's `.get()`.

## Migration

1. Add `Reborrow` trait + impls
2. Add `get()` method
3. Remove `Deref` impl
4. Fix compilation errors: add `let m = msg.get();` at each use site
5. Update `selfref_match!` macro
6. Verify Miri passes with the reproduction test
