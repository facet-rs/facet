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
    Def, DefaultSource, EnumRepr, EnumType, Facet, ListDef, MapDef, OptionDef, PtrConst, PtrMut,
    PtrUninit, ScalarType, Shape, StructKind, Type, UserType,
};
use phon_ir::{
    Access, BorrowThunks, Construct, Descriptor, EnumAccess, FieldAccess, FieldDefault, Layout,
    MapAccess, MapStorage, MapThunks, OptionAccess, OptionThunks, Presence, RecordAccess, SeqThunks,
    SequenceAccess, SequenceStorage, Tag, VariantAccess,
};
use phon_schema::{
    Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, Variant, VariantPayload,
    primitive_id, resolve_ids,
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
    // A borrowed `&str`/`&[u8]` root: same wire primitive as its owned peer, a
    // zero-copy byte sequence.
    if let Some(kind) = borrowed_kind(shape)? {
        return Ok(Derived {
            root: primitive_id(borrow_primitive(kind)),
            schemas: Vec::new(),
            descriptor: borrowed_descriptor(shape, kind)?,
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
    // Pass 1: intern composites with provisional dense-index keys, building proto
    // schemas whose references use those keys (primitives use their real id). The
    // root may be a struct or a `#[repr(int)]` enum (RPC messages are often a
    // top-level sum type).
    let mut b = Builder::default();
    let root_key = if is_struct(shape) {
        b.intern(shape)?
    } else if let Some(et) = enum_type(shape) {
        b.intern_enum(shape, et)?
    } else {
        return Err(DeriveError::Unsupported(
            "derive root must be a struct, enum, or fixed scalar so far",
        ));
    };
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
                // A `#[facet(default)]` field can be filled in when the writer
                // omits it, so it is not required (`r[compat.reader-only-fields]`).
                required: f.default.is_none(),
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
        // A borrowed `&str`/`&[u8]` field: the SAME wire primitive as its owned peer
        // (`String`/`Vec<u8>`), so the schema id matches an owned peer's exactly.
        if let Some(kind) = borrowed_kind(shape)? {
            return Ok(SchemaRef::concrete(primitive_id(borrow_primitive(kind))));
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
        } else if let Some(map_def) = map_def(shape) {
            let key = self.intern_map(map_def)?;
            Ok(SchemaRef::concrete(SchemaId(key as u64)))
        } else if let Some(et) = enum_type(shape) {
            let key = self.intern_enum(shape, et)?;
            Ok(SchemaRef::concrete(SchemaId(key as u64)))
        } else {
            Err(DeriveError::Unsupported(
                "derive: only structs, lists, options, maps, enums, and fixed scalars so far",
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

    /// Intern a `Map<K, V>` schema (e.g. `BTreeMap<K, V>`, `HashMap<K, V>`),
    /// returning its provisional key. The key and value references are resolved
    /// first, then a `Map` schema is appended. Interned by the `MapDef` pointer so
    /// two maps of the same `K`/`V` dedup.
    fn intern_map(&mut self, map_def: &'static MapDef) -> Result<usize, DeriveError> {
        let ptr = core::ptr::from_ref(map_def) as usize;
        if let Some(&k) = self.by_shape.get(&ptr) {
            return Ok(k);
        }
        let key = self.ref_of(map_def.k())?;
        let value = self.ref_of(map_def.v())?;
        let slot = self.protos.len();
        self.by_shape.insert(ptr, slot);
        self.protos.push(Schema {
            id: SchemaId(slot as u64),
            type_params: Vec::new(),
            kind: SchemaKind::Map { key, value },
        });
        Ok(slot)
    }

    /// Intern an enum schema, returning its provisional key. Only `#[repr(int)]`
    /// enums are supported (a default `repr(Rust)` discriminant layout is
    /// unspecified). Interned by the enum's `Shape` pointer, like a struct; the
    /// slot is reserved before recursing so a self-reference resolves.
    fn intern_enum(
        &mut self,
        shape: &'static Shape,
        et: &'static EnumType,
    ) -> Result<usize, DeriveError> {
        if enum_repr_width(et.enum_repr).is_none() {
            return Err(DeriveError::Unsupported(
                "derive: only #[repr(uN/iN)] enums (default repr(Rust) discriminant is unspecified)",
            ));
        }
        let ptr = shape_ptr(shape);
        if let Some(&k) = self.by_shape.get(&ptr) {
            return Ok(k);
        }
        let key = self.protos.len();
        self.by_shape.insert(ptr, key);
        self.protos.push(Schema {
            id: SchemaId(key as u64),
            type_params: Vec::new(),
            kind: SchemaKind::Dynamic, // placeholder until variants resolve
        });
        let variants = self.enum_variants(et)?;
        self.protos[key].kind = SchemaKind::Enum {
            name: shape.type_identifier.to_string(),
            variants,
        };
        Ok(key)
    }

    /// Build the schema variants: each gets its position as a stable wire index and
    /// a payload classified from facet's variant struct kind.
    fn enum_variants(&mut self, et: &'static EnumType) -> Result<Vec<Variant>, DeriveError> {
        let mut out = Vec::with_capacity(et.variants.len());
        for (i, v) in et.variants.iter().enumerate() {
            out.push(Variant {
                name: v.name.to_string(),
                index: i as u32,
                payload: self.variant_payload(v)?,
            });
        }
        Ok(out)
    }

    /// Classify a facet variant's payload into a [`VariantPayload`]. The wire bytes
    /// are the fields in order regardless of this shape; the classification only
    /// gives the schema its structure.
    fn variant_payload(&mut self, v: &'static facet::Variant) -> Result<VariantPayload, DeriveError> {
        let fields = v.data.fields;
        if fields.is_empty() {
            return Ok(VariantPayload::Unit);
        }
        match v.data.kind {
            StructKind::Struct => {
                let mut fs = Vec::with_capacity(fields.len());
                for f in fields {
                    fs.push(Field {
                        name: f.name.to_string(),
                        schema: self.ref_of(f.shape())?,
                        required: f.default.is_none(),
                    });
                }
                Ok(VariantPayload::Struct(fs))
            }
            StructKind::Tuple | StructKind::TupleStruct if fields.len() == 1 => {
                Ok(VariantPayload::Newtype(self.ref_of(fields[0].shape())?))
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                let mut refs = Vec::with_capacity(fields.len());
                for f in fields {
                    refs.push(self.ref_of(f.shape())?);
                }
                Ok(VariantPayload::Tuple(refs))
            }
            StructKind::Unit => Ok(VariantPayload::Unit),
        }
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
    // A borrowed `&str`/`&[u8]` field: a zero-copy byte-sequence descriptor (same
    // wire as its owned peer).
    if let Some(kind) = borrowed_kind(shape)? {
        return borrowed_descriptor(shape, kind);
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
    if let Some(map_def) = map_def(shape) {
        let real = real_ids[by_shape[&(core::ptr::from_ref(map_def) as usize)]];
        let key = build_descriptor(map_def.k(), by_shape, real_ids)?;
        let value = build_descriptor(map_def.v(), by_shape, real_ids)?;
        return Ok(Descriptor {
            schema: SchemaRef::concrete(real),
            layout: Layout { size, align },
            access: Access::Map(MapAccess {
                key: Box::new(key),
                value: Box::new(value),
                storage: MapStorage::Vtable(map_thunks(map_def)),
            }),
        });
    }
    if let Some(et) = enum_type(shape) {
        let width = enum_repr_width(et.enum_repr).ok_or(DeriveError::Unsupported(
            "derive: only #[repr(uN/iN)] enums",
        ))?;
        let real = real_ids[by_shape[&shape_ptr(shape)]];
        let mut variants = Vec::with_capacity(et.variants.len());
        for (i, v) in et.variants.iter().enumerate() {
            // Variant field offsets already account for the discriminant (facet).
            let mut fields = Vec::with_capacity(v.data.fields.len());
            for f in v.data.fields {
                fields.push(FieldAccess {
                    offset: f.offset,
                    descriptor: build_descriptor(f.shape(), by_shape, real_ids)?,
                    default: field_default(f),
                });
            }
            variants.push(VariantAccess {
                index: i as u32,
                // The in-memory discriminant (selector) — explicit value if any,
                // else the variant position (which is the implicit discriminant).
                selector: v.discriminant.unwrap_or(i as i64) as u64,
                payload: RecordAccess {
                    fields,
                    construct: Construct::InPlace,
                },
            });
        }
        return Ok(Descriptor {
            schema: SchemaRef::concrete(real),
            layout: Layout { size, align },
            access: Access::Enum(EnumAccess {
                // #[repr(int)] enums keep the discriminant first, at offset 0.
                tag: Tag::Direct { offset: 0, width },
                variants,
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
            default: field_default(f),
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

/// The `&'static MapDef` behind a map-typed shape (`BTreeMap<K, V>`,
/// `HashMap<K, V>`, …), or `None`.
fn map_def(shape: &'static Shape) -> Option<&'static MapDef> {
    match &shape.def {
        Def::Map(map_def) => Some(map_def),
        _ => None,
    }
}

/// The `&'static EnumType` behind an enum-typed shape, or `None`.
fn enum_type(shape: &'static Shape) -> Option<&'static EnumType> {
    match &shape.ty {
        Type::User(UserType::Enum(et)) => Some(et),
        _ => None,
    }
}

/// The discriminant width in bytes for a `#[repr(uN/iN)]` enum, or `None` for a
/// default `repr(Rust)`/niche enum whose discriminant layout is unspecified (and
/// so cannot be read or written from a layout fact).
fn enum_repr_width(repr: EnumRepr) -> Option<usize> {
    Some(match repr {
        EnumRepr::U8 | EnumRepr::I8 => 1,
        EnumRepr::U16 | EnumRepr::I16 => 2,
        EnumRepr::U32 | EnumRepr::I32 => 4,
        EnumRepr::U64 | EnumRepr::I64 => 8,
        EnumRepr::USize | EnumRepr::ISize => core::mem::size_of::<usize>(),
        EnumRepr::Rust | EnumRepr::RustNPO => return None,
    })
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
// Map thunks
// ============================================================================
//
// Like the sequence and option thunks, the engine drives an owned map through
// type-erased `unsafe extern "C"` function pointers, passing the field's
// `&'static MapDef` as `ctx`. Each adapter casts it back, wraps the engine's raw
// pointers in facet's wide-pointer types, and calls the matching `MapVTable`
// operation — so the engine never assumes the map's in-memory layout.
//
// The encode iterator needs care: facet's `init_with_value` returns a *wide*
// `PtrMut` (16 bytes) that cannot pass through the engine as a thin `*mut ()`. We
// box it: `map_iter_init` returns `Box::into_raw(Box::new(iter_ptr_mut))`, and
// `map_iter_next`/`map_iter_dealloc` reach the `PtrMut` behind that box (it is
// `Copy`, and the iterator state lives behind it, so passing a copy advances it).
//
// Spec: `r[descriptors.thunk-binding]`.

/// Build the [`MapThunks`] for a map field, with the field's `MapDef` as `ctx`.
fn map_thunks(map_def: &'static MapDef) -> MapThunks {
    MapThunks {
        ctx: core::ptr::from_ref(map_def).cast::<()>(),
        len: map_len,
        init_with_capacity: map_init_with_capacity,
        insert: map_insert,
        iter_init: map_iter_init,
        iter_next: map_iter_next,
        iter_dealloc: map_iter_dealloc,
    }
}

/// The map's current entry count, via the `MapVTable`'s `len`.
///
/// # Safety
/// `ctx` must be a `&'static MapDef` (as set by [`map_thunks`]); `map` must point
/// to an initialized map handle of the matching type.
unsafe extern "C" fn map_len(ctx: *const (), map: *const u8) -> usize {
    // Safety: `ctx` is the `&'static MapDef` set in `map_thunks`.
    let map_def = unsafe { &*ctx.cast::<MapDef>() };
    // Safety: `map` is an initialized handle of the map's type.
    unsafe { (map_def.vtable.len)(PtrConst::new(map)) }
}

/// Initialize the uninitialized map at `map` with room for `cap` entries, via
/// `init_in_place_with_capacity`.
///
/// # Safety
/// `ctx` must be a `&'static MapDef`; `map` must be uninitialized storage for the
/// map handle of the matching type.
unsafe extern "C" fn map_init_with_capacity(ctx: *const (), map: *mut u8, cap: usize) {
    // Safety: `ctx` is the `&'static MapDef`.
    let map_def = unsafe { &*ctx.cast::<MapDef>() };
    // Safety: `map` is uninitialized storage for the map.
    unsafe { (map_def.vtable.init_in_place_with_capacity)(PtrUninit::new(map), cap) };
}

/// Insert `(*key, *value)` into the initialized map at `map`, moving the key and
/// value out of their buffers (the engine then frees both without dropping), via
/// `insert`.
///
/// # Safety
/// `ctx` must be a `&'static MapDef`; `map` must be an initialized map handle;
/// `key`/`value` must point to initialized values of the map's key/value types
/// that the engine then frees without dropping (ownership is moved in here).
unsafe extern "C" fn map_insert(ctx: *const (), map: *mut u8, key: *mut u8, value: *mut u8) {
    // Safety: `ctx` is the `&'static MapDef`.
    let map_def = unsafe { &*ctx.cast::<MapDef>() };
    // Safety: `map` is initialized; `key`/`value` hold the entry to move in.
    unsafe { (map_def.vtable.insert)(PtrMut::new(map), PtrMut::new(key), PtrMut::new(value)) };
}

/// Build a boxed iterator over the entries of the initialized map at `map`, via
/// the iter vtable's `init_with_value`. The returned wide `PtrMut` is boxed so the
/// engine can carry it as a thin `*mut ()`.
///
/// # Safety
/// `ctx` must be a `&'static MapDef`; `map` must be an initialized map handle.
unsafe extern "C" fn map_iter_init(ctx: *const (), map: *const u8) -> *mut () {
    // Safety: `ctx` is the `&'static MapDef`.
    let map_def = unsafe { &*ctx.cast::<MapDef>() };
    let init = map_def
        .vtable
        .iter_vtable
        .init_with_value
        .expect("map iterator has no init_with_value; cannot encode through the typed path");
    // Safety: `map` is an initialized handle of the map's type.
    let it: PtrMut = unsafe { init(PtrConst::new(map)) };
    // Box the wide `PtrMut` so it fits the engine's thin `*mut ()` handle.
    Box::into_raw(Box::new(it)).cast::<()>()
}

/// Advance the boxed iterator, writing the next entry's borrowed key/value byte
/// pointers and returning `true`, or returning `false` at the end. Via the iter
/// vtable's `next` (a Rust-ABI fn pointer, called directly).
///
/// # Safety
/// `ctx` must be a `&'static MapDef`; `iter` must be a boxed `PtrMut` from
/// [`map_iter_init`]; `key_out`/`value_out` must be writable.
unsafe extern "C" fn map_iter_next(
    ctx: *const (),
    iter: *mut (),
    key_out: *mut *const u8,
    value_out: *mut *const u8,
) -> bool {
    // Safety: `ctx` is the `&'static MapDef`.
    let map_def = unsafe { &*ctx.cast::<MapDef>() };
    // Safety: `iter` is the boxed `PtrMut` from `map_iter_init`.
    let bx = iter.cast::<PtrMut>();
    // `PtrMut` is `Copy`; the iterator state lives behind it, so passing a copy
    // advances it.
    match unsafe { (map_def.vtable.iter_vtable.next)(*bx) } {
        Some((k, v)) => {
            // Safety: the out-params are writable.
            unsafe {
                *key_out = k.as_byte_ptr();
                *value_out = v.as_byte_ptr();
            }
            true
        }
        None => false,
    }
}

/// Free the boxed iterator built by [`map_iter_init`], via the iter vtable's
/// `dealloc` (then the `Box` drops).
///
/// # Safety
/// `ctx` must be a `&'static MapDef`; `iter` must be a boxed `PtrMut` from
/// [`map_iter_init`], freed exactly once.
unsafe extern "C" fn map_iter_dealloc(ctx: *const (), iter: *mut ()) {
    // Safety: `ctx` is the `&'static MapDef`.
    let map_def = unsafe { &*ctx.cast::<MapDef>() };
    // Safety: `iter` is the boxed `PtrMut` from `map_iter_init`, taken back exactly
    // once; the `Box` is dropped at the end of this scope.
    let bx = unsafe { Box::from_raw(iter.cast::<PtrMut>()) };
    // Safety: `*bx` is the live iterator built by `init_with_value`.
    unsafe { (map_def.vtable.iter_vtable.dealloc)(*bx) };
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

/// Which borrowed, zero-copy leaf a `Shape` is, if any.
#[derive(Clone, Copy)]
enum BorrowKind {
    /// `&str`: schema `Primitive::String`, validated as UTF-8 on construct.
    Str,
    /// `&[u8]`: schema `Primitive::Bytes`, no content constraint.
    Bytes,
}

/// Classify a `Shape` as a borrowed leaf. `&str` is a `ScalarType::Str` scalar;
/// `&[u8]` is a `&T` reference (`Def::Pointer`) whose pointee is a `[u8]` slice
/// (`Def::Slice` with a `u8` element). A borrowed slice of a non-`u8` element is
/// the common-RPC-leaf scope's boundary: it is rejected as unsupported (other
/// `&[T]` are not the zero-copy leaves this targets — `r[descriptors.thunk-binding]`).
///
/// Returns `Ok(None)` when the shape is not a borrowed leaf at all (so the caller
/// falls through to its other arms).
///
/// # Errors
/// [`DeriveError::Unsupported`] for a borrowed slice of a non-`u8` element.
fn borrowed_kind(shape: &Shape) -> Result<Option<BorrowKind>, DeriveError> {
    // `&str` — facet special-cases it as a `ScalarType::Str` scalar.
    if matches!(shape.scalar_type(), Some(ScalarType::Str)) {
        return Ok(Some(BorrowKind::Str));
    }
    // `&[T]` — a shared reference whose pointee is a slice. Only `&[u8]` is carried.
    if let Def::Pointer(ptr) = &shape.def
        && let Some(pointee) = ptr.pointee
        && let Def::Slice(slice_def) = &pointee.def
    {
        if matches!(slice_def.t().scalar_type(), Some(ScalarType::U8)) {
            return Ok(Some(BorrowKind::Bytes));
        }
        return Err(DeriveError::Unsupported(
            "borrowed slice of non-u8 not supported yet",
        ));
    }
    Ok(None)
}

/// The wire primitive a borrowed leaf encodes as — IDENTICAL to its owned peer, so
/// the schema id matches (`&str` ⟷ `String`, `&[u8]` ⟷ `Vec<u8>`).
fn borrow_primitive(kind: BorrowKind) -> Primitive {
    match kind {
        BorrowKind::Str => Primitive::String,
        BorrowKind::Bytes => Primitive::Bytes,
    }
}

/// The descriptor for a borrowed leaf (`&str`/`&[u8]`): the same wire primitive as
/// its owned peer, a byte-sequence access carrying the concrete borrow thunks. The
/// thunks are concrete — `&str`/`&[u8]` are single types, so no facet vtable is
/// needed and `ctx` is null.
fn borrowed_descriptor(
    shape: &'static Shape,
    kind: BorrowKind,
) -> Result<Descriptor, DeriveError> {
    let (size, align) = layout_of(shape)?;
    let p = borrow_primitive(kind);
    let thunks = match kind {
        BorrowKind::Str => BorrowThunks {
            ctx: core::ptr::null(),
            set_borrowed: str_set_borrowed,
            len: str_len,
            data: str_data,
        },
        BorrowKind::Bytes => BorrowThunks {
            ctx: core::ptr::null(),
            set_borrowed: bytes_set_borrowed,
            len: bytes_len,
            data: bytes_data,
        },
    };
    Ok(Descriptor {
        schema: SchemaRef::concrete(primitive_id(p)),
        layout: Layout { size, align },
        access: Access::Sequence(SequenceAccess {
            element: Box::new(Descriptor {
                schema: SchemaRef::concrete(primitive_id(Primitive::U8)),
                layout: Layout { size: 1, align: 1 },
                access: Access::Scalar,
            }),
            storage: SequenceStorage::BorrowedVtable(thunks),
        }),
    })
}

// ----------------------------------------------------------------------------
// Borrowed `&str` thunks (a UTF-8-validated fat pointer into the input).
// ----------------------------------------------------------------------------
//
// The fat-pointer layout of `&str` is unspecified, so the engine never writes it
// at a fixed offset — these concrete thunks build it where the type is known. They
// use explicit `&*` references (not `field.cast::<&str>().len()`) so the
// implicit-autoref lint stays quiet, mirroring `string_len`.

/// Construct `*field = &str` borrowing `ptr[..len]` (a run INTO the reader's
/// input). Returns `false` when the bytes are not valid UTF-8 (the field is left
/// uninitialized then).
///
/// # Safety
/// `field` must be writable, uninitialized `&str` storage; `ptr[..len]` must be a
/// readable byte run that outlives the written `&str` (the zero-copy contract).
unsafe extern "C" fn str_set_borrowed(
    _ctx: *const (),
    field: *mut u8,
    ptr: *const u8,
    len: usize,
) -> bool {
    // Safety: `ptr[..len]` is a readable byte run.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    match core::str::from_utf8(bytes) {
        Ok(s) => {
            // Safety: `field` is uninitialized `&str` storage; the borrowed `&str`
            // points into the input (which the caller keeps alive).
            unsafe { core::ptr::write(field.cast::<&str>(), s) };
            true
        }
        Err(_) => false,
    }
}

/// The borrowed `&str`'s byte length.
///
/// # Safety
/// `field` points to an initialized `&str`.
unsafe extern "C" fn str_len(_ctx: *const (), field: *const u8) -> usize {
    // Safety: `field` is an initialized `&str`.
    (unsafe { &*field.cast::<&str>() }).len()
}

/// A pointer to the borrowed `&str`'s bytes.
///
/// # Safety
/// `field` points to an initialized `&str`.
unsafe extern "C" fn str_data(_ctx: *const (), field: *const u8) -> *const u8 {
    // Safety: `field` is an initialized `&str`.
    (unsafe { &*field.cast::<&str>() }).as_ptr()
}

// ----------------------------------------------------------------------------
// Borrowed `&[u8]` thunks (a raw fat pointer into the input, no validation).
// ----------------------------------------------------------------------------

/// Construct `*field = &[u8]` borrowing `ptr[..len]`. Always succeeds (any bytes
/// are valid `&[u8]`).
///
/// # Safety
/// `field` must be writable, uninitialized `&[u8]` storage; `ptr[..len]` must be a
/// readable byte run that outlives the written `&[u8]` (the zero-copy contract).
unsafe extern "C" fn bytes_set_borrowed(
    _ctx: *const (),
    field: *mut u8,
    ptr: *const u8,
    len: usize,
) -> bool {
    // Safety: `ptr[..len]` is a readable byte run.
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    // Safety: `field` is uninitialized `&[u8]` storage; the borrowed slice points
    // into the input (which the caller keeps alive).
    unsafe { core::ptr::write(field.cast::<&[u8]>(), slice) };
    true
}

/// The borrowed `&[u8]`'s length.
///
/// # Safety
/// `field` points to an initialized `&[u8]`.
unsafe extern "C" fn bytes_len(_ctx: *const (), field: *const u8) -> usize {
    // Safety: `field` is an initialized `&[u8]`.
    (unsafe { &*field.cast::<&[u8]>() }).len()
}

/// A pointer to the borrowed `&[u8]`'s bytes.
///
/// # Safety
/// `field` points to an initialized `&[u8]`.
unsafe extern "C" fn bytes_data(_ctx: *const (), field: *const u8) -> *const u8 {
    // Safety: `field` is an initialized `&[u8]`.
    (unsafe { &*field.cast::<&[u8]>() }).as_ptr()
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

// ============================================================================
// Field defaults (the reader-only-default compat path)
// ============================================================================
//
// A reader field marked `#[facet(default)]` (or `#[facet(default = expr)]`) can be
// filled in when the *writer* omitted it (`r[compat.reader-only-fields]`). facet
// exposes this two ways on a `Field`:
//   - `DefaultSource::FromTrait`: the field type's `Default` impl, reached through
//     the field's `&'static Shape` via `Shape::call_default_in_place`.
//   - `DefaultSource::Custom(DefaultInPlaceFn)`: a custom default expression.
// Either way we bind a `(ctx, thunk)` pair: the engine calls the ctx-less-looking
// `extern "C"` thunk, passing back the opaque `ctx` (the `&'static Shape` or the
// custom fn pointer) it does not interpret. A field with no `#[facet(default)]`
// yields `None` — and a reader-only field without a default makes the schemas
// incompatible.
//
// Spec: `r[descriptors.thunk-binding]`, `r[compat.reader-only-fields]`.

/// The bound default-in-place operation for a facet field, or `None` when the
/// field is not defaultable (no `#[facet(default)]`).
fn field_default(f: &'static facet::Field) -> Option<FieldDefault> {
    match f.default? {
        DefaultSource::FromTrait => {
            // The field type's `Default`, reached through its `&'static Shape`.
            // (If the type happens not to implement `Default`,
            // `call_default_in_place` returns `None` at run time and the thunk
            // panics — but `#[facet(default)]` without an expression only compiles
            // when the field type is `Default`, so this is unreachable in practice.)
            Some(FieldDefault {
                ctx: core::ptr::from_ref::<Shape>(f.shape()).cast::<()>(),
                thunk: default_from_shape,
            })
        }
        DefaultSource::Custom(custom) => Some(FieldDefault {
            // Carry the custom default fn pointer itself as the ctx.
            ctx: custom as *const (),
            thunk: default_from_custom,
        }),
        // `DefaultSource` is `#[non_exhaustive]`; an unrecognized source still means
        // the field is defaultable, so fall back to the field type's trait default.
        _ => Some(FieldDefault {
            ctx: core::ptr::from_ref::<Shape>(f.shape()).cast::<()>(),
            thunk: default_from_shape,
        }),
    }
}

/// [`DefaultThunk`] for `DefaultSource::FromTrait`: `ctx` is the field's
/// `&'static Shape`; write its type's `Default` value at `slot`.
///
/// # Safety
/// `ctx` must be the `&'static Shape` set by [`field_default`]; `slot` must be
/// uninitialized storage of that shape's size and alignment. On return `slot` holds
/// an initialized value.
unsafe extern "C" fn default_from_shape(ctx: *const (), slot: *mut u8) {
    // Safety: `ctx` is the `&'static Shape` set in `field_default`.
    let shape: &'static Shape = unsafe { &*ctx.cast::<Shape>() };
    // Safety: `slot` is uninitialized storage matching the shape.
    unsafe { shape.call_default_in_place(PtrUninit::new(slot)) }
        .expect("reader-only field marked #[facet(default)] has no default_in_place");
}

/// [`DefaultThunk`] for `DefaultSource::Custom`: `ctx` is the custom
/// [`DefaultInPlaceFn`](facet::DefaultInPlaceFn) pointer; call it to write the
/// custom default at `slot`.
///
/// # Safety
/// `ctx` must be the custom default fn pointer set by [`field_default`]; `slot`
/// must be uninitialized storage of the field type's size and alignment.
unsafe extern "C" fn default_from_custom(ctx: *const (), slot: *mut u8) {
    // Safety: `ctx` is the `DefaultInPlaceFn` set in `field_default`.
    let custom: facet::DefaultInPlaceFn =
        unsafe { core::mem::transmute::<*const (), facet::DefaultInPlaceFn>(ctx) };
    // Safety: `slot` is uninitialized storage for the field type.
    unsafe { custom(PtrUninit::new(slot)) };
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

    // ========================================================================
    // Borrowed / zero-copy leaves: `&str` and `&[u8]`
    // ========================================================================
    //
    // A `&str`/`&[u8]` field decodes by writing a fat pointer straight INTO the
    // input buffer — no allocation, no copy. The wire is byte-identical to the
    // owned `String`/`Vec<u8>` peer (same `u32` count + bytes), so a borrowed peer
    // interoperates with an owned one. These tests prove the wire identity, the
    // zero-copy (the decoded pointers point INTO the input), and the UTF-8
    // rejection — and (jit-gated) that NativeDecode agrees.

    // The borrowed test struct: a `&str`, a `&[u8]`, and a trailing scalar.
    #[derive(Facet)]
    struct Borrowed<'a> {
        name: &'a str,
        blob: &'a [u8],
        tag: u32,
    }

    // The owned equivalent — same field names, same wire. Used as the byte-identity
    // oracle: a borrowed peer's wire must equal an owned peer's wire exactly.
    #[derive(Facet)]
    struct OwnedEquiv {
        name: String,
        blob: Vec<u8>,
        tag: u32,
    }

    /// Build a `Borrowed` wire by encoding the owned equivalent (the wire is
    /// IDENTICAL, which `derived_borrowed_wire_matches_owned` asserts).
    fn borrowed_wire(name: &str, blob: &[u8], tag: u32) -> Vec<u8> {
        let d = of::<OwnedEquiv>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let v = OwnedEquiv { name: name.to_string(), blob: blob.to_vec(), tag };
        unsafe { typed::encode(core::ptr::from_ref(&v).cast::<u8>(), &d.descriptor, &reg) }.unwrap()
    }

    #[test]
    fn derived_borrowed_wire_matches_owned() {
        // The borrowed type encodes byte-for-byte like its owned peer (same schema
        // primitives, same wire), so they interoperate.
        let bd = of::<Borrowed>().unwrap();
        let od = of::<OwnedEquiv>().unwrap();

        // The borrowed leaves resolve to the SAME wire primitives as the owned peer:
        // `&str` field -> `Primitive::String`, `&[u8]` field -> `Primitive::Bytes`.
        // (The root struct ids differ only because the struct NAMES differ, which is
        // not on the wire — the leaf primitives, and thus the bytes, are identical.)
        let bfields = struct_fields(<Borrowed as Facet>::SHAPE).unwrap();
        let name_shape = bfields[0].shape();
        let blob_shape = bfields[1].shape();
        assert_eq!(
            borrow_primitive(borrowed_kind(name_shape).unwrap().unwrap()),
            Primitive::String,
        );
        assert_eq!(
            borrow_primitive(borrowed_kind(blob_shape).unwrap().unwrap()),
            Primitive::Bytes,
        );

        let breg = Registry::new(bd.schemas.clone());
        let oreg = Registry::new(od.schemas.clone());

        let name = "héllo wörld 🐝";
        let blob: &[u8] = &[0x00, 0xFF, 0x42, 0x99, 0xAB];
        let tag = 0xDEAD_BEEFu32;

        let b = Borrowed { name, blob, tag };
        let o = OwnedEquiv { name: name.to_string(), blob: blob.to_vec(), tag };

        let b_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&b).cast::<u8>(), &bd.descriptor, &breg) }
                .unwrap();
        let o_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&o).cast::<u8>(), &od.descriptor, &oreg) }
                .unwrap();
        assert_eq!(b_bytes, o_bytes, "borrowed wire != owned wire");
    }

    #[test]
    fn derived_borrowed_decode_is_zero_copy() {
        let d = of::<Borrowed>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        let name = "héllo wörld 🐝";
        let blob: &[u8] = &[0x00, 0xFF, 0x42, 0x99, 0xAB];
        let tag = 0x1234_5678u32;

        // Keep `wire` alive for the whole scope: the decoded `&str`/`&[u8]` borrow it.
        let wire = borrowed_wire(name, blob, tag);

        let mut slot = std::mem::MaybeUninit::<Borrowed>::uninit();
        unsafe { typed::decode(&wire, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
            .unwrap();
        let back = unsafe { slot.assume_init() };

        // Values match the originals.
        assert_eq!(back.name, name);
        assert_eq!(back.blob, blob);
        assert_eq!(back.tag, tag);

        // ZERO-COPY: the decoded pointers point INTO the `wire` buffer (not a fresh
        // allocation). The wire start + offset of each run lies within `wire`.
        let wire_start = wire.as_ptr() as usize;
        let wire_end = wire_start + wire.len();
        let name_ptr = back.name.as_ptr() as usize;
        let blob_ptr = back.blob.as_ptr() as usize;
        assert!(
            (wire_start..wire_end).contains(&name_ptr),
            "decoded &str does not point into the input buffer (not zero-copy)"
        );
        assert!(
            (wire_start..wire_end).contains(&blob_ptr),
            "decoded &[u8] does not point into the input buffer (not zero-copy)"
        );
        // `back` borrows `wire`; the borrow checker keeps `wire` alive through the
        // last use above (the zero-copy contract enforced statically).
    }

    #[test]
    fn derived_borrowed_rejects_invalid_utf8() {
        use phon_engine::CompactError;
        use phon_schema::DecodeError;

        let d = of::<Borrowed>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        // name = one byte 0xFF (not valid UTF-8). The `&str` construct thunk returns
        // false → InvalidUtf8.
        let mut wire = Vec::new();
        wire.extend_from_slice(&1u32.to_le_bytes()); // name length 1
        wire.push(0xFF); // invalid UTF-8
        // blob (Vec<u8>): count 0. Then the u32 tag at its alignment.
        wire.extend_from_slice(&0u32.to_le_bytes());
        phon_schema::bytes::pad_to(&mut wire, 4);
        wire.extend_from_slice(&0x42u32.to_le_bytes());

        let mut slot = std::mem::MaybeUninit::<Borrowed>::uninit();
        let err =
            unsafe { typed::decode(&wire, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
                .unwrap_err();
        assert!(matches!(err, CompactError::Decode(DecodeError::InvalidUtf8)), "got {err:?}");
    }

    #[test]
    fn derived_borrowed_slice_non_u8_is_unsupported() {
        // A borrowed slice of a non-`u8` element is out of scope (only `&str`/`&[u8]`
        // are the carried zero-copy leaves).
        #[derive(Facet)]
        struct BadBorrow<'a> {
            xs: &'a [u32],
        }
        assert!(matches!(
            of::<BadBorrow>(),
            Err(DeriveError::Unsupported("borrowed slice of non-u8 not supported yet"))
        ));
    }

    // The borrowed leaves through the *JIT*: derive -> lower -> NativeDecode. The
    // interpreter is the oracle. The decoded `&str`/`&[u8]` must still point INTO
    // the input buffer (zero-copy), and the JIT must reject invalid UTF-8.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_borrowed_jit_matches_interpreter_and_is_zero_copy() {
        use phon_jit::native::NativeDecode;

        let d = of::<Borrowed>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();

        let name = "JITful 🐝 héllo";
        let blob: &[u8] = &[0xAB, 0xCD, 0x00, 0xFF, 0x10, 0x20];
        let tag = 0x0BAD_F00Du32;
        let wire = borrowed_wire(name, blob, tag);

        // Interpreter (oracle).
        let mut islot = std::mem::MaybeUninit::<Borrowed>::uninit();
        unsafe { typed::decode_with(&program, &wire, islot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let iback = unsafe { islot.assume_init() };

        // JIT.
        let dec = NativeDecode::compile(&program);
        let mut jslot = std::mem::MaybeUninit::<Borrowed>::uninit();
        unsafe { dec.run(&wire, jslot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let jback = unsafe { jslot.assume_init() };

        assert_eq!(jback.name, name);
        assert_eq!(jback.blob, blob);
        assert_eq!(jback.tag, tag);
        assert_eq!((jback.name, jback.blob, jback.tag), (iback.name, iback.blob, iback.tag));

        // ZERO-COPY through the JIT: the decoded pointers point INTO `wire`.
        let wire_start = wire.as_ptr() as usize;
        let wire_end = wire_start + wire.len();
        assert!((wire_start..wire_end).contains(&(jback.name.as_ptr() as usize)));
        assert!((wire_start..wire_end).contains(&(jback.blob.as_ptr() as usize)));
        // Both decoded values borrow `wire`, kept alive through their last use above.
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_borrowed_jit_rejects_invalid_utf8() {
        use phon_jit::native::NativeDecode;
        use phon_schema::DecodeError;

        let d = of::<Borrowed>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();
        let dec = NativeDecode::compile(&program);

        // name = one byte 0xFF (invalid UTF-8), blob count 0, then tag.
        let mut wire = 1u32.to_le_bytes().to_vec();
        wire.push(0xFF);
        wire.extend_from_slice(&0u32.to_le_bytes());
        phon_schema::bytes::pad_to(&mut wire, 4);
        wire.extend_from_slice(&0x42u32.to_le_bytes());

        let mut slot = std::mem::MaybeUninit::<Borrowed>::uninit();
        let err = unsafe { dec.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err, DecodeError::InvalidUtf8), "got {err:?}");
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

    // A `#[repr(u8)]` enum root with all three payload shapes — the data-directed
    // variant branch. Enums are common as top-level RPC message types.
    #[repr(u8)]
    #[derive(Facet, Debug, PartialEq)]
    enum Msg {
        Ping,
        Echo(u32),
        Move { x: i32, y: i32 },
    }

    fn msg_value(m: &Msg) -> Value {
        let mut o = VObject::new();
        match m {
            Msg::Ping => {
                o.insert(VString::new("Ping"), Value::NULL);
            }
            Msg::Echo(v) => {
                o.insert(VString::new("Echo"), Value::from(*v));
            }
            Msg::Move { x, y } => {
                let mut inner = VObject::new();
                inner.insert(VString::new("x"), Value::from(*x));
                inner.insert(VString::new("y"), Value::from(*y));
                o.insert(VString::new("Move"), Value::from(inner));
            }
        }
        o.into()
    }

    #[test]
    fn derived_enum_matches_dynamic_and_roundtrips() {
        let d = of::<Msg>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        for m in [Msg::Ping, Msg::Echo(0xCAFE), Msg::Move { x: 3, y: -4 }] {
            let typed_bytes =
                unsafe { typed::encode(core::ptr::from_ref(&m).cast::<u8>(), &d.descriptor, &reg) }
                    .unwrap();

            let dyn_bytes = compact::to_bytes(&msg_value(&m), d.root, &reg).unwrap();
            assert_eq!(typed_bytes, dyn_bytes, "encode mismatch for {m:?}");

            let mut slot = std::mem::MaybeUninit::<Msg>::uninit();
            unsafe {
                typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>())
            }
            .unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back, m, "roundtrip mismatch");
        }
    }

    #[test]
    fn derived_enum_rejects_bad_variant_index() {
        use phon_engine::CompactError;
        let d = of::<Msg>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        // Wire variant index 99 — no such variant.
        let wire = 99u32.to_le_bytes().to_vec();
        let mut slot = std::mem::MaybeUninit::<Msg>::uninit();
        let err =
            unsafe { typed::decode(&wire, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
                .unwrap_err();
        assert!(matches!(err, CompactError::BadVariantIndex(99)));
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

    // The `Option<u32>` bridge through the *JIT*: derive -> lower ->
    // NativeEncode/Decode. JIT encode == interpreter encode (byte-identical), and
    // JIT decode round-trips, for both presence arms.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_option_u32_jit_matches_interpreter_and_roundtrips() {
        use phon_jit::native::{NativeDecode, NativeEncode};

        let d = of::<Maybe>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();
        let enc = NativeEncode::compile(&program);
        let dec = NativeDecode::compile(&program);

        for val in [Some(0xABCDu32), None, Some(0u32)] {
            let m = Maybe { val, tag: 0x77 };
            let base = core::ptr::from_ref(&m).cast::<u8>();

            let jit_bytes = unsafe { enc.run(base) };
            let interp_bytes =
                unsafe { typed::encode(base, &d.descriptor, &reg) }.unwrap();
            assert_eq!(jit_bytes, interp_bytes, "encode mismatch for {val:?}");

            let mut slot = std::mem::MaybeUninit::<Maybe>::uninit();
            unsafe { dec.run(&jit_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back.val, val, "roundtrip mismatch for {val:?}");
            assert_eq!(back.tag, m.tag);
        }
    }

    // `Option<String>` through the JIT: the some-arm builds a heap `String` into
    // the engine scratch buffer, then `init_some` moves it into the `Option`.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_option_string_jit_matches_interpreter_and_roundtrips() {
        use phon_jit::native::{NativeDecode, NativeEncode};

        let d = of::<MaybeStr>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();
        let enc = NativeEncode::compile(&program);
        let dec = NativeDecode::compile(&program);

        for s in [Some("héllo 🐝 wörld".to_string()), None, Some(String::new())] {
            let m = MaybeStr { s: s.clone(), n: 0x2A };
            let base = core::ptr::from_ref(&m).cast::<u8>();

            let jit_bytes = unsafe { enc.run(base) };
            let interp_bytes =
                unsafe { typed::encode(base, &d.descriptor, &reg) }.unwrap();
            assert_eq!(jit_bytes, interp_bytes, "encode mismatch for {s:?}");

            let mut slot = std::mem::MaybeUninit::<MaybeStr>::uninit();
            unsafe { dec.run(&jit_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back.s, s, "roundtrip mismatch for {s:?}");
            assert_eq!(back.n, m.n);
        }
    }

    // The `#[repr(u8)]` enum bridge through the JIT, all three variant shapes
    // (unit, scalar payload, struct payload): JIT encode == interpreter encode and
    // JIT decode round-trips.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_enum_jit_matches_interpreter_and_roundtrips() {
        use phon_jit::native::{NativeDecode, NativeEncode};

        let d = of::<Msg>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();
        let enc = NativeEncode::compile(&program);
        let dec = NativeDecode::compile(&program);

        for m in [Msg::Ping, Msg::Echo(0xCAFE), Msg::Move { x: 3, y: -4 }] {
            let base = core::ptr::from_ref(&m).cast::<u8>();

            let jit_bytes = unsafe { enc.run(base) };
            let interp_bytes =
                unsafe { typed::encode(base, &d.descriptor, &reg) }.unwrap();
            assert_eq!(jit_bytes, interp_bytes, "encode mismatch for {m:?}");

            let mut slot = std::mem::MaybeUninit::<Msg>::uninit();
            unsafe { dec.run(&jit_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let back = unsafe { slot.assume_init() };
            assert_eq!(back, m, "roundtrip mismatch for {m:?}");
        }
    }

    // The JIT must REJECT a hostile enum wire index, never produce a value.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_enum_jit_rejects_bad_variant_index() {
        use phon_jit::native::NativeDecode;
        use phon_schema::DecodeError;

        let d = of::<Msg>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();
        let dec = NativeDecode::compile(&program);

        // Wire variant index 99 — no such variant, and not writer-only either.
        let wire = 99u32.to_le_bytes().to_vec();
        let mut slot = std::mem::MaybeUninit::<Msg>::uninit();
        let err = unsafe { dec.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        // A garbage index → BadVariantIndex (the JIT now distinguishes it from a
        // writer-only variant), carrying the index — the `DecodeError`-channel
        // counterpart of the interpreter's `CompactError::BadVariantIndex`.
        assert!(matches!(err, DecodeError::BadVariantIndex(99)), "got {err:?}");
    }

    // The JIT must REJECT a hostile `Option` presence byte, never produce a value.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn derived_option_jit_rejects_invalid_presence() {
        use phon_jit::native::NativeDecode;
        use phon_schema::DecodeError;

        let d = of::<Maybe>().unwrap();
        let reg = Registry::new(d.schemas.clone());
        let program = typed::lower(&d.descriptor, &reg).unwrap();
        let dec = NativeDecode::compile(&program);

        // presence byte 2 (neither 0 nor 1) — the JIT carries it into InvalidBool.
        let wire = vec![2u8];
        let mut slot = std::mem::MaybeUninit::<Maybe>::uninit();
        let err = unsafe { dec.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(matches!(err, DecodeError::InvalidBool(2)), "got {err:?}");
    }

    // A struct with a `BTreeMap<String, u32>` field: the data-directed map count
    // loop, a scalar value, and a string key. `BTreeMap` is sorted, so its
    // iteration order is deterministic; building the oracle `VObject` by inserting
    // keys in that same sorted order makes both codecs iterate identically (a
    // `VObject` is insertion-ordered, and the dynamic codec is string-keyed).
    #[derive(Facet)]
    struct MapHolder {
        m: std::collections::BTreeMap<String, u32>,
        tag: u32,
    }

    #[test]
    fn derived_map_string_u32_matches_dynamic_and_roundtrips() {
        let d = of::<MapHolder>().unwrap();
        // Two composite schemas: MapHolder (struct) and BTreeMap<String, u32> (map).
        assert_eq!(d.schemas.len(), 2);
        let reg = Registry::new(d.schemas.clone());

        let mut m = std::collections::BTreeMap::new();
        m.insert("alpha".to_string(), 1u32);
        m.insert("beta".to_string(), 0xCAFEu32);
        m.insert("gamma".to_string(), 0xDEAD_BEEFu32);
        let h = MapHolder { m: m.clone(), tag: 0x55 };

        let typed_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&h).cast::<u8>(), &d.descriptor, &reg) }
                .unwrap();

        // Oracle: byte-identical to the dynamic codec for the equivalent object.
        // BTreeMap iterates in sorted key order, so insert into the VObject sorted.
        let mut mobj = VObject::new();
        for (k, v) in &m {
            mobj.insert(VString::new(k.as_str()), Value::from(*v));
        }
        let mut obj = VObject::new();
        obj.insert(VString::new("m"), Value::from(mobj));
        obj.insert(VString::new("tag"), Value::from(h.tag));
        let dyn_bytes = compact::to_bytes(&obj.into(), d.root, &reg).unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        // Round-trip: decode back into a MapHolder and check the map and scalar.
        let mut slot = std::mem::MaybeUninit::<MapHolder>::uninit();
        unsafe { typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
            .unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.m, m);
        assert_eq!(back.tag, h.tag);
    }

    // A `BTreeMap<String, String>`: a heap value, exercising the value scratch-move
    // (the value `String` is decoded into engine scratch, then `insert` moves it
    // into the map and the scratch is freed without dropping).
    #[derive(Facet)]
    struct StrMapHolder {
        m: std::collections::BTreeMap<String, String>,
    }

    #[test]
    fn derived_map_string_string_matches_dynamic_and_roundtrips() {
        let d = of::<StrMapHolder>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        let mut m = std::collections::BTreeMap::new();
        m.insert("name".to_string(), "héllo 🐝".to_string());
        m.insert("other".to_string(), "wörld".to_string());
        m.insert("zed".to_string(), String::new());
        let h = StrMapHolder { m: m.clone() };

        let typed_bytes =
            unsafe { typed::encode(core::ptr::from_ref(&h).cast::<u8>(), &d.descriptor, &reg) }
                .unwrap();

        let mut mobj = VObject::new();
        for (k, v) in &m {
            mobj.insert(VString::new(k.as_str()), Value::from(v.as_str()));
        }
        let mut obj = VObject::new();
        obj.insert(VString::new("m"), Value::from(mobj));
        let dyn_bytes = compact::to_bytes(&obj.into(), d.root, &reg).unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        let mut slot = std::mem::MaybeUninit::<StrMapHolder>::uninit();
        unsafe { typed::decode(&typed_bytes, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
            .unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.m, m);
    }

    // A wire with a repeated key collapses two entries into one entry in the map;
    // the typed decode must reject it (count mismatch), matching the dynamic
    // codec's `DuplicateKey` rejection (the oracle).
    #[test]
    fn derived_map_rejects_duplicate_key() {
        use phon_engine::CompactError;
        use phon_schema::DecodeError;
        use phon_schema::bytes::{pad_to, write_u32};

        let d = of::<MapHolder>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        // Hand-build the `m` field's wire: count 2, then the SAME key "k" twice
        // (each with its own u32 value), then the trailing `tag` u32. The two
        // entries collapse to one in the BTreeMap, so len(1) != count(2).
        let mut wire = Vec::new();
        write_u32(&mut wire, 2); // map entry count
        for val in [10u32, 20u32] {
            // key "k": a String (u32 length then bytes).
            write_u32(&mut wire, 1);
            wire.push(b'k');
            // value u32 at its alignment.
            pad_to(&mut wire, 4);
            write_u32(&mut wire, val);
        }
        // trailing struct field `tag` (u32) at its alignment.
        pad_to(&mut wire, 4);
        write_u32(&mut wire, 0x77);

        let mut slot = std::mem::MaybeUninit::<MapHolder>::uninit();
        let err =
            unsafe { typed::decode(&wire, &d.descriptor, &reg, slot.as_mut_ptr().cast::<u8>()) }
                .unwrap_err();
        assert!(
            matches!(err, CompactError::Decode(DecodeError::DuplicateKey)),
            "got {err:?}"
        );

        // The engine documents that a mid-decode error leaks the partial value (the
        // same trivially-droppable limitation as sequences/options): on the
        // `DuplicateKey` path the `m` field was fully initialized (the duplicate
        // collapsed into one entry) before the error, while `tag` stayed
        // uninitialized. Reclaim just that field here so the leak the engine leaves
        // behind does not trip Miri's leak checker — this only frees what we know is
        // initialized; it does not change the engine's behavior.
        let m_field = std::mem::offset_of!(MapHolder, m);
        // Safety: on `DuplicateKey` the map field is an initialized
        // `BTreeMap<String, u32>`; we read it out and drop it exactly once.
        let partial = unsafe {
            core::ptr::read(slot.as_ptr().cast::<u8>().add(m_field).cast::<
                std::collections::BTreeMap<String, u32>,
            >())
        };
        assert_eq!(partial.len(), 1);
        drop(partial);
    }

    // ========================================================================
    // Writer ↔ reader schema compatibility (the typed decode-compat path)
    // ========================================================================
    //
    // Each test derives TWO `#[derive(Facet)]` types — a writer and a reader —
    // merges both schema batches into one `Registry`, encodes a writer value (typed
    // encode of the writer type), then `lower_decode(writer.root, &reader.descriptor,
    // reg)` + `decode_with` into reader memory. Where practical the dynamic
    // `plan::decode` over the same bytes is asserted as a cross-engine oracle.
    //
    // These reproduce `plan.rs`'s six drift cases (`r[compat.*]`) on the MEMORY side.

    use phon_engine::{plan, typed::lower_decode};

    /// Merge two derived schema batches into one registry. Both carry real
    /// content-derived ids, so identical sub-schemas dedup and distinct ones coexist.
    fn merged_registry(a: &Derived, b: &Derived) -> Registry {
        let mut schemas = a.schemas.clone();
        for s in &b.schemas {
            if !schemas.iter().any(|x| x.id == s.id) {
                schemas.push(s.clone());
            }
        }
        Registry::new(schemas)
    }

    /// Typed-encode a writer value into its compact wire bytes.
    fn encode_writer<'a, W: Facet<'a>>(w: &W, writer: &Derived, reg: &Registry) -> Vec<u8> {
        unsafe { typed::encode(core::ptr::from_ref(w).cast::<u8>(), &writer.descriptor, reg) }
            .unwrap()
    }

    // ---- 1. Field reorder is transparent --------------------------------------

    #[derive(Facet)]
    struct ReorderW {
        a: u32,
        b: u32,
    }
    #[derive(Facet)]
    struct ReorderR {
        b: u32,
        a: u32,
    }

    #[test]
    fn compat_field_reorder_is_transparent() {
        let w = of::<ReorderW>().unwrap();
        let r = of::<ReorderR>().unwrap();
        let reg = merged_registry(&w, &r);

        let value = ReorderW { a: 7, b: 9 };
        let bytes = encode_writer(&value, &w, &reg);

        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        let mut slot = std::mem::MaybeUninit::<ReorderR>::uninit();
        unsafe { typed::decode_with(&program, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.a, 7);
        assert_eq!(back.b, 9);

        // Cross-engine oracle: the dynamic plan decodes the same bytes to the
        // equivalent reader-shaped object.
        let got = plan::decode(&bytes, w.root, r.root, &reg).unwrap();
        let mut want = VObject::new();
        want.insert(VString::new("b"), Value::from(9u32));
        want.insert(VString::new("a"), Value::from(7u32));
        assert_eq!(got, Value::from(want));
    }

    // ---- 2. Writer-only field is skipped --------------------------------------

    #[derive(Facet)]
    struct SkipW {
        a: u32,
        b: String,
        c: u32,
    }
    #[derive(Facet)]
    struct SkipR {
        a: u32,
        c: u32,
    }

    #[test]
    fn compat_writer_only_field_is_skipped() {
        let w = of::<SkipW>().unwrap();
        let r = of::<SkipR>().unwrap();
        let reg = merged_registry(&w, &r);

        let value = SkipW {
            a: 11,
            b: "discard me".to_string(),
            c: 22,
        };
        let bytes = encode_writer(&value, &w, &reg);

        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        let mut slot = std::mem::MaybeUninit::<SkipR>::uninit();
        unsafe { typed::decode_with(&program, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.a, 11);
        assert_eq!(back.c, 22);

        // Oracle.
        let got = plan::decode(&bytes, w.root, r.root, &reg).unwrap();
        let mut want = VObject::new();
        want.insert(VString::new("a"), Value::from(11u32));
        want.insert(VString::new("c"), Value::from(22u32));
        assert_eq!(got, Value::from(want));
    }

    // ---- 3. Reader-only field defaulted (or required → incompatible) ----------

    #[derive(Facet)]
    struct DefaultW {
        a: u32,
    }
    #[derive(Facet)]
    struct DefaultR {
        a: u32,
        #[facet(default)]
        b: u32,
    }
    // A reader with a NON-defaulted reader-only field: incompatible.
    #[derive(Facet)]
    struct RequiredR {
        a: u32,
        extra: u32,
    }

    #[test]
    fn compat_reader_only_field_defaults() {
        let w = of::<DefaultW>().unwrap();
        let r = of::<DefaultR>().unwrap();
        let reg = merged_registry(&w, &r);

        let value = DefaultW { a: 7 };
        let bytes = encode_writer(&value, &w, &reg);

        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        let mut slot = std::mem::MaybeUninit::<DefaultR>::uninit();
        unsafe { typed::decode_with(&program, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.a, 7);
        assert_eq!(back.b, u32::default()); // 0 — the field's #[facet(default)]
    }

    #[test]
    fn compat_reader_only_field_with_custom_default() {
        // A custom default expression on the reader-only field.
        #[derive(Facet)]
        struct CustomR {
            a: u32,
            #[facet(default = 0xABCD)]
            b: u32,
        }
        let w = of::<DefaultW>().unwrap();
        let r = of::<CustomR>().unwrap();
        let reg = merged_registry(&w, &r);

        let bytes = encode_writer(&DefaultW { a: 7 }, &w, &reg);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        let mut slot = std::mem::MaybeUninit::<CustomR>::uninit();
        unsafe { typed::decode_with(&program, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.a, 7);
        assert_eq!(back.b, 0xABCD);
    }

    #[test]
    fn compat_reader_only_required_field_is_incompatible() {
        use phon_engine::CompactError;
        let w = of::<DefaultW>().unwrap();
        let r = of::<RequiredR>().unwrap();
        let reg = merged_registry(&w, &r);

        assert!(matches!(
            lower_decode(w.root, &r.descriptor, &reg),
            Err(CompactError::Incompatible(_))
        ));
        // Oracle: the dynamic plan is equally incompatible.
        assert!(matches!(
            plan::build_plan(w.root, r.root, &reg),
            Err(CompactError::Incompatible(_))
        ));
    }

    // ---- 4. Enum variant added / removed --------------------------------------

    #[repr(u8)]
    #[derive(Facet, Debug, PartialEq)]
    enum EnumW {
        A,
        B(u32),
    }
    // Reader with an ADDED variant C: reads A and B fine.
    #[repr(u8)]
    #[derive(Facet, Debug, PartialEq)]
    enum EnumRMore {
        A,
        B(u32),
        C,
    }
    // Reader that LACKS B: receiving B is a writer-only-variant error.
    #[repr(u8)]
    #[derive(Facet, Debug, PartialEq)]
    enum EnumRFewer {
        A,
    }

    #[test]
    fn compat_enum_variant_added_reads_fine() {
        let w = of::<EnumW>().unwrap();
        let r = of::<EnumRMore>().unwrap();
        let reg = merged_registry(&w, &r);

        let bytes = encode_writer(&EnumW::B(42), &w, &reg);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        let mut slot = std::mem::MaybeUninit::<EnumRMore>::uninit();
        unsafe { typed::decode_with(&program, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, EnumRMore::B(42));

        // The unit variant A round-trips too.
        let a_bytes = encode_writer(&EnumW::A, &w, &reg);
        let mut slot = std::mem::MaybeUninit::<EnumRMore>::uninit();
        unsafe { typed::decode_with(&program, &a_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        assert_eq!(unsafe { slot.assume_init() }, EnumRMore::A);
    }

    #[test]
    fn compat_enum_variant_removed_rejects() {
        use phon_engine::CompactError;
        let w = of::<EnumW>().unwrap();
        let r = of::<EnumRFewer>().unwrap();
        let reg = merged_registry(&w, &r);

        // The plan builds (A matches), but receiving B is a writer-only-variant error.
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        let b_bytes = encode_writer(&EnumW::B(42), &w, &reg);
        let mut slot = std::mem::MaybeUninit::<EnumRFewer>::uninit();
        let err =
            unsafe { typed::decode_with(&program, &b_bytes, slot.as_mut_ptr().cast::<u8>()) }
                .unwrap_err();
        // The writer index of B is 1.
        assert!(matches!(err, CompactError::WriterOnlyVariant(1)), "got {err:?}");

        // Oracle: the dynamic plan reports the same writer-only variant.
        assert!(matches!(
            plan::decode(&b_bytes, w.root, r.root, &reg),
            Err(CompactError::WriterOnlyVariant(1))
        ));

        // An A value still decodes against the fewer-variant reader.
        let a_bytes = encode_writer(&EnumW::A, &w, &reg);
        let mut slot = std::mem::MaybeUninit::<EnumRFewer>::uninit();
        unsafe { typed::decode_with(&program, &a_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        assert_eq!(unsafe { slot.assume_init() }, EnumRFewer::A);
    }

    // ---- 5. No implicit numeric widening --------------------------------------

    #[derive(Facet)]
    struct WideW {
        n: u32,
    }
    #[derive(Facet)]
    struct WideR {
        n: u64,
    }

    #[test]
    fn compat_no_implicit_widening() {
        use phon_engine::CompactError;
        let w = of::<WideW>().unwrap();
        let r = of::<WideR>().unwrap();
        let reg = merged_registry(&w, &r);

        assert!(matches!(
            lower_decode(w.root, &r.descriptor, &reg),
            Err(CompactError::Incompatible(_))
        ));
        // Oracle.
        assert!(matches!(
            plan::build_plan(w.root, r.root, &reg),
            Err(CompactError::Incompatible(_))
        ));
    }

    // ---- 6. Nested struct drift -----------------------------------------------

    #[derive(Facet)]
    struct InnerW {
        a: u32,
    }
    #[derive(Facet)]
    struct InnerR {
        a: u32,
        #[facet(default)]
        b: bool,
    }
    #[derive(Facet)]
    struct OuterW {
        inner: InnerW,
        tag: u32,
    }
    #[derive(Facet)]
    struct OuterR {
        inner: InnerR,
        tag: u32,
    }

    #[test]
    fn compat_nested_struct_drift() {
        let w = of::<OuterW>().unwrap();
        let r = of::<OuterR>().unwrap();
        let reg = merged_registry(&w, &r);

        let value = OuterW {
            inner: InnerW { a: 5 },
            tag: 0x99,
        };
        let bytes = encode_writer(&value, &w, &reg);

        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        let mut slot = std::mem::MaybeUninit::<OuterR>::uninit();
        unsafe { typed::decode_with(&program, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.inner.a, 5);
        assert!(!back.inner.b, "reader-only bool field defaults to false");
        assert_eq!(back.tag, 0x99);

        // Oracle: the dynamic plan decodes to the equivalent nested object.
        let got = plan::decode(&bytes, w.root, r.root, &reg).unwrap();
        let mut inner = VObject::new();
        inner.insert(VString::new("a"), Value::from(5u32));
        inner.insert(VString::new("b"), Value::NULL); // dynamic default is null
        let mut outer = VObject::new();
        outer.insert(VString::new("inner"), Value::from(inner));
        outer.insert(VString::new("tag"), Value::from(0x99u32));
        assert_eq!(got, Value::from(outer));
    }

    // ---- Identity: writer == reader yields a skip/default-free program --------

    #[test]
    fn compat_identity_matches_single_schema_lower() {
        // When the writer schema IS the reader schema, lower_decode must produce a
        // program equivalent to the single-schema typed `lower` (no skips/defaults).
        let d = of::<Pt>().unwrap();
        let reg = Registry::new(d.schemas.clone());

        let single = typed::lower(&d.descriptor, &reg).unwrap();
        let compat = lower_decode(d.root, &d.descriptor, &reg).unwrap();

        // No compat-only ops appear, and the op sequence matches the single-schema
        // lowering byte-for-byte (Debug equality on the IR).
        assert!(!has_compat_ops(&compat), "identity program leaked skip/default ops");
        assert_eq!(format!("{single:?}"), format!("{compat:?}"));

        // And it actually round-trips a real value.
        let p = Pt { a: 0x11, b: 0x2222_2222_2222_2222, c: 0x3333, flag: true };
        let bytes =
            unsafe { typed::encode(core::ptr::from_ref(&p).cast::<u8>(), &d.descriptor, &reg) }
                .unwrap();
        let mut slot = std::mem::MaybeUninit::<Pt>::uninit();
        unsafe { typed::decode_with(&compat, &bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back.a, p.a);
        assert_eq!(back.b, p.b);
        assert_eq!(back.c, p.c);
        assert_eq!(back.flag, p.flag);
    }

    fn has_compat_ops(program: &[phon_ir::MemOp]) -> bool {
        use phon_ir::MemOp;
        program.iter().any(|op| match op {
            MemOp::SkipWire(_) | MemOp::Default(_) => true,
            MemOp::Sequence(s) => has_compat_ops(&s.element),
            MemOp::Option(o) => has_compat_ops(&o.some),
            MemOp::Map(m) => has_compat_ops(&m.key) || has_compat_ops(&m.value),
            MemOp::Enum(e) => e.variants.iter().any(|v| has_compat_ops(&v.payload)),
            _ => false,
        })
    }

    // ========================================================================
    // Writer ↔ reader compatibility through the copy-and-patch JIT
    // ========================================================================
    //
    // The same drift cases as the interpreter `compat_*` tests above, but the
    // reconciliation program runs through `NativeDecode` (the copy-and-patch JIT)
    // rather than the threaded interpreter. The principle: compatibility is resolved
    // entirely at lowering — `lower_decode` bakes in the skips, defaults, reorders,
    // and remaps — so the JIT just compiles that program and decode runs at full
    // speed regardless of schema drift. The interpreter (`typed::decode_with`) over
    // the SAME program is the oracle: the two engines must agree byte-for-byte.
    //
    // These exercise the `MemOp::SkipWire` stencil (writer-only fields) and the
    // `MemOp::Default` stencil (reader-only `#[facet(default)]` fields) added to the
    // JIT here, plus reorder / enum add+remove / nested struct drift.

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    use phon_jit::native::NativeDecode;

    /// Decode `bytes` against `program` with BOTH engines into separate reader-typed
    /// slots; returns `(jit, interp)`. The two must be field-equal — the interpreter
    /// is the oracle for the JIT.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn decode_both<R>(program: &phon_ir::MemProgram, bytes: &[u8]) -> (R, R) {
        let jit = NativeDecode::compile(program);
        let mut jit_slot = std::mem::MaybeUninit::<R>::uninit();
        unsafe { jit.run(bytes, jit_slot.as_mut_ptr().cast::<u8>()) }.unwrap();

        let mut interp_slot = std::mem::MaybeUninit::<R>::uninit();
        unsafe { typed::decode_with(program, bytes, interp_slot.as_mut_ptr().cast::<u8>()) }
            .unwrap();

        (unsafe { jit_slot.assume_init() }, unsafe { interp_slot.assume_init() })
    }

    /// 1. Field reorder: the JIT decodes reordered scalars into the reader's layout,
    ///    agreeing with the interpreter and the known values.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn compat_jit_field_reorder_is_transparent() {
        let w = of::<ReorderW>().unwrap();
        let r = of::<ReorderR>().unwrap();
        let reg = merged_registry(&w, &r);

        let bytes = encode_writer(&ReorderW { a: 7, b: 9 }, &w, &reg);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();

        let (jit, interp) = decode_both::<ReorderR>(&program, &bytes);
        assert_eq!(jit.a, 7);
        assert_eq!(jit.b, 9);
        assert_eq!((jit.a, jit.b), (interp.a, interp.b));
    }

    /// 2. Writer-only field: the JIT runs the `MemOp::SkipWire` stencil to consume
    ///    the writer's `String` field, writing nothing for it, and agrees with the
    ///    interpreter.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn compat_jit_writer_only_field_is_skipped() {
        let w = of::<SkipW>().unwrap();
        let r = of::<SkipR>().unwrap();
        let reg = merged_registry(&w, &r);

        let value = SkipW { a: 11, b: "discard me".to_string(), c: 22 };
        let bytes = encode_writer(&value, &w, &reg);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        assert!(has_compat_ops(&program), "expected a SkipWire op in the program");

        let (jit, interp) = decode_both::<SkipR>(&program, &bytes);
        assert_eq!(jit.a, 11);
        assert_eq!(jit.c, 22);
        assert_eq!((jit.a, jit.c), (interp.a, interp.c));
    }

    /// 3a. Reader-only `#[facet(default)]` field: the JIT runs the `MemOp::Default`
    ///     stencil (calling the field's default thunk, no wire read) and agrees with
    ///     the interpreter.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn compat_jit_reader_only_field_defaults() {
        let w = of::<DefaultW>().unwrap();
        let r = of::<DefaultR>().unwrap();
        let reg = merged_registry(&w, &r);

        let bytes = encode_writer(&DefaultW { a: 7 }, &w, &reg);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        assert!(has_compat_ops(&program), "expected a Default op in the program");

        let (jit, interp) = decode_both::<DefaultR>(&program, &bytes);
        assert_eq!(jit.a, 7);
        assert_eq!(jit.b, u32::default()); // 0 — the field's #[facet(default)]
        assert_eq!((jit.a, jit.b), (interp.a, interp.b));
    }

    /// 3b. A custom default expression on the reader-only field: the `MemOp::Default`
    ///     thunk writes the custom value (0xABCD), matching the interpreter.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn compat_jit_reader_only_field_with_custom_default() {
        #[derive(Facet)]
        struct CustomR {
            a: u32,
            #[facet(default = 0xABCD)]
            b: u32,
        }
        let w = of::<DefaultW>().unwrap();
        let r = of::<CustomR>().unwrap();
        let reg = merged_registry(&w, &r);

        let bytes = encode_writer(&DefaultW { a: 7 }, &w, &reg);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        assert!(has_compat_ops(&program), "expected a Default op in the program");

        let (jit, interp) = decode_both::<CustomR>(&program, &bytes);
        assert_eq!(jit.a, 7);
        assert_eq!(jit.b, 0xABCD);
        assert_eq!((jit.a, jit.b), (interp.a, interp.b));
    }

    /// 4a. Enum variant added on the reader: the JIT decodes the writer's A and
    ///     B(42) into the wider reader enum, agreeing with the interpreter (compared
    ///     directly via the reader enum's `PartialEq`).
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn compat_jit_enum_variant_added_reads_fine() {
        let w = of::<EnumW>().unwrap();
        let r = of::<EnumRMore>().unwrap();
        let reg = merged_registry(&w, &r);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();

        for (val, want) in [
            (EnumW::B(42), EnumRMore::B(42)),
            (EnumW::A, EnumRMore::A),
        ] {
            let bytes = encode_writer(&val, &w, &reg);
            let (jit, interp) = decode_both::<EnumRMore>(&program, &bytes);
            assert_eq!(jit, want);
            assert_eq!(jit, interp);
        }
    }

    /// 4b. Enum variant removed on the reader: receiving the writer-only variant B
    ///     is a decode error in the JIT (an unmatched wire index, since the JIT enum
    ///     stencil carries no matching variant), while the surviving variant A still
    ///     decodes and agrees with the interpreter.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn compat_jit_enum_variant_removed_rejects_b_reads_a() {
        let w = of::<EnumW>().unwrap();
        let r = of::<EnumRFewer>().unwrap();
        let reg = merged_registry(&w, &r);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();

        // The writer-only variant B is rejected by the JIT — now distinctly, as
        // WriterOnlyVariant (the `DecodeError`-channel counterpart of the
        // interpreter's `CompactError::WriterOnlyVariant`), not a generic error.
        let b_bytes = encode_writer(&EnumW::B(42), &w, &reg);
        let jit = NativeDecode::compile(&program);
        let mut slot = std::mem::MaybeUninit::<EnumRFewer>::uninit();
        let err = unsafe { jit.run(&b_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap_err();
        assert!(
            matches!(err, phon_schema::DecodeError::WriterOnlyVariant(1)),
            "got {err:?}"
        );

        // A still decodes, agreeing with the interpreter.
        let a_bytes = encode_writer(&EnumW::A, &w, &reg);
        let (jit_a, interp_a) = decode_both::<EnumRFewer>(&program, &a_bytes);
        assert_eq!(jit_a, EnumRFewer::A);
        assert_eq!(jit_a, interp_a);
    }

    /// 5. Nested struct drift: the inner struct gains a reader-only `#[facet(default)]`
    ///    `bool` field. The JIT runs the nested `MemOp::Default` stencil and agrees
    ///    with the interpreter on every field.
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn compat_jit_nested_struct_drift() {
        let w = of::<OuterW>().unwrap();
        let r = of::<OuterR>().unwrap();
        let reg = merged_registry(&w, &r);

        let value = OuterW { inner: InnerW { a: 5 }, tag: 0x99 };
        let bytes = encode_writer(&value, &w, &reg);
        let program = lower_decode(w.root, &r.descriptor, &reg).unwrap();
        assert!(has_compat_ops(&program), "expected a nested Default op in the program");

        let (jit, interp) = decode_both::<OuterR>(&program, &bytes);
        assert_eq!(jit.inner.a, 5);
        assert!(!jit.inner.b, "reader-only bool field defaults to false");
        assert_eq!(jit.tag, 0x99);
        assert_eq!(
            (jit.inner.a, jit.inner.b, jit.tag),
            (interp.inner.a, interp.inner.b, interp.tag),
        );
    }
}
