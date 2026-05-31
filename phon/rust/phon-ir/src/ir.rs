//! The intermediate representation: a decode plan lowered to a straight,
//! pre-sequenced run of [`Op`]s.
//!
//! Compatibility planning (in `phon-engine`) reconciles a writer schema with a
//! reader schema into a value-shaped *tree*; lowering flattens that tree into a
//! `Program`. Every type-directed decision — which primitive, which field order,
//! which fields to skip or default, how enum variants map — is made once, during
//! lowering, and frozen into the op sequence. What remains in the program is only
//! data-directed control flow that genuinely cannot be precomputed: the element
//! count of a sequence, the active variant of an enum, the presence bit of an
//! option.
//!
//! Two consumers run the same `Program`: the interpreter (a stack machine, in
//! `phon-engine::interp`) and, later, the JIT (copy-and-patch, in `phon-jit`).
//! Defining the IR here is what makes the JIT a second consumer of something that
//! exists from the first commit rather than a retrofit.
//!
//! **Invariant.** Running a complete `Program` against a reader leaves exactly
//! one value on the interpreter's stack — the decoded result. Each variant below
//! documents its own net effect; container ops consume their children's pushes
//! and net `+1`.
//!
//! This first cut is the *decode*, *dynamic-`Value`* path — the mirror of
//! `phon-engine`'s compatibility planner. Encode lowering and the typed
//! (descriptor-driven) path reuse this vocabulary and extend it.
//!
//! Spec: "The intermediate representation" (`r[ir.*]`).

use phon_schema::bytes::{Reader, skip_pad};
use phon_schema::{DecodeError, Primitive, SchemaRef};

/// A lowered decode program: a straight run of [`Op`]s executed start to finish.
/// Container bodies (sequence element, map key/value, option payload, enum arm,
/// fixed-array element) are themselves `Program`s — recursion appears only at
/// genuine data-directed control flow, never within a fixed-shape run. A struct
/// of structs of scalars lowers to a single branch-free `Program`.
pub type Program = Vec<Op>;

/// One lowered decode step. Each reads from the wire and adjusts the
/// interpreter's value stack; the documented net stack effect of a *complete*
/// lowered subtree is always `+1`.
#[derive(Clone, Debug)]
pub enum Op {
    /// Decode a primitive from the wire and push its value. Net `+1`.
    Scalar(Primitive),
    /// Decode a self-describing dynamic value and push it. Net `+1`.
    Dynamic,
    /// Push a null — a reader-only field's default, or a unit variant payload.
    /// Net `+1`.
    Null,
    /// Decode a value by this writer schema reference and discard it: a
    /// writer-only field the reader does not have (`r[compat.skip-writer-only]`).
    /// Net `0`.
    Skip(SchemaRef),
    /// Pop `keys.len()` values (the top of the stack, in order) and assemble an
    /// object pairing each key with its value; push it. The values were pushed by
    /// the immediately preceding ops, in `keys` order. Net `+1`.
    Object { keys: Vec<String> },
    /// Pop `count` values (the top of the stack, in order) into an array; push it.
    /// Used for tuples and tuple variant payloads, whose heterogeneous elements
    /// were lowered inline. Net `+1`.
    Array { count: usize },
    /// Read a `u32` length `n`; run `body` `n` times (each leaves one element on
    /// the stack); collect the `n` elements into an array, rejecting duplicates
    /// when `set`. Push the array. Net `+1`.
    ///
    /// `min_wire` is the element's minimum wire size for the `r[validate.lengths]`
    /// count guard: `0` for a zero-sized element (an empty struct, `unit`, …),
    /// else `1`. A `0` switches the guard to a fixed cap, since the buffer cannot
    /// bound a count of zero-byte elements.
    Seq {
        set: bool,
        min_wire: usize,
        body: Program,
    },
    /// Read a `u32` length `n`; run `key` then `value` `n` times; assemble an
    /// object (string keys), rejecting duplicate keys. Push it. Net `+1`.
    Map { key: Program, value: Program },
    /// Run `body` `product(dimensions)` times (a fixed-shape array); collect into
    /// an array; push it. The product is computed at run time so lowering stays
    /// infallible. `min_wire` bounds the product exactly as in [`Op::Seq`]. Net `+1`.
    FixedArray {
        dimensions: Vec<u64>,
        min_wire: usize,
        body: Program,
    },
    /// Read a presence byte; on `1` run `some` (leaving its value), on `0` push
    /// null. Net `+1`.
    Option { some: Program },
    /// Read a `u32` writer variant index; dispatch to the matching arm, run its
    /// payload, and wrap the result as a single-key object under the reader's
    /// variant name. An index with no arm is a writer-only variant: a decode
    /// error (`r[compat.enum]`). Net `+1`.
    Enum { arms: Vec<EnumArm> },
}

/// One enum arm: the writer's variant index it matches, the reader's name for
/// that variant, and the lowered payload program.
#[derive(Clone, Debug)]
pub struct EnumArm {
    pub writer_index: u32,
    pub reader_name: String,
    pub payload: Program,
}

/// A lowered *typed* program: the memory side of the IR. Where [`Program`] builds
/// a dynamic [`Value`](facet_value::Value) on a stack, a `MemProgram` moves bytes
/// between the wire and a value's in-memory layout, at offsets the descriptor
/// supplies (`r[ir.memory]`).
///
/// In this first cut — fixed scalars and in-place records — a whole nested
/// `repr(Rust)` struct dissolves into a flat run of [`MemOp::Scalar`] copies at
/// folded, base-relative offsets: no branches, the splicing `r[ir.inlining]`
/// describes taken to its limit. Owned sequences, options, and enums (which
/// allocate or branch at run time) extend this later.
pub type MemProgram = Vec<MemOp>;

/// One typed step. The base pointer is supplied at run time; `offset` is relative
/// to it.
#[derive(Clone, Debug)]
pub enum MemOp {
    /// Copy a run of `size` bytes between memory at `offset` and the wire, which
    /// is first padded to `align` (`r[compact.alignment]`). A single scalar, or a
    /// fused run of adjacent scalars (see [`fuse`]). Encode reads memory and
    /// writes the wire; decode reads the wire and writes memory. Sound only where
    /// host byte order equals the wire's (little-endian), which every phon target
    /// is.
    Scalar { offset: usize, size: usize, align: usize },
    /// An owned, contiguous sequence (`Vec<T>`) at `field_offset`: a `u32` count
    /// then `count` elements, each decoded by the element's own program. Decode
    /// initializes the sequence and fills it; encode reads its length and
    /// elements. See [`SeqOp`]. Used when the element has structure; a run of
    /// trivially-copyable elements uses [`Bytes`](MemOp::Bytes) instead.
    Sequence(Box<SeqOp>),
    /// A bulk contiguous run of trivially-copyable elements — `String`, `Vec<u8>`,
    /// or (via bulk-copy lowering) `Vec<u32>`/`Vec<f64>`/… : a `u32` element count
    /// then `count * stride` contiguous bytes moved in **one block**, no per-element
    /// loop. Decode optionally validates the bytes as UTF-8 (for `String`). See
    /// [`BytesOp`].
    Bytes(Box<BytesOp>),
    /// A BORROWED, zero-copy contiguous byte run — `&str` or `&[u8]` — at
    /// `field_offset`. The wire form is IDENTICAL to [`Bytes`](MemOp::Bytes) (a
    /// `u32` element count then `count * stride` contiguous bytes), so a borrowed
    /// peer interoperates byte-for-byte with an owned peer. Where decode of an owned
    /// run allocates and bulk-copies, decode of a borrowed run writes a fat pointer
    /// straight INTO the reader's input buffer — no allocation, no copy. The decoded
    /// `&str`/`&[u8]` borrows the input; the caller must keep the input bytes alive
    /// as long as the decoded value (the standard zero-copy contract). The fat
    /// pointer is never written at a fixed offset (the `&str`/`&[T]` layout is
    /// unspecified) — a [`BorrowThunks`] construct thunk builds it where the type is
    /// concrete. See [`BorrowOp`].
    Borrow(Box<BorrowOp>),
    /// An `Option<T>` at `field_offset`: a `u8` presence byte (`0` none / `1` some),
    /// then — only when some — the inner `T` decoded by its own program. The first
    /// *data-directed* op: the branch is taken on a value read at run time, not
    /// resolved at lowering. See [`OptionOp`].
    Option(Box<OptionOp>),
    /// A `#[repr(uN/iN)]` enum: a `u32` wire variant index then the active variant's
    /// payload. Encode reads the in-memory discriminant (at `tag_offset`, `tag_width`
    /// bytes) to pick the variant; decode reads the wire index, writes the
    /// discriminant, then runs the variant's payload program. Data-directed: the
    /// active arm is chosen at run time. See [`EnumOp`].
    Enum(Box<EnumOp>),
    /// An owned map (`BTreeMap<K, V>`, `HashMap<K, V>`, …) at `field_offset`: a
    /// `u32` entry count then, per entry, the key decoded by its own program then
    /// the value decoded by its own program. Data-directed: the count drives a
    /// run-time loop. Encode reads the entry count and iterates the entries through
    /// a stateful iterator; decode initializes the map with capacity, then for each
    /// entry decodes a key+value into engine scratch and inserts (moving both in).
    /// See [`MapOp`].
    Map(Box<MapOp>),
    /// A writer-only value present on the wire but absent from the reader: consume
    /// its wire bytes and write NOTHING to memory (`r[compat.skip-writer-only]`).
    /// Decode-only — it advances the cursor by a pre-built wire skeleton (see
    /// [`SkipOp`]) without touching the reader value. Net memory effect: none.
    SkipWire(Box<SkipOp>),
    /// A reader-only field absent from the writer: write its default into memory at
    /// `offset` with NO wire read (`r[compat.reader-only-fields]`). Decode-only.
    /// See [`DefaultOp`].
    Default(Box<DefaultOp>),
    /// An opaque field (`#[facet(opaque = ...)]`) at `field_offset`: a `u32` byte
    /// length then that many bytes of an inner encoding the engine never interprets.
    /// On the wire it is IDENTICAL to a `Primitive::Bytes` run (a `u32` count + raw
    /// bytes), so a cross-impl peer reads it as opaque bytes. Encode reserves the
    /// `u32`, calls the [`OpaqueThunks`] encode thunk to append the inner bytes, then
    /// backpatches the length (newly possible because phon is fixed-width, not
    /// varints). Decode reads the length, borrows the span from the input, and hands
    /// it to the decode thunk — zero-copy, the inner schema unknown. See [`OpaqueOp`].
    Opaque(Box<OpaqueOp>),
}

/// A type-erased "write this field's default in place" operation, supplied by the
/// front door for a reader-only field that carries a `#[facet(default)]`. The
/// engine never knows the field type; it calls the thunk, passing back the opaque
/// `ctx` (a `&'static Shape` for a trait default, or a custom default fn) the front
/// door understands. Mirrors the `ctx`-carrying thunk style of [`SeqThunks`] and
/// friends — the spec's bare `fn(slot)` cannot close over the per-field type, so
/// the `ctx` carries it.
pub type DefaultThunk = unsafe extern "C" fn(ctx: *const (), slot: *mut u8);

/// A reader-only-default op's payload (boxed in [`MemOp::Default`]). Initializes the
/// reader field at `base + offset` to its default in place, reading no wire bytes.
#[derive(Clone, Debug)]
pub struct DefaultOp {
    /// Where the reader field lives, relative to the base.
    pub offset: usize,
    /// Opaque per-field context the front door binds (passed to `default`).
    pub ctx: *const (),
    /// Initialize the uninitialized reader field at `slot` to its default.
    pub default: DefaultThunk,
}

/// A pre-built wire skeleton of a writer value, advancing the cursor only — never
/// reading or writing the reader's memory. Built once at lowering from the writer
/// schema (see `skip_op`), run by the decode interpreter to consume a writer-only
/// field's bytes (`r[compat.skip-writer-only]`).
#[derive(Clone, Debug)]
pub enum SkipOp {
    /// A fixed scalar: pad the cursor to `align`, then advance `size` bytes.
    Scalar { size: usize, align: usize },
    /// A bulk byte run (`String`, `Vec<scalar>`): read a `u32` count, pad to
    /// `elem_align`, then advance `count * stride` bytes.
    Bytes { stride: usize, elem_align: usize },
    /// An owned sequence of structured elements (`Vec<struct>`): read a `u32` count,
    /// then skip the element `count` times.
    Seq(Box<SkipOp>),
    /// An `Option<T>`: read a `u8` presence byte; on `1` skip the inner, on `0`
    /// nothing, any other byte is a decode error.
    Option(Box<SkipOp>),
    /// A `#[repr(int)]` enum: read a `u32` writer variant index, then skip that
    /// variant's field-skips. An index matching no entry is a decode error.
    Enum(Vec<(u32, Vec<SkipOp>)>),
    /// An owned map: read a `u32` entry count, then skip key then value `count`
    /// times.
    Map(Box<SkipOp>, Box<SkipOp>),
    /// A struct or tuple: skip each field in wire order.
    Struct(Vec<SkipOp>),
}

/// Advance the reader past one writer value described by `op`, writing nothing to
/// memory. The wire-shape mirror of the decode cursor moves, sharing the
/// `read_len`/`skip_pad`/`read_u8`/`read_u32` and bounds checks the decoders use.
///
/// One implementation, two consumers: the interpreter's `MemOp::SkipWire` arm and
/// the JIT's `phon_stencil_skipwire` wrapper both call this, so writer-only fields
/// are consumed identically regardless of decode engine.
///
/// An enum wire index matching no arm is hostile input; here it becomes
/// [`DecodeError::Malformed`] (the JIT maps its skip-failure status the same way).
///
/// Spec: `r[compat.skip-writer-only]`.
///
/// # Errors
/// [`DecodeError`] for truncated input, a bad `Option` presence byte, or an enum
/// wire index with no matching arm.
// r[impl compat.skip-writer-only]
pub fn skip(r: &mut Reader, op: &SkipOp) -> Result<(), DecodeError> {
    match op {
        SkipOp::Scalar { size, align } => {
            skip_pad(r, *align)?;
            r.read_slice(*size)?;
            Ok(())
        }
        SkipOp::Bytes { stride, elem_align } => {
            let count = r.read_len((*stride).max(1))?;
            skip_pad(r, *elem_align)?;
            r.read_slice(count * stride)?;
            Ok(())
        }
        SkipOp::Seq(element) => {
            let count = r.read_len(1)?;
            for _ in 0..count {
                skip(r, element)?;
            }
            Ok(())
        }
        SkipOp::Option(inner) => match r.read_u8()? {
            0 => Ok(()),
            1 => skip(r, inner),
            b => Err(DecodeError::InvalidBool(b)),
        },
        SkipOp::Enum(arms) => {
            let wire_index = r.read_u32()?;
            let (_, fields) = arms
                .iter()
                .find(|(idx, _)| *idx == wire_index)
                .ok_or(DecodeError::Malformed("enum variant index out of range"))?;
            for f in fields {
                skip(r, f)?;
            }
            Ok(())
        }
        SkipOp::Map(key, value) => {
            let count = r.read_len(1)?;
            for _ in 0..count {
                skip(r, key)?;
                skip(r, value)?;
            }
            Ok(())
        }
        SkipOp::Struct(fields) => {
            for f in fields {
                skip(r, f)?;
            }
            Ok(())
        }
    }
}

/// An owned-sequence op's payload (boxed in [`MemOp::Sequence`] to keep `MemOp`
/// small).
#[derive(Clone, Debug)]
pub struct SeqOp {
    /// Where the sequence handle (e.g. the `Vec`) lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode one element, run at each element slot (offsets
    /// relative to the element).
    pub element: MemProgram,
    /// Bytes between consecutive elements in the sequence's contiguous storage
    /// (the element type's size).
    pub stride: usize,
    /// Alignment of the element type — the engine allocates the element buffer
    /// itself with this layout, then hands it to `from_raw_parts`.
    pub elem_align: usize,
    /// Minimum wire bytes one element occupies (for length-vs-remaining checks,
    /// `r[validate.lengths]`).
    pub min_wire: usize,
    /// Type-erased operations on the sequence handle (front-door bound).
    pub thunks: SeqThunks,
}

/// Type-erased operations on an owned sequence handle, supplied by the front door
/// (`r[descriptors.thunk-binding]`). A `Vec`'s in-memory layout is not something
/// the engine may assume, so it never pokes the handle directly — it calls these.
/// `ctx` is an opaque per-type pointer the front door understands (e.g. the
/// element's list vtable); the engine passes it back untouched.
///
/// Decode is engine-owned: the engine allocates and fills the element buffer
/// itself, then [`from_raw_parts`](Self::from_raw_parts) adopts it into the
/// handle in one move — no per-element vtable traffic, and in the JIT the alloc
/// and fill loop are code the engine emits.
#[derive(Clone, Copy, Debug)]
pub struct SeqThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// Construct the sequence at `list` from a buffer of `len` elements the engine
    /// allocated with `cap` capacity: `*list = Vec::from_raw_parts(ptr, len, cap)`.
    /// The buffer must have been allocated with the element type's array layout
    /// (the engine guarantees this).
    pub from_raw_parts:
        unsafe extern "C" fn(ctx: *const (), list: *mut u8, ptr: *mut u8, len: usize, cap: usize),
    /// The sequence's current element count.
    pub len: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> usize,
    /// A pointer to the sequence's contiguous element storage (for reading).
    pub data: unsafe extern "C" fn(ctx: *const (), list: *const u8) -> *const u8,
}

/// Validates a contiguous byte run before it is adopted into an owned handle:
/// `String` runs check UTF-8 (`r[validate.text]`), `Vec<u8>`/`Vec<scalar>` runs
/// accept anything. Returns `true` when the bytes are valid for the target type.
///
/// One function pointer, called the same way by both engines: the interpreter
/// invokes it directly, and the JIT reaches it as an *indirect* call (so it needs
/// no relocation — the reason in-stencil UTF-8 validation routes through here
/// rather than calling `core::str::from_utf8` inline).
pub type ByteValidator = unsafe extern "C" fn(ptr: *const u8, len: usize) -> bool;

/// A bulk byte-run op's payload (boxed in [`MemOp::Bytes`]). The wire form is a
/// `u32` element count then `count * stride` contiguous bytes — one block copy in
/// each direction, no per-element loop. `String` and `Vec<u8>` use `stride == 1`;
/// `Vec<scalar>` uses the element size.
#[derive(Clone, Debug)]
pub struct BytesOp {
    /// Where the owned handle (the `String`/`Vec`) lives, relative to the base.
    pub field_offset: usize,
    /// Bytes per element: 1 for `String`/`Vec<u8>`, the element size otherwise.
    pub stride: usize,
    /// Alignment of the contiguous element buffer.
    pub elem_align: usize,
    /// Validate the contiguous bytes on decode before adopting them. `String` runs
    /// check UTF-8; `Vec` runs accept anything. See [`ByteValidator`].
    pub validate: ByteValidator,
    /// Type-erased handle operations (`from_raw_parts` adopts the buffer; `len`
    /// returns the element count; `data` points at the contiguous bytes).
    pub thunks: SeqThunks,
}

/// A borrowed, zero-copy byte-run op's payload (boxed in [`MemOp::Borrow`]). The
/// wire form is a `u32` element count then `count * stride` contiguous bytes —
/// IDENTICAL to [`BytesOp`] — but decode writes a fat pointer into the input
/// buffer rather than allocating and copying. `&str` and `&[u8]` use `stride == 1`
/// and `elem_align == 1`.
#[derive(Clone, Debug)]
pub struct BorrowOp {
    /// Where the borrowed handle (the `&str`/`&[u8]` fat pointer) lives, relative
    /// to the base.
    pub field_offset: usize,
    /// Bytes per element: 1 for `&str`/`&[u8]`.
    pub stride: usize,
    /// Alignment of the borrowed run on the wire (1 for `&str`/`&[u8]`).
    pub elem_align: usize,
    /// Type-erased construct/read operations on the borrowed handle (front-door
    /// bound).
    pub thunks: BorrowThunks,
}

/// Type-erased operations on a BORROWED contiguous byte run (`&str`/`&[u8]`),
/// supplied by the front door (`r[descriptors.thunk-binding]`), mirroring
/// [`SeqThunks`]. The `&str`/`&[T]` fat-pointer layout is unspecified, so the
/// engine never writes it at a fixed offset — it calls [`set_borrowed`](Self::set_borrowed),
/// where the type is concrete, to build the fat pointer pointing into the input.
/// `ctx` is an opaque per-type pointer the engine passes back untouched (it can be
/// null for the concrete `&str`/`&[u8]` thunks, which need no per-type context).
#[derive(Clone, Copy, Debug)]
pub struct BorrowThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// Construct the borrowed value at `field`, pointing it at `ptr[..len]` (a run
    /// INTO the reader's input). For `&str`:
    /// `core::ptr::write(field.cast::<&str>(), core::str::from_utf8(slice::from_raw_parts(ptr, len))?)`;
    /// for `&[u8]`: `core::ptr::write(field.cast::<&[u8]>(), slice::from_raw_parts(ptr, len))`.
    /// Returns `false` on invalid content (e.g. non-UTF-8 for `&str`), which the
    /// engine maps to a decode error; the field is left uninitialized then.
    pub set_borrowed:
        unsafe extern "C" fn(ctx: *const (), field: *mut u8, ptr: *const u8, len: usize) -> bool,
    /// The borrowed run's element count (its byte length for `&str`/`&[u8]`).
    pub len: unsafe extern "C" fn(ctx: *const (), field: *const u8) -> usize,
    /// A pointer to the borrowed run's contiguous bytes (for reading on encode).
    pub data: unsafe extern "C" fn(ctx: *const (), field: *const u8) -> *const u8,
}

/// An optional op's payload (boxed in [`MemOp::Option`]). The wire form is a `u8`
/// presence byte then, only when present, the inner value. The engine never
/// assumes the in-memory `Option<T>` layout (a repr(Rust) niche or tag); it reads
/// and builds presence through the [`OptionThunks`] vtable.
#[derive(Clone, Debug)]
pub struct OptionOp {
    /// Where the `Option<T>` handle lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode the inner `T`, run at the inner value (offsets relative
    /// to the inner start).
    pub some: MemProgram,
    /// The inner `T`'s size and alignment — the engine allocates a scratch buffer
    /// of this layout on decode, fills it with the inner program, then moves it into
    /// the `Option` via `init_some`.
    pub inner_size: usize,
    /// Alignment of the inner `T` (for the decode scratch buffer).
    pub inner_align: usize,
    /// Type-erased presence operations on the `Option` handle (front-door bound).
    pub thunks: OptionThunks,
}

/// Type-erased operations on an `Option<T>` handle, supplied by the front door
/// (`r[descriptors.thunk-binding]`), mirroring [`SeqThunks`]. The engine never
/// pokes the `Option`'s niche/tag directly — it calls these. `ctx` is an opaque
/// per-type pointer (the inner type's option vtable) the engine passes back
/// untouched.
#[derive(Clone, Copy, Debug)]
pub struct OptionThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// Whether the option at `option` is `Some`.
    pub is_some: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> bool,
    /// A pointer to the contained value (valid only when `is_some`).
    pub get_value: unsafe extern "C" fn(ctx: *const (), option: *const u8) -> *const u8,
    /// Initialize the uninitialized option at `option` to `Some(*value)`, moving the
    /// inner value out of `value` (the engine then frees `value`'s storage without
    /// dropping it).
    pub init_some: unsafe extern "C" fn(ctx: *const (), option: *mut u8, value: *mut u8),
    /// Initialize the uninitialized option at `option` to `None`.
    pub init_none: unsafe extern "C" fn(ctx: *const (), option: *mut u8),
}

/// A `#[repr(int)]` enum op's payload (boxed in [`MemOp::Enum`]). The wire form is
/// a `u32` variant index then the active variant's fields. In memory the
/// discriminant lives at `tag_offset` (base-relative), `tag_width` bytes wide; the
/// variant's fields live at their own base-relative offsets (already past the
/// discriminant, per facet). Only `#[repr(uN/iN)]` enums lower here — a default
/// `repr(Rust)` enum has an unspecified discriminant layout.
#[derive(Clone, Debug)]
pub struct EnumOp {
    /// Where the in-memory discriminant lives, relative to the base.
    pub tag_offset: usize,
    /// The discriminant's width in bytes (1/2/4/8), from the `#[repr(int)]` type.
    pub tag_width: usize,
    /// The variants, each with its wire index, in-memory discriminant, and payload
    /// program. Looked up by wire index on decode, by discriminant on encode.
    pub variants: Vec<EnumVariantOp>,
    /// Writer variant indices that exist in the *writer* schema but have no reader
    /// counterpart (the decode-compat path only — empty for a single-schema lower).
    /// Receiving one of these on the wire is a writer-only-variant decode error
    /// (`r[compat.enum]`), distinct from a wholly out-of-range index.
    pub writer_only: Vec<u32>,
}

/// One enum variant in a [`MemOp::Enum`].
#[derive(Clone, Debug)]
pub struct EnumVariantOp {
    /// The `u32` written to / read from the wire to identify this variant.
    pub wire_index: u32,
    /// The in-memory discriminant value (its low `tag_width` bytes) identifying
    /// this variant — `i64`-derived, stored as `u64` for width-masked comparison.
    pub selector: u64,
    /// The variant's payload fields, with base-relative offsets, in wire order.
    pub payload: MemProgram,
}

/// An owned-map op's payload (boxed in [`MemOp::Map`]). The wire form is a `u32`
/// entry count then, per entry, the key value then the value value (each by its
/// own sub-program). The engine never assumes the map's in-memory layout: it
/// reads length, iterates entries, initializes with capacity, and inserts through
/// the [`MapThunks`] vtable. Mirrors [`OptionOp`] with a key+value sub-program, a
/// stateful encode iterator, and init+insert on decode.
#[derive(Clone, Debug)]
pub struct MapOp {
    /// Where the map handle lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode one key (offsets relative to the key value).
    pub key: MemProgram,
    /// How to encode/decode one value (offsets relative to the value value).
    pub value: MemProgram,
    /// The key type's size and alignment — the engine allocates a scratch buffer of
    /// this layout on decode, fills it with the key program, then moves it into the
    /// map via `insert`.
    pub key_size: usize,
    /// Alignment of the key type (for the decode scratch buffer).
    pub key_align: usize,
    /// The value type's size and alignment (decode scratch buffer).
    pub value_size: usize,
    /// Alignment of the value type (for the decode scratch buffer).
    pub value_align: usize,
    /// Type-erased operations on the map handle (front-door bound).
    pub thunks: MapThunks,
}

/// Type-erased operations on an owned map handle, supplied by the front door
/// (`r[descriptors.thunk-binding]`), mirroring [`OptionThunks`]. The engine never
/// pokes the map's in-memory layout directly — it calls these. `ctx` is an opaque
/// per-type pointer (the map's def) the engine passes back untouched.
///
/// Encode is driven by a stateful iterator: `iter_init` builds it, `iter_next`
/// advances it (yielding borrowed key/value pointers), and `iter_dealloc` frees
/// it. Decode initializes the map with `init_with_capacity`, then `insert`s each
/// decoded pair (moving the key and value out of engine scratch).
#[derive(Clone, Copy, Debug)]
pub struct MapThunks {
    /// Opaque per-type context, passed to every thunk.
    pub ctx: *const (),
    /// The map's current entry count.
    pub len: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> usize,
    /// Initialize the uninitialized map at `map` with room for `cap` entries.
    pub init_with_capacity: unsafe extern "C" fn(ctx: *const (), map: *mut u8, cap: usize),
    /// Insert `(*key, *value)` into the initialized map at `map`, moving the key and
    /// value out of their buffers (the engine then frees both without dropping).
    pub insert: unsafe extern "C" fn(ctx: *const (), map: *mut u8, key: *mut u8, value: *mut u8),
    /// Build a stateful iterator over the entries of the initialized map at `map`.
    pub iter_init: unsafe extern "C" fn(ctx: *const (), map: *const u8) -> *mut (),
    /// Advance the iterator, writing the next entry's borrowed key and value
    /// pointers to `key_out`/`value_out` and returning `true`, or returning `false`
    /// at the end.
    pub iter_next: unsafe extern "C" fn(
        ctx: *const (),
        iter: *mut (),
        key_out: *mut *const u8,
        value_out: *mut *const u8,
    ) -> bool,
    /// Free the iterator built by `iter_init`.
    pub iter_dealloc: unsafe extern "C" fn(ctx: *const (), iter: *mut ()),
}

/// An opaque-field op's payload (boxed in [`MemOp::Opaque`]). The wire form is a
/// `u32` byte length then that many inner bytes — IDENTICAL to a `Primitive::Bytes`
/// run, so a peer that does not know the inner type reads it as opaque bytes. The
/// engine frames it (reserve the `u32`, backpatch after sub-encoding); the
/// [`OpaqueThunks`] fill (encode) or consume (decode) the inner span.
#[derive(Clone, Debug)]
pub struct OpaqueOp {
    /// Where the opaque field (e.g. a `Payload` enum) lives, relative to the base.
    pub field_offset: usize,
    /// Type-erased encode/decode of the inner value (front-door bound).
    pub thunks: OpaqueThunks,
}

/// Type-erased operations on an opaque field (`#[facet(opaque = ...)]`), supplied
/// by the front door (`r[descriptors.thunk-binding]`), mirroring [`SeqThunks`]. The
/// engine never knows the inner type — it frames the field as a length-prefixed
/// blob and delegates the inner bytes to these thunks. `ctx` is an opaque per-field
/// pointer (the field's opaque-adapter definition) the engine passes back untouched.
///
/// Encode reserves a `u32` length, calls [`encode`](Self::encode) to APPEND the
/// inner value's bytes to `out`, then backpatches the length. Decode reads the
/// length, borrows the inner span from the reader's input, and calls
/// [`decode`](Self::decode) to build the value at `slot` — the decoded value may
/// borrow that span (the standard zero-copy contract).
#[derive(Clone, Copy, Debug)]
pub struct OpaqueThunks {
    /// Opaque per-field context, passed to every thunk.
    pub ctx: *const (),
    /// Append the inner value's encoded bytes to `out`. `field` points at the
    /// opaque field in memory; the engine has already reserved (and will backpatch)
    /// the `u32` length prefix, so this writes ONLY the inner bytes.
    pub encode: unsafe extern "C" fn(ctx: *const (), field: *const u8, out: *mut Vec<u8>),
    /// Build the opaque value at `slot` from the inner span `bytes[..len]` (borrowed
    /// from the reader's input). Returns `false` if the adapter rejects the input,
    /// which the engine maps to a decode error (the field is left uninitialized then).
    pub decode:
        unsafe extern "C" fn(ctx: *const (), bytes: *const u8, len: usize, slot: *mut u8) -> bool,
}

/// Coalesce adjacent scalar copies that are contiguous in *both* the wire and
/// memory into one larger copy — the specialization the IR exists for. A flat
/// struct whose wire layout matches its memory layout collapses to a single
/// `memcpy`; a `repr(Rust)` struct collapses to a copy per contiguous run.
///
/// Two consecutive ops fuse when the second needs no wire padding after the first
/// (wire-contiguous) and its memory offset continues the first's (mem-contiguous).
/// The fused op keeps the run's starting alignment; the bytes it produces are
/// identical, so this is transparent to correctness — only faster.
///
/// Spec: `r[ir.inlining]` (the lowering-time coalescing it describes).
// r[impl ir.inlining]
#[must_use]
pub fn fuse(program: MemProgram) -> MemProgram {
    let mut out: MemProgram = Vec::with_capacity(program.len());
    // `None` once a variable-length op (a sequence) makes the static wire
    // position unknown; scalars after that can't be proven contiguous, so they
    // aren't fused (their padding is still handled at run time).
    let mut wire_pos: Option<usize> = Some(0);
    for op in program {
        match op {
            MemOp::Scalar { offset, size, align } => {
                let pad = wire_pos.map(|p| align.wrapping_sub(p & (align - 1)) & (align - 1));
                let fuses = pad == Some(0)
                    && matches!(
                        out.last(),
                        Some(MemOp::Scalar { offset: po, size: ps, .. }) if po + ps == offset
                    );
                if fuses {
                    if let Some(MemOp::Scalar { size: ps, .. }) = out.last_mut() {
                        *ps += size;
                    }
                } else {
                    out.push(MemOp::Scalar { offset, size, align });
                }
                wire_pos = wire_pos.map(|p| p + pad.unwrap_or(0) + size);
            }
            // Variable-length / data-directed ops make the static wire position
            // unknown after them. `SkipWire` consumes opaque writer bytes, so it
            // too poisons the static position.
            seq @ (MemOp::Sequence(_)
            | MemOp::Bytes(_)
            | MemOp::Borrow(_)
            | MemOp::Option(_)
            | MemOp::Enum(_)
            | MemOp::Map(_)
            | MemOp::Opaque(_)
            | MemOp::SkipWire(_)) => {
                out.push(seq);
                wire_pos = None;
            }
            // A reader-only default reads no wire bytes, so it leaves the static
            // wire position untouched; it is not a scalar, so it breaks a fuse run
            // (a scalar after it cannot fuse with one before it). Just push it.
            def @ MemOp::Default(_) => out.push(def),
        }
    }
    out
}
