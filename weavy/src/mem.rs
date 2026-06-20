//! Shared typed-memory thunk vocabulary for lowered programs.
//!
//! These are the raw, type-erased hooks a front door binds when a lowered memory
//! program needs operations the bytecode engine cannot derive from layout facts:
//! constructing `Vec`/map/set handles, reading `Option`/`Result` presence,
//! validating string bytes, and delegating opaque payloads.

/// A type-erased "write this field's default in place" operation, supplied by
/// the front door for a reader-only field that can be filled locally.
///
/// The engine never knows the field type; it calls the thunk, passing back the
/// opaque `ctx` the front door understands.
pub type DefaultThunk = unsafe extern "C" fn(ctx: *const (), slot: *mut u8);

/// A reader-only-default op's payload. Initializes the reader field at
/// `base + offset` to its default in place, reading no wire bytes.
#[derive(Clone, Debug)]
pub struct DefaultOp {
    /// Where the reader field lives, relative to the base.
    pub offset: usize,
    /// Opaque per-field context the front door binds (passed to `default`).
    pub ctx: *const (),
    /// Initialize the uninitialized reader field at `slot` to its default.
    pub default: DefaultThunk,
}

/// Type-erased operations on an owned sequence handle, supplied by the front
/// door. A `Vec`'s in-memory layout is not something an engine may assume, so it
/// never pokes the handle directly — it calls these. `ctx` is an opaque per-type
/// pointer the front door understands; the engine passes it back untouched.
///
/// Decode is engine-owned: the engine allocates and fills the element buffer
/// itself, then [`from_raw_parts`](Self::from_raw_parts) adopts it into the
/// handle in one move.
#[derive(Clone, Copy, Debug)]
pub struct SeqThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// Construct the sequence at `list` from a buffer of `len` elements the engine
    /// allocated with `cap` capacity.
    ///
    /// The buffer must have been allocated with the element type's array layout
    /// (the engine guarantees this).
    pub from_raw_parts:
        unsafe extern "C" fn(ctx: *const (), list: *mut u8, ptr: *mut u8, len: usize, cap: usize),
    /// The sequence's current element count.
    pub len: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> usize,
    /// A pointer to the sequence's contiguous element storage.
    pub data: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> *const u8,
}

/// Type-erased operations on an owned set handle, supplied by the front door.
/// The engine never assumes the set's in-memory layout: encode iterates borrowed
/// elements, and decode initializes the set then inserts each decoded element.
#[derive(Clone, Copy, Debug)]
pub struct SetThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// The set's current element count.
    pub len: unsafe extern "C" fn(ctx: *const (), set: *const u8) -> usize,
    /// Initialize the uninitialized set at `set` with room for `cap` entries.
    pub init_with_capacity: unsafe extern "C" fn(ctx: *const (), set: *mut u8, cap: usize),
    /// Insert `*value` into the initialized set, moving it out of the scratch
    /// buffer. Returns `false` when the element was already present.
    pub insert: unsafe extern "C" fn(ctx: *const (), set: *mut u8, value: *mut u8) -> bool,
    /// Build a stateful iterator over the initialized set.
    pub iter_init: unsafe extern "C" fn(ctx: *const (), set: *const u8) -> *mut (),
    /// Advance the iterator, writing the next borrowed element pointer to
    /// `value_out` and returning `true`, or returning `false` at the end.
    pub iter_next:
        unsafe extern "C" fn(ctx: *const (), iter: *mut (), value_out: *mut *const u8) -> bool,
    /// Free the iterator built by `iter_init`.
    pub iter_dealloc: unsafe extern "C" fn(ctx: *const (), iter: *mut ()),
}

/// Validates a contiguous byte run before it is adopted into an owned handle.
///
/// `String` runs check UTF-8, while `Vec<u8>`/`Vec<scalar>` runs accept anything.
/// Returns `true` when the bytes are valid for the target type.
pub type ByteValidator = unsafe extern "C" fn(ptr: *const u8, len: usize) -> bool;

/// Type-erased operations on a borrowed contiguous byte run (`&str`/`&[u8]`),
/// supplied by the front door, mirroring [`SeqThunks`].
///
/// The `&str`/`&[T]` fat-pointer layout is unspecified, so the engine never
/// writes it at a fixed offset — it calls
/// [`set_borrowed`](Self::set_borrowed), where the type is concrete, to build
/// the fat pointer pointing into the input.
#[derive(Clone, Copy, Debug)]
pub struct BorrowThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// Construct the borrowed value at `field`, pointing it at `ptr[..len]`.
    ///
    /// Returns `false` on invalid content, which the engine maps to a decode
    /// error; the field is left uninitialized then.
    pub set_borrowed:
        unsafe extern "C" fn(ctx: *const (), field: *mut u8, ptr: *const u8, len: usize) -> bool,
    /// The borrowed run's element count.
    pub len: unsafe extern "C" fn(ctx: *const (), field: *const u8) -> usize,
    /// A pointer to the borrowed run's contiguous bytes.
    pub data: unsafe extern "C" fn(ctx: *const (), field: *const u8) -> *const u8,
}

/// Type-erased operations on an `Option<T>` handle, supplied by the front door,
/// mirroring [`SeqThunks`]. The engine never pokes the `Option`'s niche/tag
/// directly — it calls these. `ctx` is an opaque per-type pointer the engine
/// passes back untouched.
#[derive(Clone, Copy, Debug)]
pub struct OptionThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// Whether the option at `option` is `Some`.
    pub is_some: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> bool,
    /// A pointer to the contained value (valid only when `is_some`).
    pub get_value: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> *const u8,
    /// Initialize the uninitialized option at `option` to `Some(*value)`, moving
    /// the inner value out of `value`.
    pub init_some: unsafe extern "C" fn(ctx: *const (), option: *mut u8, value: *mut u8),
    /// Initialize the uninitialized option at `option` to `None`.
    pub init_none: unsafe extern "C" fn(ctx: *const (), option: *mut u8),
}

/// Type-erased operations on an owned map handle, supplied by the front door,
/// mirroring [`OptionThunks`]. The engine never pokes the map's in-memory layout
/// directly — it calls these. `ctx` is an opaque per-type pointer the engine
/// passes back untouched.
///
/// Encode is driven by a stateful iterator: `iter_init` builds it, `iter_next`
/// advances it, and `iter_dealloc` frees it. Decode initializes the map with
/// `init_with_capacity`, then `insert`s each decoded pair.
#[derive(Clone, Copy, Debug)]
pub struct MapThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// The map's current entry count.
    pub len: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> usize,
    /// Initialize the uninitialized map at `map` with room for `cap` entries.
    pub init_with_capacity: unsafe extern "C" fn(ctx: *const (), map: *mut u8, cap: usize),
    /// Insert `(*key, *value)` into the initialized map at `map`, moving the key
    /// and value out of their buffers.
    pub insert: unsafe extern "C" fn(ctx: *const (), map: *mut u8, key: *mut u8, value: *mut u8),
    /// Build a stateful iterator over the entries of the initialized map at `map`.
    pub iter_init: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> *mut (),
    /// Advance the iterator, writing the next entry's borrowed key and value
    /// pointers to `key_out`/`value_out` and returning `true`, or returning
    /// `false` at the end.
    pub iter_next: unsafe extern "C" fn(
        ctx: *const (),
        iter: *mut (),
        key_out: *mut *const u8,
        value_out: *mut *const u8,
    ) -> bool,
    /// Free the iterator built by `iter_init`.
    pub iter_dealloc: unsafe extern "C" fn(ctx: *const (), iter: *mut ()),
}

/// Type-erased operations on a `Result<T, E>` handle, supplied by the front door,
/// mirroring [`OptionThunks`] with two value-carrying arms. The engine never
/// pokes the `Result`'s niche/tag directly — it calls these. `ctx` is an opaque
/// per-type pointer the engine passes back untouched.
#[derive(Clone, Copy, Debug)]
pub struct ResultThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// Whether the result at `result` is `Ok`.
    pub is_ok: unsafe extern "C" fn(ctx: *const (), result: *const u8) -> bool,
    /// A pointer to the contained `Ok` value (valid only when `is_ok`).
    pub get_ok: unsafe extern "C" fn(ctx: *const (), result: *const u8) -> *const u8,
    /// A pointer to the contained `Err` value (valid only when not `is_ok`).
    pub get_err: unsafe extern "C" fn(ctx: *const (), result: *const u8) -> *const u8,
    /// Initialize the uninitialized result at `result` to `Ok(*value)`, moving the
    /// inner value out of `value`.
    pub init_ok: unsafe extern "C" fn(ctx: *const (), result: *mut u8, value: *mut u8),
    /// Initialize the uninitialized result at `result` to `Err(*value)`, moving
    /// the inner value out of `value`.
    pub init_err: unsafe extern "C" fn(ctx: *const (), result: *mut u8, value: *mut u8),
}

/// Type-erased operations on an owned pointer handle, supplied by the front door.
/// The engine never assumes the pointer layout or allocation strategy: it borrows
/// the pointee for encode and constructs the owner from a decoded pointee on
/// decode.
#[derive(Clone, Copy, Debug)]
pub struct PointerThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// Borrow the initialized pointer's pointee.
    pub borrow: unsafe extern "C" fn(ctx: *const (), pointer: *const u8) -> *const u8,
    /// Initialize `pointer` from `*value`, moving the pointee out of engine scratch.
    pub init: unsafe extern "C" fn(ctx: *const (), pointer: *mut u8, value: *mut u8),
}

/// Type-erased operations on an opaque field, supplied by the front door,
/// mirroring [`SeqThunks`]. The engine never knows the inner type — it frames the
/// field as a length-prefixed blob and delegates the inner bytes to these thunks.
/// `ctx` is an opaque per-field pointer the engine passes back untouched.
#[derive(Clone, Copy, Debug)]
pub struct OpaqueThunks {
    /// Opaque per-field context, passed to every thunk.
    pub ctx: *const (),
    /// Append the inner value's encoded bytes to `out`.
    pub encode: unsafe extern "C" fn(ctx: *const (), field: *const u8, out: *mut Vec<u8>),
    /// Build the opaque value at `slot` from the inner span `bytes[..len]`
    /// borrowed from the reader's input.
    ///
    /// Returns `false` if the adapter rejects the input, which the engine maps to
    /// a decode error.
    pub decode:
        unsafe extern "C" fn(ctx: *const (), bytes: *const u8, len: usize, slot: *mut u8) -> bool,
}
