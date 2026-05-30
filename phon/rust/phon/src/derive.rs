//! The facet bridge: turn a `#[derive(Facet)]` type's `Shape` into a phon schema
//! batch and a [`Descriptor`].
//!
//! This is the **only** place field offsets come from — facet's
//! [`Field::offset`](facet::Field), read off the reflected `Shape`. The engine
//! never computes layout and `offset_of!` never appears: facet is exactly the
//! tool that hands us offsets, sizes, and alignments for any reflected type.
//!
//! Two products fall out of one walk of the `Shape`:
//! - a **schema batch** with real content-derived ids (via
//!   [`resolve_ids`](phon_schema::resolve_ids)), for a registry — the *wire* view;
//! - a **descriptor** carrying those same ids plus the memory offsets — the
//!   *memory* view.
//!
//! First cut: structs of fixed-width scalars (and nested structs), in-place
//! construction. Sequences, options, enums, and maps extend this as the typed
//! engine path grows.
//!
//! Spec: "Rust" (language section), `r[descriptors.fact-driven]`.

use std::collections::HashMap;
use std::fmt;

use facet::{
    Def, Facet, ListDef, OptionDef, PtrConst, PtrMut, PtrUninit, ScalarType, Shape, Type, UserType,
};
use phon_ir::{
    Access, Construct, Descriptor, FieldAccess, Layout, OptionAccess, OptionThunks, Presence,
    RecordAccess, SeqThunks, SequenceAccess, SequenceStorage,
};
use phon_schema::{
    Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, primitive_id, resolve_ids,
};

/// phon's view of a Rust type, derived from its facet `Shape`: the resolved
/// schema batch (for a [`Registry`](phon_engine::Registry)), the root schema id,
/// and the descriptor.
#[derive(Clone, Debug)]
pub struct Derived {
    /// The root type's content-derived schema id.
    pub root: SchemaId,
    /// Every composite schema reachable from the root, with real ids; feed this
    /// to a registry. Primitives are intrinsic and not listed.
    pub schemas: Vec<Schema>,
    /// The root type's descriptor (memory layout + how to build it).
    pub descriptor: Descriptor,
}

/// Why a `Shape` could not (yet) be lowered to phon.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeriveError {
    /// A kind the bridge does not handle yet (only structs of fixed scalars).
    Unsupported(&'static str),
    /// An unsized type has no layout to describe.
    Unsized,
}

impl fmt::Display for DeriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeriveError::Unsupported(what) => write!(f, "cannot derive phon from this type: {what}"),
            DeriveError::Unsized => write!(f, "cannot derive phon from an unsized type"),
        }
    }
}

impl std::error::Error for DeriveError {}

/// Derive phon's view of the `#[derive(Facet)]` type `T`.
///
/// # Errors
/// [`DeriveError`] if `T` uses a shape the bridge does not handle yet.
pub fn of<'a, T: Facet<'a>>() -> Result<Derived, DeriveError> {
    of_shape(T::SHAPE)
}

/// Derive phon's view from a reflected `Shape` directly.
///
/// # Errors
/// As [`of`].
pub fn of_shape(shape: &'static Shape) -> Result<Derived, DeriveError> {
    // A `String` root: a byte sequence with schema `Primitive::String`.
    if is_string(shape) {
        return Ok(Derived {
            root: primitive_id(Primitive::String),
            schemas: Vec::new(),
            descriptor: string_descriptor(shape)?,
        });
    }
    // A bare scalar root: no composite batch, the id is the primitive's.
    if let Some(p) = scalar_primitive(shape)? {
        let (size, align) = layout_of(shape)?;
        return Ok(Derived {
            root: primitive_id(p),
            schemas: Vec::new(),
            descriptor: Descriptor {
                schema: SchemaRef::concrete(primitive_id(p)),
                layout: Layout { size, align },
                access: Access::Scalar,
            },
        });
    }
    if !is_struct(shape) {
        return Err(DeriveError::Unsupported(
            "derive root must be a struct or fixed scalar so far",
        ));
    }

    // Pass 1: intern composites with provisional dense-index keys, building proto
    // schemas whose references use those keys (primitives use their real id).
    let mut b = Builder::default();
    let root_key = b.intern(shape)?;
    let by_shape = b.by_shape;

    // Resolve provisional keys to real content-derived ids. `resolved[k]` is the
    // schema interned at provisional key `k`, so its id is that key's real id.
    let resolved = resolve_ids(b.protos);
    let real_ids: Vec<SchemaId> = resolved.iter().map(|s| s.id).collect();

    // Pass 2: build the descriptor with the real ids and facet's offsets.
    let descriptor = build_descriptor(shape, &by_shape, &real_ids)?;

    Ok(Derived {
        root: real_ids[root_key],
        schemas: resolved,
        descriptor,
    })
}

/// Pass 1 state: composites interned to provisional keys (= indices into
/// `protos`), deduplicated by their `Shape` pointer.
#[derive(Default)]
struct Builder {
    protos: Vec<Schema>,
    by_shape: HashMap<usize, usize>,
}

impl Builder {
    /// Intern a struct shape, returning its provisional key. The slot is reserved
    /// before recursing so a self- or mutual reference resolves to a key.
    fn intern(&mut self, shape: &'static Shape) -> Result<usize, DeriveError> {
        let ptr = shape_ptr(shape);
        if let Some(&k) = self.by_shape.get(&ptr) {
            return Ok(k);
        }
        let key = self.protos.len();
        self.by_shape.insert(ptr, key);
        self.protos.push(Schema {
            id: SchemaId(key as u64),
            type_params: Vec::new(),
            kind: SchemaKind::Dynamic, // placeholder, replaced once fields resolve
        });
        let kind = self.struct_kind(shape)?;
        self.protos[key].kind = kind;
        Ok(key)
    }

    fn struct_kind(&mut self, shape: &'static Shape) -> Result<SchemaKind, DeriveError> {
        let fields = struct_fields(shape)?;
        let mut out = Vec::with_capacity(fields.len());
        for f in fields {
            out.push(Field {
                name: f.name.to_string(),
                schema: self.ref_of(f.shape())?,
                required: true,
            });
        }
        Ok(SchemaKind::Struct {
            name: shape.type_identifier.to_string(),
            fields: out,
        })
    }

    /// The schema reference for a field's type: a primitive's real id, a nested
    /// struct's provisional key, or a `List`'s provisional key.
    fn ref_of(&mut self, shape: &'static Shape) -> Result<SchemaRef, DeriveError> {
        if is_string(shape) {
            return Ok(SchemaRef::concrete(primitive_id(Primitive::String)));
        }
        if let Some(p) = scalar_primitive(shape)? {
            Ok(SchemaRef::concrete(primitive_id(p)))
        } else if is_struct(shape) {
            let key = self.intern(shape)?;
            Ok(SchemaRef::concrete(SchemaId(key as u64)))
        } else if let Some(list_def) = list_def(shape) {
            let key = self.intern_list(list_def)?;
            Ok(SchemaRef::concrete(SchemaId(key as u64)))
        } else if let Some(opt) = option_def(shape) {
            let key = self.intern_option(opt)?;
            Ok(SchemaRef::concrete(SchemaId(key as u64)))
        } else {
            Err(DeriveError::Unsupported(
                "derive: only structs, lists, options, and fixed scalars so far",
            ))
        }
    }

    /// Intern a `List` schema (e.g. `Vec<T>`), returning its provisional key. The
    /// element reference is resolved first (recursing into composites as needed),
    /// then a `List` schema is appended. Lists are interned by their `ListDef`
    /// pointer so two `Vec<T>` of the same `T` dedup.
    fn intern_list(&mut self, list_def: &'static ListDef) -> Result<usize, DeriveError> {
        let ptr = core::ptr::from_ref(list_def) as usize;
        if let Some(&k) = self.by_shape.get(&ptr) {
            return Ok(k);
        }
        let element = self.ref_of(list_def.t())?;
        let key = self.protos.len();
        self.by_shape.insert(ptr, key);
        self.protos.push(Schema {
            id: SchemaId(key as u64),
            type_params: Vec::new(),
            kind: SchemaKind::List { element },
        });
        Ok(key)
    }

    /// Intern an `Option<T>` schema, returning its provisional key. Interned by the
    /// `OptionDef` pointer so two `Option<T>` of the same `T` dedup.
    fn intern_option(&mut self, opt: &'static OptionDef) -> Result<usize, DeriveError> {
        let ptr = core::ptr::from_ref(opt) as usize;
        if let Some(&k) = self.by_shape.get(&ptr) {
            return Ok(k);
        }
        let element = self.ref_of(opt.t())?;
        let key = self.protos.len();
        self.by_shape.insert(ptr, key);
        self.protos.push(Schema {
            id: SchemaId(key as u64),
            type_params: Vec::new(),
            kind: SchemaKind::Option { element },
        });
        Ok(key)
    }
}

fn build_descriptor(
    shape: &'static Shape,
    by_shape: &HashMap<usize, usize>,
    real_ids: &[SchemaId],
) -> Result<Descriptor, DeriveError> {
    let (size, align) = layout_of(shape)?;
    if is_string(shape) {
        return string_descriptor(shape);
    }
    if let Some(p) = scalar_primitive(shape)? {
        return Ok(Descriptor {
            schema: SchemaRef::concrete(primitive_id(p)),
            layout: Layout { size, align },
            access: Access::Scalar,
        });
    }
    if let Some(list_def) = list_def(shape) {
        let real = real_ids[by_shape[&(core::ptr::from_ref(list_def) as usize)]];
        let element = build_descriptor(list_def.t(), by_shape, real_ids)?;
        return Ok(Descriptor {
            schema: SchemaRef::concrete(real),
            layout: Layout { size, align },
            access: Access::Sequence(SequenceAccess {
                element: Box::new(element),
                storage: SequenceStorage::Vtable(list_thunks(list_def)),
            }),
        });
    }
    if let Some(opt) = option_def(shape) {
        let real = real_ids[by_shape[&(core::ptr::from_ref(opt) as usize)]];
        let some = build_descriptor(opt.t(), by_shape, real_ids)?;
        return Ok(Descriptor {
            schema: SchemaRef::concrete(real),
            layout: Layout { size, align },
            access: Access::Option(OptionAccess {
                presence: Presence::Vtable(option_thunks(opt)),
                some: Box::new(some),
            }),
        });
    }
    let fields = struct_fields(shape)?;
    let real = real_ids[by_shape[&shape_ptr(shape)]];
    let mut accesses = Vec::with_capacity(fields.len());
    for f in fields {
        accesses.push(FieldAccess {
            offset: f.offset,
            descriptor: build_descriptor(f.shape(), by_shape, real_ids)?,
        });
    }
    Ok(Descriptor {
        schema: SchemaRef::concrete(real),
        layout: Layout { size, align },
        access: Access::Record(RecordAccess {
            fields: accesses,
            construct: Construct::InPlace,
        }),
    })
}

fn shape_ptr(shape: &'static Shape) -> usize {
    core::ptr::from_ref(shape) as usize
}

fn is_struct(shape: &Shape) -> bool {
    matches!(&shape.ty, Type::User(UserType::Struct(_)))
}

fn struct_fields(shape: &'static Shape) -> Result<&'static [facet::Field], DeriveError> {
    match &shape.ty {
        Type::User(UserType::Struct(st)) => Ok(st.fields),
        _ => Err(DeriveError::Unsupported("derive: expected a struct")),
    }
}

fn layout_of(shape: &Shape) -> Result<(usize, usize), DeriveError> {
    let layout = shape.layout.sized_layout().map_err(|_| DeriveError::Unsized)?;
    Ok((layout.size(), layout.align()))
}

/// The `&'static ListDef` behind a list-typed shape (`Vec<T>`, …), or `None`.
fn list_def(shape: &'static Shape) -> Option<&'static ListDef> {
    match &shape.def {
        Def::List(list_def) => Some(list_def),
        _ => None,
    }
}

/// The `&'static OptionDef` behind an `Option<T>`-typed shape, or `None`.
fn option_def(shape: &'static Shape) -> Option<&'static OptionDef> {
    match &shape.def {
        Def::Option(opt) => Some(opt),
        _ => None,
    }
}

// ============================================================================
// Sequence thunks
// ============================================================================
//
// The engine drives owned sequences through three `unsafe extern "C"` function
// pointers (`SeqThunks`), passing an opaque `ctx`. We use the field's
// `&'static ListDef` as that `ctx`: each adapter casts it back, wraps the
// engine's raw `*mut u8`/`*const u8` handle in facet's wide-pointer types, and
// calls the matching `ListDef` operation. The adapters are fixed — not generated
// per element type — because the per-`T` knowledge lives in the `ListDef`'s
// `type_ops`. The engine owns the element buffer; `from_raw_parts` adopts it.
//
// Spec: `r[descriptors.thunk-binding]`.

/// Build the [`SeqThunks`] for a list field, with the field's `ListDef` as `ctx`.
///
/// # Panics
/// If the `ListDef` lacks `from_raw_parts` or `as_ptr` (every `Vec<T>` has both;
/// other list types may not, in which case the typed path cannot carry them).
fn list_thunks(list_def: &'static ListDef) -> SeqThunks {
    assert!(
        list_def.from_raw_parts().is_some(),
        "list type has no from_raw_parts op; cannot decode through the typed path"
    );
    assert!(
        list_def.vtable.as_ptr.is_some(),
        "list type is not contiguous (no as_ptr); cannot encode through the typed path"
    );
    SeqThunks {
        ctx: core::ptr::from_ref(list_def).cast::<()>(),
        from_raw_parts: seq_from_raw_parts,
        len: seq_len,
        data: seq_data,
    }
}

/// Adopt an engine-allocated buffer of `len` (capacity `cap`) elements into the
/// list at `list`, via the `ListDef`'s `from_raw_parts`.
///
/// # Safety
/// `ctx` must be a `&'static ListDef` (as set by [`list_thunks`]); `list` must be
/// uninitialized storage for the list handle; `ptr`/`len`/`cap` must satisfy the
/// list's `from_raw_parts` contract (the engine guarantees the buffer layout).
unsafe extern "C" fn seq_from_raw_parts(
    ctx: *const (),
    list: *mut u8,
    ptr: *mut u8,
    len: usize,
    cap: usize,
) {
    // Safety: `ctx` is the `&'static ListDef` set in `list_thunks`.
    let list_def = unsafe { &*ctx.cast::<ListDef>() };
    let from_raw_parts = list_def
        .from_raw_parts()
        .expect("from_raw_parts presence checked in list_thunks");
    // Safety: forwarded from this fn's contract; the list handle and element
    // buffer are thin pointers, so the facet wide-pointer wrappers are exact.
    unsafe { from_raw_parts(PtrUninit::new(list), PtrMut::new(ptr), len, cap) };
}

/// The list's current element count, via the `ListDef` vtable's `len`.
///
/// # Safety
/// `ctx` must be a `&'static ListDef`; `list` must point to an initialized list
/// handle of the matching type.
unsafe extern "C" fn seq_len(ctx: *const (), list: *const u8) -> usize {
    // Safety: `ctx` is the `&'static ListDef` set in `list_thunks`.
    let list_def = unsafe { &*ctx.cast::<ListDef>() };
    // Safety: `list` is an initialized handle of the list's type.
    unsafe { (list_def.vtable.len)(PtrConst::new(list)) }
}

/// A pointer to the list's contiguous element storage, via the vtable's `as_ptr`.
///
/// # Safety
/// `ctx` must be a `&'static ListDef`; `list` must point to an initialized list
/// handle of the matching type.
unsafe extern "C" fn seq_data(ctx: *const (), list: *const u8) -> *const u8 {
    // Safety: `ctx` is the `&'static ListDef` set in `list_thunks`.
    let list_def = unsafe { &*ctx.cast::<ListDef>() };
    let as_ptr = list_def
        .vtable
        .as_ptr
        .expect("as_ptr presence checked in list_thunks");
    // Safety: `list` is an initialized handle; the data buffer is a thin pointer.
    let data = unsafe { as_ptr(PtrConst::new(list)) };
    data.as_byte_ptr()
}

// ============================================================================
// Option thunks
// ============================================================================
//
// Like the sequence thunks, the engine drives an `Option<T>` through type-erased
// `unsafe extern "C"` function pointers, passing the field's `&'static OptionDef`
// as `ctx`. Each adapter casts it back, wraps the engine's raw pointer in facet's
// wide-pointer types, and calls the matching `OptionVTable` operation — so the
// engine never assumes the in-memory niche/tag layout of `Option<T>`.
//
// Spec: `r[descriptors.thunk-binding]`.

/// Build the [`OptionThunks`] for an `Option` field, with its `OptionDef` as `ctx`.
fn option_thunks(opt: &'static OptionDef) -> OptionThunks {
    OptionThunks {
        ctx: core::ptr::from_ref(opt).cast::<()>(),
        is_some: opt_is_some,
        get_value: opt_get_value,
        init_some: opt_init_some,
        init_none: opt_init_none,
    }
}

/// Whether the option at `option` is `Some`, via the `OptionVTable`'s `is_some`.
///
/// # Safety
/// `ctx` is the `&'static OptionDef` set in [`option_thunks`]; `option` points to
/// an initialized `Option<T>` of the matching type.
unsafe extern "C" fn opt_is_some(ctx: *const (), option: *const u8) -> bool {
    // Safety: `ctx` is the `&'static OptionDef`.
    let opt = unsafe { &*ctx.cast::<OptionDef>() };
    // Safety: `option` is an initialized handle of the option's type.
    unsafe { (opt.vtable.is_some)(PtrConst::new(option)) }
}

/// A pointer to the contained value (valid only when some), via `get_value`.
///
/// # Safety
/// As [`opt_is_some`]; the engine reads the result only when the option is some.
unsafe extern "C" fn opt_get_value(ctx: *const (), option: *const u8) -> *const u8 {
    // Safety: `ctx` is the `&'static OptionDef`.
    let opt = unsafe { &*ctx.cast::<OptionDef>() };
    // Safety: `option` is an initialized handle.
    unsafe { (opt.vtable.get_value)(PtrConst::new(option)) }
}

/// Initialize the uninitialized option at `option` to `Some(*value)`, moving the
/// inner value out of `value`, via `init_some`.
///
/// # Safety
/// `ctx` is the `&'static OptionDef`; `option` is uninitialized option storage;
/// `value` points to an initialized inner value that the engine then frees without
/// dropping (ownership is moved into the option here).
unsafe extern "C" fn opt_init_some(ctx: *const (), option: *mut u8, value: *mut u8) {
    // Safety: `ctx` is the `&'static OptionDef`.
    let opt = unsafe { &*ctx.cast::<OptionDef>() };
    // Safety: `option` is uninitialized; `value` holds the inner value to move in.
    unsafe { (opt.vtable.init_some)(PtrUninit::new(option), PtrMut::new(value)) };
}

/// Initialize the uninitialized option at `option` to `None`, via `init_none`.
///
/// # Safety
/// `ctx` is the `&'static OptionDef`; `option` is uninitialized option storage.
unsafe extern "C" fn opt_init_none(ctx: *const (), option: *mut u8) {
    // Safety: `ctx` is the `&'static OptionDef`.
    let opt = unsafe { &*ctx.cast::<OptionDef>() };
    // Safety: `option` is uninitialized storage for the option.
    unsafe { (opt.vtable.init_none)(PtrUninit::new(option)) };
}

// ============================================================================
// String (a bulk byte run, validated as UTF-8)
// ============================================================================
//
// A `String` field's *schema* is `Primitive::String`, but its *descriptor* is an
// owned byte sequence: the engine bulk-copies the bytes, validates UTF-8, and
// adopts them via `String::from_raw_parts`. The thunks are concrete — `String` is
// a single type, so no facet vtable is needed.

fn is_string(shape: &Shape) -> bool {
    matches!(shape.scalar_type(), Some(ScalarType::String))
}

/// The descriptor for a `String` field or root: schema `Primitive::String`, an
/// owned byte-sequence access carrying the concrete `String` thunks.
fn string_descriptor(shape: &'static Shape) -> Result<Descriptor, DeriveError> {
    let (size, align) = layout_of(shape)?;
    Ok(Descriptor {
        schema: SchemaRef::concrete(primitive_id(Primitive::String)),
        layout: Layout { size, align },
        access: Access::Sequence(SequenceAccess {
            element: Box::new(Descriptor {
                schema: SchemaRef::concrete(primitive_id(Primitive::U8)),
                layout: Layout { size: 1, align: 1 },
                access: Access::Scalar,
            }),
            storage: SequenceStorage::Vtable(SeqThunks {
                ctx: core::ptr::null(),
                from_raw_parts: string_from_raw_parts,
                len: string_len,
                data: string_data,
            }),
        }),
    })
}

/// Adopt the engine's UTF-8-validated buffer into the `String` at `list`.
///
/// # Safety
/// `list` is uninitialized `String` storage; the engine validated the bytes as
/// UTF-8 and `ptr`/`len`/`cap` satisfy `String::from_raw_parts`.
unsafe extern "C" fn string_from_raw_parts(
    _ctx: *const (),
    list: *mut u8,
    ptr: *mut u8,
    len: usize,
    cap: usize,
) {
    // Safety: forwarded; the engine guarantees valid UTF-8 and a matching layout.
    let s = unsafe { String::from_raw_parts(ptr, len, cap) };
    unsafe { core::ptr::write(list.cast::<String>(), s) };
}

/// The `String`'s byte length.
///
/// # Safety
/// `list` points to an initialized `String`.
unsafe extern "C" fn string_len(_ctx: *const (), list: *const u8) -> usize {
    let s: &String = unsafe { &*list.cast::<String>() };
    s.len()
}

/// A pointer to the `String`'s bytes.
///
/// # Safety
/// `list` points to an initialized `String`.
unsafe extern "C" fn string_data(_ctx: *const (), list: *const u8) -> *const u8 {
    let s: &String = unsafe { &*list.cast::<String>() };
    s.as_ptr()
}

/// Map a fixed-width scalar shape to a phon primitive. `Ok(None)` when the shape
/// is not a scalar at all (e.g. a struct); an error for scalar kinds the typed
/// path cannot yet carry (`usize`/`isize`, net types, …). `String` is handled
/// separately (see [`is_string`]) — it is a byte sequence, not a fixed scalar.
fn scalar_primitive(shape: &Shape) -> Result<Option<Primitive>, DeriveError> {
    let Some(scalar) = shape.scalar_type() else {
        return Ok(None);
    };
    Ok(Some(match scalar {
        ScalarType::Unit => Primitive::Unit,
        ScalarType::Bool => Primitive::Bool,
        ScalarType::Char => Primitive::Char,
        ScalarType::U8 => Primitive::U8,
        ScalarType::U16 => Primitive::U16,
        ScalarType::U32 => Primitive::U32,
        ScalarType::U64 => Primitive::U64,
        ScalarType::U128 => Primitive::U128,
        ScalarType::I8 => Primitive::I8,
        ScalarType::I16 => Primitive::I16,
        ScalarType::I32 => Primitive::I32,
        ScalarType::I64 => Primitive::I64,
        ScalarType::I128 => Primitive::I128,
        ScalarType::F32 => Primitive::F32,
        ScalarType::F64 => Primitive::F64,
        _ => {
            return Err(DeriveError::Unsupported(
                "derive: scalar type not supported yet (string, usize/isize, net, …)",
            ));
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_value::{VArray, VObject, VString, Value};
    use phon_engine::{Registry, compact, typed};

    // repr(Rust): the compiler may reorder these in memory, so the descriptor's
    // offsets (from facet) are not the schema/wire order. The cross-check below
    // only passes if the bridge reads real offsets — exactly what offset_of! used
    // to fake.
    #[derive(Facet)]
    struct Pt {
        a: u8,
        b: u64,
        c: u16,
        flag: bool,
    }

    #[derive(Facet)]
    struct Outer {
        tag: u8,
        inner: Pt,
        n: u32,
    }

    fn pt_object(a: u8, b: u64, c: u16, flag: bool) -> Value {
        let mut o = VObject::new();
        o.insert(VString::new("a"), Value::from(a));
        o.insert(VString::new("b"), Value::from(b));
        o.insert(VString::new("c"), Value::from(c));
        o.insert(VString::new("flag"), Value::from(flag));
        o.into()
    }

    #[test]
    fn derived_struct_typed_matches_dynamic_and_roundtrips() {
        let d = of::<Pt>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        let p = Pt {
            a: 0x11,
            b: 0x2222_2222_2222_2222,
            c: 0x3333,
            flag: true,
        };
        let typed_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&p).cast::<u8>(), &d.descriptor, &reg) }
                .unwrap();

        // Oracle: byte-identical to the dynamic codec for the equivalent object.
        let dyn_bytes =
            compact::to_bytes(&pt_object(p.a, p.b, p.c, p.flag), d.root, &reg).unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        // Round-trip back into a Pt.
        let mut slot = std::mem::MaybeUninit::<Pt>::uninit();
        unsafe { typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
            .unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.a, p.a);
        assert_eq!(back.b, p.b);
        assert_eq!(back.c, p.c);
        assert_eq!(back.flag, p.flag);
    }

    #[test]
    fn derived_nested_struct_matches_dynamic() {
        let d = of::<Outer>().unwrap();
        // Two composite schemas reachable: Outer and Pt.
        assert_eq!(d.schemas.len(), 2);
        let reg = Registry::new(d.schemas.clone());

        let o = Outer {
            tag: 0x77,
            inner: Pt {
                a: 1,
                b: 1 << 40,
                c: 9,
                flag: false,
            },
            n: 0xDEAD_BEEF,
        };
        let typed_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&o).cast::<u8>(), &d.descriptor, &reg) }
                .unwrap();

        let mut obj = VObject::new();
        obj.insert(VString::new("tag"), Value::from(o.tag));
        obj.insert(
            VString::new("inner"),
            pt_object(o.inner.a, o.inner.b, o.inner.c, o.inner.flag),
        );
        obj.insert(VString::new("n"), Value::from(o.n));
        let dyn_bytes = compact::to_bytes(&obj.into(), d.root, &reg).unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        let mut slot = std::mem::MaybeUninit::<Outer>::uninit();
        unsafe { typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
            .unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.tag, o.tag);
        assert_eq!(back.inner.b, o.inner.b);
        assert_eq!(back.n, o.n);
    }

    // A struct with an owned `Vec<u32>` field: exercises the sequence bridge end
    // to end, through real facet reflection (no hand-written thunks).
    #[derive(Facet)]
    struct Holder {
        items: Vec<u32>,
        tag: u32,
    }

    #[test]
    fn derived_vec_field_typed_matches_dynamic_and_roundtrips() {
        let d = of::<Holder>().unwrap();
        // Two composite schemas reachable: Holder (struct) and Vec<u32> (list).
        assert_eq!(d.schemas.len(), 2);
        let reg = Registry::new(d.schemas.clone());

        let h = Holder {
            items: vec![1, 2, 999, 0xDEAD_BEEF],
            tag: 0x55,
        };
        let typed_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&h).cast::<u8>(), &d.descriptor, &reg) }
                .unwrap();

        // Oracle: byte-identical to the dynamic codec for the equivalent object
        // (a VArray `items` and a number `tag`).
        let mut arr = VArray::new();
        for &v in &h.items {
            arr.push(Value::from(v));
        }
        let mut obj = VObject::new();
        obj.insert(VString::new("items"), Value::from(arr));
        obj.insert(VString::new("tag"), Value::from(h.tag));
        let dyn_bytes = compact::to_bytes(&obj.into(), d.root, &reg).unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        // Round-trip: decode back into a Holder and check the Vec and scalar.
        let mut slot = std::mem::MaybeUninit::<Holder>::uninit();
        unsafe { typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
            .unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.items, h.items);
        assert_eq!(back.tag, h.tag);
    }

    // A struct with an owned `String` field: the bulk byte run, end to end.
    #[derive(Facet)]
    struct Named {
        name: String,
        id: u32,
    }

    #[test]
    fn derived_string_field_matches_dynamic_and_roundtrips() {
        let d = of::<Named>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let v = Named {
            name: "héllo wörld 🐝".to_string(),
            id: 0x42,
        };

        let typed_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&v).cast::<u8>(), &d.descriptor, &reg) }
                .unwrap();

        let mut obj = VObject::new();
        obj.insert(VString::new("name"), Value::from(v.name.as_str()));
        obj.insert(VString::new("id"), Value::from(v.id));
        let dyn_bytes = compact::to_bytes(&obj.into(), d.root, &reg).unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        let mut slot = std::mem::MaybeUninit::<Named>::uninit();
        unsafe { typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
            .unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.name, v.name);
        assert_eq!(back.id, v.id);
    }

    #[test]
    fn derived_string_rejects_invalid_utf8() {
        use phon_engine::CompactError;
        use phon_schema::DecodeError;

        let d = of::<Named>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        // name = one byte 0xFF (not valid UTF-8), then the u32 id at its alignment.
        let mut wire = Vec::new();
        wire.extend_from_slice(&1u32.to_le_bytes()); // name length 1
        wire.push(0xFF); // invalid UTF-8
        wire.extend_from_slice(&[0, 0, 0]); // pad pos 5 -> 8 for the u32
        wire.extend_from_slice(&0x42u32.to_le_bytes());

        let mut slot = std::mem::MaybeUninit::<Named>::uninit();
        let err =
            unsafe { typed::decode(&wire, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
                .unwrap_err();
        assert!(matches!(err, CompactError::Decode(DecodeError::InvalidUtf8)));
    }

    // The String bridge through the *JIT*: derive -> lower -> NativeEncode/Decode.
    // This exercises the real `validate_utf8` thunk the lowering installs, flowing
    // through the copy-and-patch stencil as an indirect call.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_string_field_jit_matches_interpreter_and_roundtrips() {
        use phon_jit::native::{NativeDecode, NativeEncode};

        let d = of::<Named>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();
        let v = Named {
            name: "JITful 🐝 héllo wörld".to_string(),
            id: 0x99,
        };

        // JIT encode == interpreter encode == byte-identical wire.
        let jit_bytes =
            unsafe { NativeEncode::compile(&program).run(core::ptr::from_ref(&v).cast::<u8>()) };
        let interp_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&v).cast::<u8>(), &d.descriptor, &reg) }
                .unwrap();
        assert_eq!(jit_bytes, interp_bytes);

        // JIT decode round-trips (validating UTF-8 in-stencil).
        let dec = NativeDecode::compile(&program);
        let mut slot = std::mem::MaybeUninit::<Named>::uninit();
        unsafe { dec.run(&jit_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.name, v.name);
        assert_eq!(back.id, v.id);
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_string_field_jit_rejects_invalid_utf8() {
        use phon_jit::native::NativeDecode;
        use phon_schema::DecodeError;

        let d = of::<Named>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();
        let dec = NativeDecode::compile(&program);

        // name = one byte 0xFF (invalid UTF-8), pad to 4, then the u32 id.
        let mut wire = 1u32.to_le_bytes().to_vec();
        wire.push(0xFF);
        wire.extend_from_slice(&[0, 0, 0]);
        wire.extend_from_slice(&0x99u32.to_le_bytes());

        let mut slot = std::mem::MaybeUninit::<Named>::uninit();
        let err = unsafe { dec.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err, DecodeError::InvalidUtf8));
    }

    // A struct with an `Option<u32>` field: the data-directed presence branch.
    #[derive(Facet)]
    struct Maybe {
        val: Option<u32>,
        tag: u32,
    }

    #[test]
    fn derived_option_u32_matches_dynamic_and_roundtrips() {
        let d = of::<Maybe>().unwrap();
        // Two composite schemas: Maybe (struct) and Option<u32>.
        assert_eq!(d.schemas.len(), 2);
        let reg = Registry::new(d.schemas.clone());

        for val in [Some(0xABCDu32), None] {
            let m = Maybe { val, tag: 0x77 };
            let typed_bytes =
                unsafe { typed::encode(core::ptr::from_ref(&m).cast::<u8>(), &d.descriptor, &reg) }
                    .unwrap();

            let mut obj = VObject::new();
            obj.insert(
                VString::new("val"),
                match val {
                    Some(x) => Value::from(x),
                    None => Value::NULL,
                },
            );
            obj.insert(VString::new("tag"), Value::from(m.tag));
            let dyn_bytes = compact::to_bytes(&obj.into(), d.root, &reg).unwrap();
            assert_eq!(typed_bytes, dyn_bytes, "val = {val:?}");

            let mut slot = std::mem::MaybeUninit::<Maybe>::uninit();
            unsafe {
                typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>())
            }
            .unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back.val, val);
            assert_eq!(back.tag, m.tag);
        }
    }

    // `Option<String>`: a some-payload that owns heap — exercises the decode
    // scratch buffer + `init_some` move (the inner `String` is built into scratch,
    // then moved into the `Option`, then the scratch freed without dropping).
    #[derive(Facet)]
    struct MaybeStr {
        s: Option<String>,
        n: u32,
    }

    #[test]
    fn derived_option_string_matches_dynamic_and_roundtrips() {
        let d = of::<MaybeStr>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        for s in [Some("héllo 🐝".to_string()), None] {
            let m = MaybeStr { s: s.clone(), n: 0x2A };
            let typed_bytes =
                unsafe { typed::encode(core::ptr::from_ref(&m).cast::<u8>(), &d.descriptor, &reg) }
                    .unwrap();

            let mut obj = VObject::new();
            obj.insert(
                VString::new("s"),
                match &s {
                    Some(x) => Value::from(x.as_str()),
                    None => Value::NULL,
                },
            );
            obj.insert(VString::new("n"), Value::from(m.n));
            let dyn_bytes = compact::to_bytes(&obj.into(), d.root, &reg).unwrap();
            assert_eq!(typed_bytes, dyn_bytes, "s = {s:?}");

            let mut slot = std::mem::MaybeUninit::<MaybeStr>::uninit();
            unsafe {
                typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>())
            }
            .unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back.s, s);
            assert_eq!(back.n, m.n);
        }
    }

    #[test]
    fn derived_option_rejects_invalid_presence() {
        use phon_engine::CompactError;
        use phon_schema::DecodeError;
        let d = of::<Maybe>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        // presence byte 2 (neither 0 nor 1) — rejected like the dynamic codec.
        let wire = vec![2u8];
        let mut slot = std::mem::MaybeUninit::<Maybe>::uninit();
        let err =
            unsafe { typed::decode(&wire, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
                .unwrap_err();
        assert!(matches!(err, CompactError::Decode(DecodeError::InvalidBool(2))));
    }
}
