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
    /// Explicit byte ownership for bytes this record descriptor can prove.
    ///
    /// Optimizers may only treat a gap as padding when it appears here as
    /// [`ByteOwner::Padding`]. Missing bytes, unknown ranges, and layout facts not
    /// represented here are barriers.
    pub byte_ownership: RecordByteOwnership,
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

/// Proven byte ownership for a record-shaped descriptor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordByteOwnership {
    pub ranges: Vec<ByteRange>,
}

impl RecordByteOwnership {
    /// No usable byte-ownership proof.
    #[must_use]
    pub fn unknown(layout_size: usize) -> Self {
        if layout_size == 0 {
            Self::default()
        } else {
            Self {
                ranges: vec![ByteRange {
                    offset: 0,
                    len: layout_size,
                    owner: ByteOwner::Unknown,
                }],
            }
        }
    }

    /// Mark only field byte ranges. Gaps remain unrepresented and therefore
    /// unknown to consumers.
    #[must_use]
    pub fn fields_only<SchemaRef>(fields: &[FieldAccess<SchemaRef>]) -> Self {
        let Some(fields) = sorted_field_ranges(fields) else {
            return Self::default();
        };
        Self {
            ranges: fields
                .into_iter()
                .map(|field| ByteRange {
                    offset: field.offset,
                    len: field.len,
                    owner: ByteOwner::Field(field.index),
                })
                .collect(),
        }
    }

    /// Derive field and padding ranges for a plain record whose full layout is
    /// known. Any overlap, out-of-bounds field, or arithmetic overflow falls back
    /// to one unknown range for the whole layout.
    #[must_use]
    pub fn from_record_layout<SchemaRef>(
        layout: Layout,
        fields: &[FieldAccess<SchemaRef>],
    ) -> Self {
        let Some(fields) = sorted_field_ranges(fields) else {
            return Self::unknown(layout.size);
        };
        let Some(last_end) = fields
            .last()
            .map_or(Some(0), |field| field.offset.checked_add(field.len))
        else {
            return Self::unknown(layout.size);
        };
        if last_end > layout.size {
            return Self::unknown(layout.size);
        }

        let mut ranges = Vec::with_capacity(fields.len().saturating_mul(2).saturating_add(1));
        let mut cursor = 0usize;
        for field in fields {
            if cursor < field.offset {
                ranges.push(ByteRange {
                    offset: cursor,
                    len: field.offset - cursor,
                    owner: ByteOwner::Padding,
                });
            }
            if field.len != 0 {
                ranges.push(ByteRange {
                    offset: field.offset,
                    len: field.len,
                    owner: ByteOwner::Field(field.index),
                });
            }
            cursor = field.offset + field.len;
        }
        if cursor < layout.size {
            ranges.push(ByteRange {
                offset: cursor,
                len: layout.size - cursor,
                owner: ByteOwner::Padding,
            });
        }
        Self { ranges }
    }

    /// Whether every byte in `offset..offset + len` is explicitly known padding.
    ///
    /// Missing ranges, unknown ranges, field ranges, and overflow are all barriers.
    #[must_use]
    pub fn is_padding_range(&self, offset: usize, len: usize) -> bool {
        if len == 0 {
            return true;
        }
        let Some(end) = offset.checked_add(len) else {
            return false;
        };
        let mut cursor = offset;

        for range in &self.ranges {
            let Some(range_end) = range.offset.checked_add(range.len) else {
                return false;
            };
            if range_end <= cursor {
                continue;
            }
            if range.offset > cursor {
                return false;
            }
            if range.owner != ByteOwner::Padding {
                return false;
            }
            cursor = range_end.min(end);
            if cursor == end {
                return true;
            }
        }

        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteRange {
    pub offset: usize,
    pub len: usize,
    pub owner: ByteOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteOwner {
    Field(usize),
    Padding,
    Unknown,
}

#[derive(Clone, Copy)]
struct FieldByteRange {
    index: usize,
    offset: usize,
    len: usize,
}

fn sorted_field_ranges<SchemaRef>(
    fields: &[FieldAccess<SchemaRef>],
) -> Option<Vec<FieldByteRange>> {
    let mut ranges = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        let len = field.descriptor.layout.size;
        let end = field.offset.checked_add(len)?;
        if len != 0 {
            ranges.push(FieldByteRange {
                index,
                offset: field.offset,
                len,
            });
        } else if end < field.offset {
            return None;
        }
    }
    ranges.sort_by_key(|range| (range.offset, range.index));

    let mut prev_end = 0usize;
    for range in &ranges {
        if range.offset < prev_end {
            return None;
        }
        prev_end = range.offset.checked_add(range.len)?;
    }
    Some(ranges)
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
    /// Copy several scalar segments in one grouped op.
    ///
    /// Each segment keeps its own wire alignment. Memory bytes between segments
    /// are not read or written; this is only equivalent to the scalar stream when
    /// those gaps are known padding or when the segments are contiguous.
    ScalarRun(Box<ScalarRunOp>),
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

/// One scalar segment inside a grouped scalar run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalarSegment {
    pub offset: usize,
    pub size: usize,
    pub align: usize,
}

impl ScalarSegment {
    fn end(self) -> Option<usize> {
        self.offset.checked_add(self.size)
    }
}

/// A scalar run preserving per-segment wire alignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarRunOp {
    pub segments: Vec<ScalarSegment>,
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
    let zero_sized = element.iter().all(|op| match op {
        MemOp::Scalar { size: 0, .. } => true,
        MemOp::ScalarRun(run) => run.segments.iter().all(|segment| segment.size == 0),
        _ => false,
    });
    usize::from(!zero_sized)
}

/// Shape-only counts for a typed memory program.
///
/// Nested inline programs are counted once, as shape, not multiplied by runtime
/// sequence/map/set lengths. [`MemOp::CallBlock`] is counted as a call op; block
/// bodies are counted by [`lowered_mem_program_stats`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemProgramStats {
    pub op_count: usize,
    pub scalar_op_count: usize,
    pub scalar_run_count: usize,
    pub scalar_run_segment_count: usize,
    pub native_int_count: usize,
    pub sequence_count: usize,
    pub set_count: usize,
    pub bytes_count: usize,
    pub borrow_count: usize,
    pub option_count: usize,
    pub enum_count: usize,
    pub enum_variant_count: usize,
    pub map_count: usize,
    pub dynamic_count: usize,
    pub result_count: usize,
    pub pointer_count: usize,
    pub skip_wire_count: usize,
    pub default_count: usize,
    pub opaque_count: usize,
    pub call_block_count: usize,
}

impl MemProgramStats {
    /// Add another shape counter into this one.
    pub fn accumulate(&mut self, other: Self) {
        self.op_count += other.op_count;
        self.scalar_op_count += other.scalar_op_count;
        self.scalar_run_count += other.scalar_run_count;
        self.scalar_run_segment_count += other.scalar_run_segment_count;
        self.native_int_count += other.native_int_count;
        self.sequence_count += other.sequence_count;
        self.set_count += other.set_count;
        self.bytes_count += other.bytes_count;
        self.borrow_count += other.borrow_count;
        self.option_count += other.option_count;
        self.enum_count += other.enum_count;
        self.enum_variant_count += other.enum_variant_count;
        self.map_count += other.map_count;
        self.dynamic_count += other.dynamic_count;
        self.result_count += other.result_count;
        self.pointer_count += other.pointer_count;
        self.skip_wire_count += other.skip_wire_count;
        self.default_count += other.default_count;
        self.opaque_count += other.opaque_count;
        self.call_block_count += other.call_block_count;
    }
}

/// Shape-only counts for a lowered typed memory program with recursive blocks.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoweredMemProgramStats {
    pub root: MemProgramStats,
    pub blocks: MemProgramStats,
    pub total: MemProgramStats,
    pub block_count: usize,
}

impl LoweredMemProgramStats {
    /// Add another lowered-program shape counter into this one.
    pub fn accumulate(&mut self, other: Self) {
        self.root.accumulate(other.root);
        self.blocks.accumulate(other.blocks);
        self.total.accumulate(other.total);
        self.block_count += other.block_count;
    }
}

/// Count the typed memory IR shape for one program.
#[must_use]
pub fn mem_program_stats<BlockId>(program: &[MemOp<BlockId>]) -> MemProgramStats {
    let mut stats = MemProgramStats::default();
    add_mem_program_stats(program, &mut stats);
    stats
}

/// Count the typed memory IR shape for a lowered program and its block table.
#[must_use]
pub fn lowered_mem_program_stats<BlockId>(
    lowered: &crate::Lowered<BlockId, MemOp<BlockId>>,
) -> LoweredMemProgramStats {
    let root = mem_program_stats(&lowered.program);
    let mut blocks = MemProgramStats::default();
    for block in lowered.blocks.values() {
        blocks.accumulate(mem_program_stats(block));
    }
    let mut total = root;
    total.accumulate(blocks);

    LoweredMemProgramStats {
        root,
        blocks,
        total,
        block_count: lowered.blocks.len(),
    }
}

fn add_mem_program_stats<BlockId>(program: &[MemOp<BlockId>], stats: &mut MemProgramStats) {
    for op in program {
        stats.op_count += 1;
        match op {
            MemOp::Scalar { .. } => stats.scalar_op_count += 1,
            MemOp::ScalarRun(run) => {
                stats.scalar_run_count += 1;
                stats.scalar_run_segment_count += run.segments.len();
            }
            MemOp::NativeInt { .. } => stats.native_int_count += 1,
            MemOp::Sequence(seq) => {
                stats.sequence_count += 1;
                add_mem_program_stats(&seq.element, stats);
            }
            MemOp::Set(set) => {
                stats.set_count += 1;
                add_mem_program_stats(&set.element, stats);
            }
            MemOp::Bytes(_) => stats.bytes_count += 1,
            MemOp::Borrow(_) => stats.borrow_count += 1,
            MemOp::Option(option) => {
                stats.option_count += 1;
                add_mem_program_stats(&option.some, stats);
            }
            MemOp::Enum(en) => {
                stats.enum_count += 1;
                stats.enum_variant_count += en.variants.len();
                for variant in &en.variants {
                    add_mem_program_stats(&variant.payload, stats);
                }
            }
            MemOp::Map(map) => {
                stats.map_count += 1;
                add_mem_program_stats(&map.key, stats);
                add_mem_program_stats(&map.value, stats);
            }
            MemOp::Dynamic { .. } => stats.dynamic_count += 1,
            MemOp::Result(result) => {
                stats.result_count += 1;
                add_mem_program_stats(&result.ok, stats);
                add_mem_program_stats(&result.err, stats);
            }
            MemOp::Pointer(pointer) => {
                stats.pointer_count += 1;
                add_mem_program_stats(&pointer.pointee, stats);
            }
            MemOp::SkipWire(_) => stats.skip_wire_count += 1,
            MemOp::Default(_) => stats.default_count += 1,
            MemOp::Opaque(_) => stats.opaque_count += 1,
            MemOp::CallBlock { .. } => stats.call_block_count += 1,
        }
    }
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

/// Group adjacent scalar ops inside one record when memory gaps are proven padding.
///
/// The optimizer is intentionally record-local: byte ownership is record-relative,
/// and after flattening there is no way to distinguish padding from arbitrary
/// untouched memory. A `ScalarRun` keeps each segment's wire alignment, so this
/// does not change compact-wire padding behavior.
#[must_use]
pub fn group_record_scalars<BlockId>(
    program: MemProgram<BlockId>,
    ownership: &RecordByteOwnership,
    record_base: usize,
) -> MemProgram<BlockId> {
    let mut out = Vec::with_capacity(program.len());
    let mut run = Vec::new();

    for op in program {
        if let Some(segments) = scalar_segments(&op) {
            if run_can_append(&run, &segments, ownership, record_base) {
                run.extend(segments);
            } else {
                flush_scalar_run(&mut out, &mut run);
                run.extend(segments);
            }
        } else {
            flush_scalar_run(&mut out, &mut run);
            out.push(op);
        }
    }
    flush_scalar_run(&mut out, &mut run);
    out
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

fn scalar_segments<BlockId>(op: &MemOp<BlockId>) -> Option<Vec<ScalarSegment>> {
    match op {
        MemOp::Scalar {
            offset,
            size,
            align,
        } => Some(vec![ScalarSegment {
            offset: *offset,
            size: *size,
            align: *align,
        }]),
        MemOp::ScalarRun(run) => Some(run.segments.clone()),
        _ => None,
    }
}

fn run_can_append(
    run: &[ScalarSegment],
    next: &[ScalarSegment],
    ownership: &RecordByteOwnership,
    record_base: usize,
) -> bool {
    let (Some(last), Some(first)) = (run.last(), next.first()) else {
        return true;
    };
    let Some(last_end) = last.end() else {
        return false;
    };
    if first.offset < last_end {
        return false;
    }
    if first.offset == last_end {
        return true;
    }
    absolute_gap_is_padding(ownership, record_base, last_end, first.offset)
}

fn absolute_gap_is_padding(
    ownership: &RecordByteOwnership,
    record_base: usize,
    start: usize,
    end: usize,
) -> bool {
    let Some(rel_start) = start.checked_sub(record_base) else {
        return false;
    };
    let Some(rel_end) = end.checked_sub(record_base) else {
        return false;
    };
    if rel_end < rel_start {
        return false;
    }
    ownership.is_padding_range(rel_start, rel_end - rel_start)
}

fn flush_scalar_run<BlockId>(out: &mut MemProgram<BlockId>, run: &mut Vec<ScalarSegment>) {
    match run.len() {
        0 => {}
        1 => {
            let segment = run[0];
            out.push(MemOp::Scalar {
                offset: segment.offset,
                size: segment.size,
                align: segment.align,
            });
        }
        _ => {
            out.push(MemOp::ScalarRun(Box::new(ScalarRunOp {
                segments: core::mem::take(run),
            })));
            return;
        }
    }
    run.clear();
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
            run @ MemOp::ScalarRun(_) => {
                out.push(run);
                wire_pos = None;
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

    fn field(offset: usize, size: usize) -> FieldAccess<()> {
        FieldAccess {
            offset,
            descriptor: Descriptor {
                schema: (),
                layout: Layout { size, align: 1 },
                access: Access::Scalar,
            },
            default: None,
        }
    }

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
    fn mem_program_stats_count_inline_shapes_and_lowered_blocks() {
        let program = vec![
            MemOp::<u8>::Scalar {
                offset: 0,
                size: 4,
                align: 4,
            },
            MemOp::ScalarRun(Box::new(ScalarRunOp {
                segments: vec![
                    ScalarSegment {
                        offset: 8,
                        size: 2,
                        align: 2,
                    },
                    ScalarSegment {
                        offset: 12,
                        size: 4,
                        align: 4,
                    },
                ],
            })),
            MemOp::Enum(Box::new(EnumOp {
                tag_offset: 16,
                tag_width: 4,
                variants: vec![
                    EnumVariantOp {
                        wire_index: 0,
                        selector: 0,
                        payload: vec![MemOp::Dynamic { field_offset: 24 }],
                    },
                    EnumVariantOp {
                        wire_index: 1,
                        selector: 1,
                        payload: vec![MemOp::NativeInt {
                            offset: 32,
                            mem_size: 8,
                            signed: false,
                        }],
                    },
                ],
                writer_only: vec![9],
            })),
            MemOp::CallBlock {
                schema: 7,
                offset: 40,
            },
        ];

        let stats = mem_program_stats(&program);
        assert_eq!(stats.op_count, 6);
        assert_eq!(stats.scalar_op_count, 1);
        assert_eq!(stats.scalar_run_count, 1);
        assert_eq!(stats.scalar_run_segment_count, 2);
        assert_eq!(stats.enum_count, 1);
        assert_eq!(stats.enum_variant_count, 2);
        assert_eq!(stats.dynamic_count, 1);
        assert_eq!(stats.native_int_count, 1);
        assert_eq!(stats.call_block_count, 1);

        let mut lowered = crate::Lowered::new(program);
        lowered.blocks.insert(
            7,
            vec![
                MemOp::Scalar {
                    offset: 0,
                    size: 1,
                    align: 1,
                },
                MemOp::SkipWire(Box::new(SkipOp::Scalar { size: 4, align: 4 })),
            ],
        );

        let lowered_stats = lowered_mem_program_stats(&lowered);
        assert_eq!(lowered_stats.block_count, 1);
        assert_eq!(lowered_stats.root.op_count, 6);
        assert_eq!(lowered_stats.blocks.op_count, 2);
        assert_eq!(lowered_stats.blocks.skip_wire_count, 1);
        assert_eq!(lowered_stats.total.op_count, 8);
        assert_eq!(lowered_stats.total.scalar_op_count, 2);
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

    #[test]
    fn record_byte_ownership_marks_internal_and_tail_padding() {
        let fields = [field(0, 4), field(8, 2)];
        let ownership =
            RecordByteOwnership::from_record_layout(Layout { size: 12, align: 4 }, &fields);

        assert_eq!(
            ownership.ranges,
            [
                ByteRange {
                    offset: 0,
                    len: 4,
                    owner: ByteOwner::Field(0),
                },
                ByteRange {
                    offset: 4,
                    len: 4,
                    owner: ByteOwner::Padding,
                },
                ByteRange {
                    offset: 8,
                    len: 2,
                    owner: ByteOwner::Field(1),
                },
                ByteRange {
                    offset: 10,
                    len: 2,
                    owner: ByteOwner::Padding,
                },
            ]
        );
    }

    #[test]
    fn record_field_ranges_do_not_turn_gaps_into_padding() {
        let fields = [field(0, 4), field(8, 2)];
        let ownership = RecordByteOwnership::fields_only(&fields);

        assert_eq!(
            ownership.ranges,
            [
                ByteRange {
                    offset: 0,
                    len: 4,
                    owner: ByteOwner::Field(0),
                },
                ByteRange {
                    offset: 8,
                    len: 2,
                    owner: ByteOwner::Field(1),
                },
            ]
        );
    }

    #[test]
    fn record_byte_ownership_falls_back_to_unknown_for_bad_ranges() {
        let overlapping = [field(0, 8), field(4, 4)];
        let out_of_bounds = [field(8, 8)];

        assert_eq!(
            RecordByteOwnership::from_record_layout(Layout { size: 12, align: 4 }, &overlapping),
            RecordByteOwnership::unknown(12)
        );
        assert_eq!(
            RecordByteOwnership::from_record_layout(Layout { size: 12, align: 4 }, &out_of_bounds),
            RecordByteOwnership::unknown(12)
        );
    }

    #[test]
    fn record_byte_ownership_answers_padding_ranges() {
        let fields = [field(0, 4), field(8, 2)];
        let ownership =
            RecordByteOwnership::from_record_layout(Layout { size: 12, align: 4 }, &fields);

        assert!(ownership.is_padding_range(4, 4));
        assert!(ownership.is_padding_range(10, 2));
        assert!(!ownership.is_padding_range(2, 4));
        assert!(!ownership.is_padding_range(12, 1));
    }

    #[test]
    fn record_scalar_grouping_crosses_explicit_padding() {
        let fields = [field(0, 4), field(8, 2)];
        let ownership =
            RecordByteOwnership::from_record_layout(Layout { size: 12, align: 4 }, &fields);
        let program = vec![
            MemOp::<()>::Scalar {
                offset: 16,
                size: 4,
                align: 4,
            },
            MemOp::Scalar {
                offset: 24,
                size: 2,
                align: 2,
            },
        ];

        let grouped = group_record_scalars(program, &ownership, 16);

        match grouped.as_slice() {
            [MemOp::ScalarRun(run)] => assert_eq!(
                run.segments,
                [
                    ScalarSegment {
                        offset: 16,
                        size: 4,
                        align: 4,
                    },
                    ScalarSegment {
                        offset: 24,
                        size: 2,
                        align: 2,
                    },
                ]
            ),
            other => panic!("expected one scalar run, got {other:?}"),
        }
    }

    #[test]
    fn record_scalar_grouping_crosses_contiguous_wire_padding() {
        let fields = [field(0, 4), field(4, 8)];
        let ownership =
            RecordByteOwnership::from_record_layout(Layout { size: 12, align: 8 }, &fields);
        let program = vec![
            MemOp::<()>::Scalar {
                offset: 0,
                size: 4,
                align: 4,
            },
            MemOp::Scalar {
                offset: 4,
                size: 8,
                align: 8,
            },
        ];

        let grouped = group_record_scalars(program, &ownership, 0);

        match grouped.as_slice() {
            [MemOp::ScalarRun(run)] => assert_eq!(run.segments.len(), 2),
            other => panic!("expected one scalar run, got {other:?}"),
        }
    }

    #[test]
    fn record_scalar_grouping_does_not_cross_unknown_gap() {
        let fields = [field(0, 4), field(8, 2)];
        let ownership = RecordByteOwnership::fields_only(&fields);
        let program = vec![
            MemOp::<()>::Scalar {
                offset: 0,
                size: 4,
                align: 4,
            },
            MemOp::Scalar {
                offset: 8,
                size: 2,
                align: 2,
            },
        ];

        let grouped = group_record_scalars(program, &ownership, 0);

        assert!(matches!(
            grouped.as_slice(),
            [
                MemOp::Scalar { offset: 0, .. },
                MemOp::Scalar { offset: 8, .. }
            ]
        ));
    }
}
