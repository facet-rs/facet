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

/// A caller-local descriptor tree: schema identity, process-local memory layout,
/// and the access strategy for reading or constructing the value.
#[derive(Clone, Debug)]
pub struct Descriptor<SchemaRef> {
    /// The caller's schema reference for this value.
    pub schema: SchemaRef,
    /// Process-local size and alignment.
    pub layout: Layout,
    /// How to read and construct this value.
    pub access: Access<SchemaRef>,
}

/// Process-local size and alignment, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub size: usize,
    pub align: usize,
}

/// How a value's bytes are read and constructed.
#[derive(Clone, Debug)]
pub enum Access<SchemaRef> {
    /// A fixed-width scalar whose in-memory bytes equal its wire bytes.
    Scalar,
    /// A struct or tuple: fields at fixed offsets.
    Record(RecordAccess<SchemaRef>),
    /// A sum type: an active variant chosen by a tag, with a payload per variant.
    Enum(EnumAccess<SchemaRef>),
    /// none / some.
    Option(OptionAccess<SchemaRef>),
    /// A fixed-shape array: `count` elements inline, `stride` apart.
    Array {
        element: Box<Descriptor<SchemaRef>>,
        count: usize,
        stride: usize,
    },
    /// A runtime-shape tensor.
    Tensor(TensorAccess<SchemaRef>),
    /// A dynamic homogeneous sequence or byte sequence.
    Sequence(SequenceAccess<SchemaRef>),
    /// A set stored behind caller-provided thunks.
    Set(SetAccess<SchemaRef>),
    /// Key / value pairs.
    Map(MapAccess<SchemaRef>),
    /// A result-like two-armed sum whose local layout is thunk-driven.
    Result(ResultAccess<SchemaRef>),
    /// An owning pointer whose wire shape is its pointee.
    Pointer(PointerAccess<SchemaRef>),
    /// A dynamic self-describing value owned by the caller.
    Dynamic,
    /// An opaque value whose inner encoding is delegated to caller thunks.
    Opaque(OpaqueThunks),
    /// A back-edge to a recursive schema block.
    Recurse,
}

/// A struct or tuple: its fields at offsets, with how to construct it.
#[derive(Clone, Debug)]
pub struct RecordAccess<SchemaRef> {
    pub fields: Vec<FieldAccess<SchemaRef>>,
    pub construct: Construct,
}

/// One field: its byte offset within the record, and its descriptor.
#[derive(Clone, Debug)]
pub struct FieldAccess<SchemaRef> {
    pub offset: usize,
    pub descriptor: Descriptor<SchemaRef>,
    /// How to write this field's default in place when a reader-only field is
    /// absent from the wire.
    pub default: Option<FieldDefault>,
}

/// A field's bound default-in-place operation.
#[derive(Clone, Copy, Debug)]
pub struct FieldDefault {
    /// Opaque per-field context the front door binds.
    pub ctx: *const (),
    /// Initialize the uninitialized field at `slot` to its default.
    pub thunk: DefaultThunk,
}

/// How a record is built on decode.
#[derive(Clone, Debug)]
pub enum Construct {
    /// Decode writes each field into its offset in uninitialized storage.
    InPlace,
    /// Decode fills a scratch buffer, then a thunk builds the real value from it.
    Thunk(Thunk),
}

/// A sum type: a tag selecting the active variant, and the per-variant payloads.
#[derive(Clone, Debug)]
pub struct EnumAccess<SchemaRef> {
    pub tag: Tag,
    pub variants: Vec<VariantAccess<SchemaRef>>,
}

/// How the active variant is read and set.
#[derive(Clone, Debug)]
pub enum Tag {
    /// An integer discriminant `width` bytes wide at `offset`.
    Direct { offset: usize, width: usize },
    /// A niche where the discriminating region overlaps payload bytes.
    Niche { offset: usize, width: usize },
    /// Caller-defined tag operations.
    Thunk { read: Thunk, write: Thunk },
}

/// One variant: its schema index, local tag selector, and payload fields.
#[derive(Clone, Debug)]
pub struct VariantAccess<SchemaRef> {
    pub index: u32,
    pub selector: u64,
    pub payload: RecordAccess<SchemaRef>,
}

/// An optional value: how presence is read/written, and the some-payload.
#[derive(Clone, Debug)]
pub struct OptionAccess<SchemaRef> {
    pub presence: Presence,
    pub some: Box<Descriptor<SchemaRef>>,
}

/// How none-vs-some is encoded in memory.
#[derive(Clone, Debug)]
pub enum Presence {
    /// A dedicated tag region.
    Tag {
        offset: usize,
        width: usize,
        none_value: u64,
    },
    /// The some-payload's own bytes encode none at a pattern.
    Niche {
        offset: usize,
        width: usize,
        none_pattern: Vec<u8>,
    },
    /// Caller-defined presence operations.
    Thunk {
        is_some: Thunk,
        set_none: Thunk,
        set_some: Thunk,
    },
    /// Front-door-bound presence via an option vtable.
    Vtable(OptionThunks),
}

/// A dynamic homogeneous sequence or byte sequence: its element and storage.
#[derive(Clone, Debug)]
pub struct SequenceAccess<SchemaRef> {
    pub element: Box<Descriptor<SchemaRef>>,
    pub storage: SequenceStorage,
}

/// A set: its element descriptor and storage strategy.
#[derive(Clone, Debug)]
pub struct SetAccess<SchemaRef> {
    pub element: Box<Descriptor<SchemaRef>>,
    pub storage: SetStorage,
}

/// How a set's elements are read and constructed in memory.
#[derive(Clone, Debug)]
pub enum SetStorage {
    /// Front-door-bound set operations.
    Vtable(SetThunks),
}

/// How a sequence's elements are stored in memory.
#[derive(Clone, Debug)]
pub enum SequenceStorage {
    /// Owned contiguous run with explicit local handle offsets.
    Owned {
        ptr_offset: usize,
        len_offset: usize,
        cap_offset: Option<usize>,
        allocate: Thunk,
    },
    /// Borrowed contiguous run with explicit local handle offsets.
    Borrowed {
        ptr_offset: usize,
        len_offset: usize,
    },
    /// Non-flat storage through caller-provided operations.
    Thunk { len: Thunk, get: Thunk, push: Thunk },
    /// An owned contiguous sequence reached through front-door-bound thunks.
    Vtable(SeqThunks),
    /// A borrowed, zero-copy contiguous byte run reached through bound thunks.
    BorrowedVtable(BorrowThunks),
}

/// A result-like value: ok/err payload descriptors and local operations.
#[derive(Clone, Debug)]
pub struct ResultAccess<SchemaRef> {
    pub ok: Box<Descriptor<SchemaRef>>,
    pub err: Box<Descriptor<SchemaRef>>,
    pub thunks: ResultThunks,
}

/// An owning pointer: its pointee descriptor and local operations.
#[derive(Clone, Debug)]
pub struct PointerAccess<SchemaRef> {
    pub pointee: Box<Descriptor<SchemaRef>>,
    pub thunks: PointerThunks,
}

/// Key/value pairs: the key and value descriptors and how the map is stored.
#[derive(Clone, Debug)]
pub struct MapAccess<SchemaRef> {
    pub key: Box<Descriptor<SchemaRef>>,
    pub value: Box<Descriptor<SchemaRef>>,
    pub storage: MapStorage,
}

/// How a map's entries are read and constructed in memory.
#[derive(Clone, Debug)]
pub enum MapStorage {
    /// Named same-language thunks.
    Thunk {
        len: Thunk,
        iterate: Thunk,
        insert: Thunk,
    },
    /// Front-door-bound map operations.
    Vtable(MapThunks),
}

/// A runtime-shape tensor.
#[derive(Clone, Debug)]
pub struct TensorAccess<SchemaRef> {
    pub element: Box<Descriptor<SchemaRef>>,
    /// Encode: read the dimension sizes.
    pub shape: Thunk,
    /// The flat row-major elements.
    pub data: SequenceStorage,
    /// Decode: give the filled flat data its shape.
    pub reshape: Thunk,
}

/// A named function the implementation provides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thunk {
    /// Resolved to a function pointer by the binding.
    pub name: String,
}

/// A typed memory program whose block calls use caller-defined block ids.
// r[impl ir.one-vocabulary]
pub type MemProgram<BlockId> = crate::Program<MemOp<BlockId>>;

/// One typed-memory step. The base pointer is supplied at run time; `offset`
/// fields are relative to it.
#[derive(Clone, Debug)]
pub enum MemOp<BlockId> {
    /// Copy a run of `size` bytes between memory at `offset` and the wire, which
    /// is first padded to `align`. A single scalar, or a fused run of adjacent
    /// scalars.
    Scalar {
        offset: usize,
        size: usize,
        align: usize,
    },
    /// A native-sized integer (`usize`/`isize`) whose wire primitive is fixed-width
    /// (`u64`/`i64`) on every platform.
    NativeInt {
        offset: usize,
        mem_size: usize,
        signed: bool,
    },
    /// An owned, contiguous sequence.
    Sequence(Box<SeqOp<BlockId>>),
    /// An owned set.
    Set(Box<SetOp<BlockId>>),
    /// A bulk contiguous run of trivially-copyable elements.
    Bytes(Box<BytesOp>),
    /// A borrowed, zero-copy contiguous byte run.
    Borrow(Box<BorrowOp>),
    /// An `Option<T>` handle.
    Option(Box<OptionOp<BlockId>>),
    /// A `#[repr(uN/iN)]` enum.
    Enum(Box<EnumOp<BlockId>>),
    /// An owned map.
    Map(Box<MapOp<BlockId>>),
    /// A self-describing dynamic value at `field_offset`.
    Dynamic { field_offset: usize },
    /// A `Result<T, E>` handle.
    Result(Box<ResultOp<BlockId>>),
    /// An owned pointer.
    Pointer(Box<PointerOp<BlockId>>),
    /// A writer-only value present on the wire but absent from the reader.
    SkipWire(Box<SkipOp>),
    /// A reader-only field absent from the writer.
    Default(Box<DefaultOp>),
    /// An opaque field whose inner encoding is delegated to caller-supplied thunks.
    Opaque(Box<OpaqueOp>),
    /// A call into a recursive block program, run at `base + offset`.
    CallBlock { schema: BlockId, offset: usize },
}

/// A pre-built wire skeleton of a writer value, advancing the cursor only.
#[derive(Clone, Debug)]
pub enum SkipOp {
    /// A fixed scalar: pad the cursor to `align`, then advance `size` bytes.
    Scalar { size: usize, align: usize },
    /// A bulk byte run: read a `u32` count, pad to `elem_align`, then advance
    /// `count * stride` bytes.
    Bytes { stride: usize, elem_align: usize },
    /// An owned sequence of structured elements.
    Seq(Box<SkipOp>),
    /// An `Option<T>`: read a presence byte, then skip the inner when present.
    Option(Box<SkipOp>),
    /// A `#[repr(int)]` enum: read a writer variant index, then skip that
    /// variant's field skips.
    Enum(Vec<(u32, Vec<SkipOp>)>),
    /// An owned map: read an entry count, then skip key then value for each entry.
    Map(Box<SkipOp>, Box<SkipOp>),
    /// A struct or tuple: skip each field in wire order.
    Struct(Vec<SkipOp>),
    /// A self-describing dynamic value.
    Dynamic,
}

/// An owned-sequence op's payload.
#[derive(Clone, Debug)]
pub struct SeqOp<BlockId> {
    /// Where the sequence handle lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode one element, run at each element slot.
    pub element: MemProgram<BlockId>,
    /// Bytes between consecutive elements in contiguous storage.
    pub stride: usize,
    /// Alignment of the element type.
    pub elem_align: usize,
    /// Minimum wire bytes one element occupies.
    pub min_wire: usize,
    /// Type-erased operations on the sequence handle.
    pub thunks: SeqThunks,
}

/// An owned-set op's payload.
#[derive(Clone, Debug)]
pub struct SetOp<BlockId> {
    /// Where the set handle lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode one element.
    pub element: MemProgram<BlockId>,
    /// Element size for decode scratch allocation.
    pub elem_size: usize,
    /// Element alignment for decode scratch allocation.
    pub elem_align: usize,
    /// Minimum wire bytes one element occupies.
    pub min_wire: usize,
    /// Type-erased operations on the set handle.
    pub thunks: SetThunks,
}

/// A bulk byte-run op's payload.
#[derive(Clone, Debug)]
pub struct BytesOp {
    /// Where the owned handle lives, relative to the base.
    pub field_offset: usize,
    /// Bytes per element.
    pub stride: usize,
    /// Alignment of the contiguous element buffer.
    pub elem_align: usize,
    /// Validate the contiguous bytes on decode before adopting them.
    pub validate: ByteValidator,
    /// Type-erased handle operations.
    pub thunks: SeqThunks,
}

/// A borrowed, zero-copy byte-run op's payload.
#[derive(Clone, Debug)]
pub struct BorrowOp {
    /// Where the borrowed handle lives, relative to the base.
    pub field_offset: usize,
    /// Bytes per element.
    pub stride: usize,
    /// Alignment of the borrowed run on the wire.
    pub elem_align: usize,
    /// Type-erased construct/read operations on the borrowed handle.
    pub thunks: BorrowThunks,
}

/// An optional op's payload.
#[derive(Clone, Debug)]
pub struct OptionOp<BlockId> {
    /// Where the `Option<T>` handle lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode the inner `T`.
    pub some: MemProgram<BlockId>,
    /// The inner `T`'s size.
    pub inner_size: usize,
    /// The inner `T`'s alignment.
    pub inner_align: usize,
    /// Type-erased presence operations on the `Option` handle.
    pub thunks: OptionThunks,
}

/// A `#[repr(int)]` enum op's payload.
#[derive(Clone, Debug)]
pub struct EnumOp<BlockId> {
    /// Where the in-memory discriminant lives, relative to the base.
    pub tag_offset: usize,
    /// The discriminant's width in bytes.
    pub tag_width: usize,
    /// The variants, each with its wire index, in-memory discriminant, and payload
    /// program.
    pub variants: Vec<EnumVariantOp<BlockId>>,
    /// Writer variant indices with no reader counterpart.
    pub writer_only: Vec<u32>,
}

/// One enum variant in a [`MemOp::Enum`].
#[derive(Clone, Debug)]
pub struct EnumVariantOp<BlockId> {
    /// The `u32` written to / read from the wire to identify this variant.
    pub wire_index: u32,
    /// The in-memory discriminant value identifying this variant.
    pub selector: u64,
    /// The variant's payload fields, with base-relative offsets, in wire order.
    pub payload: MemProgram<BlockId>,
}

/// An owned-map op's payload.
#[derive(Clone, Debug)]
pub struct MapOp<BlockId> {
    /// Where the map handle lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode one key.
    pub key: MemProgram<BlockId>,
    /// How to encode/decode one value.
    pub value: MemProgram<BlockId>,
    /// The key type's size.
    pub key_size: usize,
    /// The key type's alignment.
    pub key_align: usize,
    /// The value type's size.
    pub value_size: usize,
    /// The value type's alignment.
    pub value_align: usize,
    /// Type-erased operations on the map handle.
    pub thunks: MapThunks,
}

/// A `Result<T, E>` op's payload.
#[derive(Clone, Debug)]
pub struct ResultOp<BlockId> {
    /// Where the `Result<T, E>` handle lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode the `Ok` payload.
    pub ok: MemProgram<BlockId>,
    /// The `Ok` payload's size.
    pub ok_size: usize,
    /// The `Ok` payload's alignment.
    pub ok_align: usize,
    /// The wire index identifying the `Ok` arm.
    pub ok_wire_index: u32,
    /// How to encode/decode the `Err` payload.
    pub err: MemProgram<BlockId>,
    /// The `Err` payload's size.
    pub err_size: usize,
    /// The `Err` payload's alignment.
    pub err_align: usize,
    /// The wire index identifying the `Err` arm.
    pub err_wire_index: u32,
    /// Type-erased presence/construction operations on the `Result`.
    pub thunks: ResultThunks,
}

/// An owned-pointer op's payload.
#[derive(Clone, Debug)]
pub struct PointerOp<BlockId> {
    /// Where the pointer handle lives, relative to the base.
    pub field_offset: usize,
    /// How to encode/decode the pointee `T`.
    pub pointee: MemProgram<BlockId>,
    /// The pointee's size for decode scratch allocation.
    pub pointee_size: usize,
    /// The pointee's alignment for decode scratch allocation.
    pub pointee_align: usize,
    /// Type-erased borrow/construct operations on the owning pointer.
    pub thunks: PointerThunks,
}

/// An opaque-field op's payload.
#[derive(Clone, Debug)]
pub struct OpaqueOp {
    /// Where the opaque field lives, relative to the base.
    pub field_offset: usize,
    /// Type-erased encode/decode of the inner value.
    pub thunks: OpaqueThunks,
}

/// Errors from shape-only lowering helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoweringError {
    /// A fixed-array bulk copy would overflow `usize`.
    ArrayBulkCopySizeOverflow,
    /// A fixed-array element's base-relative offset would overflow `usize`.
    ArrayElementOffsetOverflow,
}

/// The minimum wire bytes one owned-container element occupies.
///
/// A program made entirely of zero-sized scalar copies occupies no wire bytes,
/// so length guards must use a fixed cap instead of deriving a cap from the
/// reader's remaining byte count. Every other element occupies at least one byte.
#[must_use]
pub fn element_min_wire<BlockId>(element: &[MemOp<BlockId>]) -> usize {
    let zero_sized = element
        .iter()
        .all(|op| matches!(op, MemOp::Scalar { size: 0, .. }));
    usize::from(!zero_sized)
}

/// Return the scalar alignment when an element program can be represented as one
/// contiguous byte run inside a sequence or fixed array.
#[must_use]
pub fn bulk_scalar_align<BlockId>(element: &[MemOp<BlockId>], stride: usize) -> Option<usize> {
    match element {
        [
            MemOp::Scalar {
                offset: 0,
                size,
                align,
            },
        ] if *size == stride && *align != 0 && stride.is_multiple_of(*align) => Some(*align),
        _ => None,
    }
}

/// Build an owned-sequence op, using a bulk byte run when the lowered element is
/// one scalar covering the full stride.
#[must_use]
pub fn owned_sequence_op<BlockId>(
    field_offset: usize,
    element: MemProgram<BlockId>,
    stride: usize,
    elem_align: usize,
    validate: ByteValidator,
    thunks: SeqThunks,
) -> MemOp<BlockId> {
    let element = fuse(element);
    if bulk_scalar_align(&element, stride).is_some() {
        MemOp::Bytes(Box::new(BytesOp {
            field_offset,
            stride,
            elem_align,
            validate,
            thunks,
        }))
    } else {
        let min_wire = element_min_wire(&element);
        MemOp::Sequence(Box::new(SeqOp {
            field_offset,
            element,
            stride,
            elem_align,
            min_wire,
            thunks,
        }))
    }
}

/// Build a set op from a pre-lowered element program.
#[must_use]
pub fn set_op<BlockId>(
    field_offset: usize,
    element: MemProgram<BlockId>,
    elem_size: usize,
    elem_align: usize,
    thunks: SetThunks,
) -> MemOp<BlockId> {
    let element = fuse(element);
    let min_wire = element_min_wire(&element);
    MemOp::Set(Box::new(SetOp {
        field_offset,
        element,
        elem_size,
        elem_align,
        min_wire,
        thunks,
    }))
}

/// Lower each record field at `base + field.offset`.
pub fn lower_record_fields<SchemaRef, BlockId, Error>(
    fields: &[FieldAccess<SchemaRef>],
    base: usize,
    out: &mut MemProgram<BlockId>,
    mut lower_field: impl FnMut(
        &Descriptor<SchemaRef>,
        usize,
        &mut MemProgram<BlockId>,
    ) -> Result<(), Error>,
) -> Result<(), Error> {
    for field in fields {
        lower_field(&field.descriptor, base + field.offset, out)?;
    }
    Ok(())
}

/// Lower a fixed-size inline array, collapsing it to one scalar copy when a
/// single element is itself a full-stride scalar byte run.
pub fn lower_fixed_array<BlockId, Error>(
    count: usize,
    stride: usize,
    base: usize,
    out: &mut MemProgram<BlockId>,
    mut lower_element: impl FnMut(usize, &mut MemProgram<BlockId>) -> Result<(), Error>,
) -> Result<(), Error>
where
    Error: From<LoweringError>,
{
    let mut element_ops = Vec::new();
    lower_element(0, &mut element_ops)?;
    let element_ops = fuse(element_ops);

    if let Some(align) = bulk_scalar_align(&element_ops, stride) {
        out.push(MemOp::Scalar {
            offset: base,
            size: fixed_array_copy_size(count, stride).map_err(Error::from)?,
            align,
        });
        return Ok(());
    }

    for index in 0..count {
        let offset = array_element_offset(base, index, stride).map_err(Error::from)?;
        lower_element(offset, out)?;
    }

    Ok(())
}

/// Total byte count for a collapsed fixed-array copy.
pub fn fixed_array_copy_size(count: usize, stride: usize) -> Result<usize, LoweringError> {
    count
        .checked_mul(stride)
        .ok_or(LoweringError::ArrayBulkCopySizeOverflow)
}

/// Base-relative offset for one fixed-array element.
pub fn array_element_offset(
    base: usize,
    index: usize,
    stride: usize,
) -> Result<usize, LoweringError> {
    let rel = index
        .checked_mul(stride)
        .ok_or(LoweringError::ArrayElementOffsetOverflow)?;
    base.checked_add(rel)
        .ok_or(LoweringError::ArrayElementOffsetOverflow)
}

/// Coalesce adjacent scalar copies that are contiguous in both wire and memory.
// r[impl ir.inlining]
#[must_use]
pub fn fuse<BlockId>(program: MemProgram<BlockId>) -> MemProgram<BlockId> {
    let mut out: MemProgram<BlockId> = Vec::with_capacity(program.len());
    let mut wire_pos: Option<usize> = Some(0);

    for op in program {
        match op {
            MemOp::Scalar {
                offset,
                size,
                align,
            } => {
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
                    out.push(MemOp::Scalar {
                        offset,
                        size,
                        align,
                    });
                }
                wire_pos = wire_pos.map(|p| p + pad.unwrap_or(0) + size);
            }
            MemOp::NativeInt {
                offset,
                mem_size,
                signed,
            } => {
                let align = 8usize;
                let size = 8usize;
                let pad = wire_pos.map(|p| align.wrapping_sub(p & (align - 1)) & (align - 1));
                out.push(MemOp::NativeInt {
                    offset,
                    mem_size,
                    signed,
                });
                wire_pos = wire_pos.map(|p| p + pad.unwrap_or(0) + size);
            }
            seq @ (MemOp::Sequence(_)
            | MemOp::Set(_)
            | MemOp::Bytes(_)
            | MemOp::Borrow(_)
            | MemOp::Option(_)
            | MemOp::Enum(_)
            | MemOp::Map(_)
            | MemOp::Result(_)
            | MemOp::Pointer(_)
            | MemOp::Dynamic { .. }
            | MemOp::Opaque(_)
            | MemOp::CallBlock { .. }
            | MemOp::SkipWire(_)) => {
                out.push(seq);
                wire_pos = None;
            }
            def @ MemOp::Default(_) => out.push(def),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_min_wire_distinguishes_zero_sized_programs() {
        assert_eq!(element_min_wire::<()>(&[]), 0);
        assert_eq!(
            element_min_wire(&[MemOp::<()>::Scalar {
                offset: 0,
                size: 0,
                align: 1,
            }]),
            0
        );
        assert_eq!(
            element_min_wire(&[MemOp::<()>::Scalar {
                offset: 0,
                size: 4,
                align: 4,
            }]),
            1
        );
    }

    #[test]
    fn bulk_scalar_align_requires_one_full_stride_scalar() {
        let scalar = [MemOp::<()>::Scalar {
            offset: 0,
            size: 8,
            align: 4,
        }];
        assert_eq!(bulk_scalar_align(&scalar, 8), Some(4));

        let partial = [MemOp::<()>::Scalar {
            offset: 0,
            size: 4,
            align: 4,
        }];
        assert_eq!(bulk_scalar_align(&partial, 8), None);

        let shifted = [MemOp::<()>::Scalar {
            offset: 4,
            size: 4,
            align: 4,
        }];
        assert_eq!(bulk_scalar_align(&shifted, 4), None);
    }

    #[test]
    fn fixed_array_lowering_collapses_full_stride_scalar_elements() {
        let mut out = Vec::new();
        lower_fixed_array::<(), LoweringError>(3, 4, 16, &mut out, |base, out| {
            out.push(MemOp::Scalar {
                offset: base,
                size: 4,
                align: 4,
            });
            Ok(())
        })
        .unwrap();

        match out.as_slice() {
            [
                MemOp::Scalar {
                    offset,
                    size,
                    align,
                },
            ] => {
                assert_eq!((*offset, *size, *align), (16, 12, 4));
            }
            other => panic!("expected one collapsed scalar op, got {other:?}"),
        }
    }

    #[test]
    fn fixed_array_lowering_replays_structured_elements_at_checked_offsets() {
        let mut out = Vec::new();
        lower_fixed_array::<(), LoweringError>(2, 8, 16, &mut out, |base, out| {
            out.push(MemOp::Scalar {
                offset: base + 4,
                size: 4,
                align: 4,
            });
            Ok(())
        })
        .unwrap();

        let offsets: Vec<_> = out
            .iter()
            .map(|op| match op {
                MemOp::Scalar { offset, .. } => *offset,
                other => panic!("unexpected op {other:?}"),
            })
            .collect();
        assert_eq!(offsets, [20, 28]);
    }

    #[test]
    fn fixed_array_offset_helpers_report_overflow() {
        assert_eq!(
            fixed_array_copy_size(usize::MAX, 2),
            Err(LoweringError::ArrayBulkCopySizeOverflow)
        );
        assert_eq!(
            array_element_offset(usize::MAX - 1, 1, 2),
            Err(LoweringError::ArrayElementOffsetOverflow)
        );
    }
}
