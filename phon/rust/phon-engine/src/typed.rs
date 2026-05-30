//! The typed path: lower a [`Descriptor`] (which carries its schema) into a flat
//! [`MemProgram`], then run it to encode or decode a value living in this
//! process's memory.
//!
//! This is the memory counterpart to the dynamic [`Value`](facet_value::Value)
//! path. The split is phon's schema+descriptor pairing: the **schema** (resolved
//! through the registry) decides the wire bytes and their order; the
//! **descriptor** says where each field lives in memory. Because the wire is
//! schema-driven, a typed value produces byte-identical output to the dynamic
//! codec for the same logical value — that equivalence is the oracle the tests
//! check, and it's what lets a typed peer and a dynamic peer interoperate.
//!
//! Lowering walks the descriptor once and folds every field offset to be
//! relative to the value's base pointer. A nested fixed struct therefore
//! dissolves into a single straight run of scalar copies (`r[ir.inlining]`,
//! `r[ir.memory]`) — no per-decode descriptor walk, no branches. Owned
//! sequences, options, enums (allocation and run-time branching) come next.
//!
//! First cut: fixed-width scalars and in-place records (struct/tuple). Anything
//! else lowers to [`CompactError::Unsupported`].
//!
//! Spec: "The descriptor model", "Compact mode", `r[ir.memory]`.

use phon_ir::ir::{MemOp, MemProgram};
use phon_ir::{Access, Construct, Descriptor};
use phon_schema::bytes::Reader;
use phon_schema::{DecodeError, Primitive, SchemaKind};

use crate::compact::{self, CompactError, Registry, Resolved, alignment, pad_to, skip_pad};

type Result<T> = core::result::Result<T, CompactError>;

/// The wire (and, for our targets, in-memory) size of a fixed-width scalar, or
/// `None` for the variable-length and uninhabited primitives, which need
/// allocation or are never values and so are not plain copies.
fn fixed_size(p: Primitive) -> Option<usize> {
    Some(match p {
        Primitive::Unit => 0,
        Primitive::Bool | Primitive::U8 | Primitive::I8 => 1,
        Primitive::U16 | Primitive::I16 => 2,
        Primitive::U32 | Primitive::I32 | Primitive::F32 | Primitive::Char => 4,
        Primitive::U64 | Primitive::I64 | Primitive::F64 => 8,
        Primitive::U128 | Primitive::I128 => 16,
        Primitive::String
        | Primitive::Bytes
        | Primitive::Never
        | Primitive::DateTime
        | Primitive::Uuid
        | Primitive::QName => return None,
    })
}

// ============================================================================
// Lowering
// ============================================================================

/// Lower a descriptor into a flat [`MemProgram`]: a list of base-relative memory
/// copies, in wire order. Build it once, run it many times.
///
/// # Errors
/// [`CompactError`] if a referenced schema is missing, the descriptor and schema
/// disagree, or a kind this first cut does not handle is reached.
// r[impl ir.memory]
pub fn lower(descriptor: &Descriptor, reg: &Registry) -> Result<MemProgram> {
    let mut out = Vec::new();
    lower_node(descriptor, reg, 0, &mut out)?;
    Ok(out)
}

// r[impl ir.inlining]
fn lower_node(d: &Descriptor, reg: &Registry, base: usize, out: &mut MemProgram) -> Result<()> {
    match (&d.access, compact::resolve(reg, &d.schema)?) {
        (Access::Scalar, Resolved::Primitive(p)) => {
            let size = fixed_size(p)
                .ok_or(CompactError::Unsupported("typed: variable-length scalar field"))?;
            out.push(MemOp::Scalar {
                offset: base,
                size,
                align: alignment(p),
            });
            Ok(())
        }
        (Access::Record(ra), Resolved::Composite(kind)) => {
            let arity = match &kind {
                SchemaKind::Struct { fields, .. } => fields.len(),
                SchemaKind::Tuple { elements } => elements.len(),
                _ => {
                    return Err(CompactError::TypeMismatch {
                        expected: "struct or tuple for a record descriptor",
                    });
                }
            };
            if arity != ra.fields.len() {
                return Err(CompactError::Malformed("descriptor/schema field count mismatch"));
            }
            match &ra.construct {
                Construct::InPlace => {}
                Construct::Thunk(_) => {
                    return Err(CompactError::Unsupported("typed: thunk construction"));
                }
            }
            // Splice each field in wire order, folding its memory offset into the
            // base. A field's own descriptor carries its schema and layout.
            for fa in &ra.fields {
                lower_node(&fa.descriptor, reg, base + fa.offset, out)?;
            }
            Ok(())
        }
        _ => Err(CompactError::Unsupported(
            "typed: only fixed scalars and in-place records so far",
        )),
    }
}

// ============================================================================
// Encode
// ============================================================================

/// Encode the value at `base` into compact bytes, by a prebuilt program.
///
/// # Safety
/// `base` must point to an initialized value matching the descriptor the program
/// was lowered from, readable for every `offset + size` the program touches.
#[must_use]
pub unsafe fn encode_with(program: &MemProgram, base: *const u8) -> Vec<u8> {
    let mut out = Vec::new();
    for op in program {
        match op {
            MemOp::Scalar { offset, size, align } => {
                pad_to(&mut out, *align);
                // Safety: the value is valid for reads over this field's bytes.
                let src = unsafe { core::slice::from_raw_parts(base.add(*offset), *size) };
                out.extend_from_slice(src);
            }
        }
    }
    out
}

/// Lower `descriptor` and encode the value at `base` in one step.
///
/// # Safety
/// As [`encode_with`].
///
/// # Errors
/// As [`lower`].
pub unsafe fn encode(base: *const u8, descriptor: &Descriptor, reg: &Registry) -> Result<Vec<u8>> {
    let program = lower(descriptor, reg)?;
    // Safety: forwarded from this function's contract.
    Ok(unsafe { encode_with(&program, base) })
}

// ============================================================================
// Decode
// ============================================================================

/// Decode compact `bytes` into the value at `base`, by a prebuilt program,
/// rejecting trailing bytes.
///
/// # Safety
/// `base` must point to writable, suitably sized and aligned uninitialized
/// storage for the descriptor the program was lowered from. On `Ok` the bytes it
/// covers are initialized.
///
/// # Errors
/// [`CompactError`] for malformed or trailing input.
pub unsafe fn decode_with(program: &MemProgram, bytes: &[u8], base: *mut u8) -> Result<()> {
    let mut r = Reader::new(bytes);
    for op in program {
        match op {
            MemOp::Scalar { offset, size, align } => {
                skip_pad(&mut r, *align)?;
                let src = r.read_slice(*size)?;
                // Safety: `base` is valid for writes over this field's bytes, and
                // the wire bytes equal the in-memory bytes for a fixed scalar.
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), base.add(*offset), *size) };
            }
        }
    }
    if r.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(r.remaining())));
    }
    Ok(())
}

/// Lower `descriptor` and decode `bytes` into the value at `base` in one step.
///
/// # Safety
/// As [`decode_with`].
///
/// # Errors
/// As [`lower`] and [`decode_with`].
pub unsafe fn decode(
    bytes: &[u8],
    descriptor: &Descriptor,
    reg: &Registry,
    base: *mut u8,
) -> Result<()> {
    let program = lower(descriptor, reg)?;
    // Safety: forwarded from this function's contract.
    unsafe { decode_with(&program, bytes, base) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{MaybeUninit, align_of, offset_of, size_of};
    use facet_value::{VObject, VString, Value};
    use phon_ir::{FieldAccess, Layout, RecordAccess};
    use phon_schema::{Field, Schema, SchemaId, SchemaKind, SchemaRef, primitive_id};

    fn scalar(p: Primitive, size: usize, align: usize) -> Descriptor {
        Descriptor {
            schema: SchemaRef::concrete(primitive_id(p)),
            layout: Layout { size, align },
            access: Access::Scalar,
        }
    }

    fn field(name: &str, p: Primitive) -> Field {
        Field {
            name: name.to_string(),
            schema: SchemaRef::concrete(primitive_id(p)),
            required: true,
        }
    }

    // repr(Rust): the compiler is free to reorder these, so the memory offsets
    // are *not* the schema/wire order. That is the point — only the descriptor's
    // `offset_of!` values know where each field really lives.
    #[derive(Debug, PartialEq)]
    struct Pt {
        a: u8,
        b: u64,
        c: u16,
        flag: bool,
    }

    fn pt_schema() -> Schema {
        Schema {
            id: SchemaId(1),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "Pt".to_string(),
                fields: vec![
                    field("a", Primitive::U8),
                    field("b", Primitive::U64),
                    field("c", Primitive::U16),
                    field("flag", Primitive::Bool),
                ],
            },
        }
    }

    fn pt_descriptor() -> Descriptor {
        Descriptor {
            schema: SchemaRef::concrete(SchemaId(1)),
            layout: Layout {
                size: size_of::<Pt>(),
                align: align_of::<Pt>(),
            },
            access: Access::Record(RecordAccess {
                fields: vec![
                    FieldAccess {
                        offset: offset_of!(Pt, a),
                        descriptor: scalar(Primitive::U8, 1, 1),
                    },
                    FieldAccess {
                        offset: offset_of!(Pt, b),
                        descriptor: scalar(Primitive::U64, 8, 8),
                    },
                    FieldAccess {
                        offset: offset_of!(Pt, c),
                        descriptor: scalar(Primitive::U16, 2, 2),
                    },
                    FieldAccess {
                        offset: offset_of!(Pt, flag),
                        descriptor: scalar(Primitive::Bool, 1, 1),
                    },
                ],
                construct: Construct::InPlace,
            }),
        }
    }

    fn pt_object(p: &Pt) -> Value {
        let mut o = VObject::new();
        o.insert(VString::new("a"), Value::from(p.a));
        o.insert(VString::new("b"), Value::from(p.b));
        o.insert(VString::new("c"), Value::from(p.c));
        o.insert(VString::new("flag"), Value::from(p.flag));
        o.into()
    }

    #[test]
    fn typed_struct_matches_dynamic_and_roundtrips() {
        let reg = Registry::new([pt_schema()]);
        let desc = pt_descriptor();
        let program = lower(&desc, &reg).unwrap();
        // Fixed struct of scalars: a flat run of copies, nothing else.
        assert_eq!(program.len(), 4);

        let p = Pt {
            a: 0x11,
            b: 0x2222_2222_2222_2222,
            c: 0x3333,
            flag: true,
        };
        let typed_bytes = unsafe { encode_with(&program, (&raw const p).cast::<u8>()) };

        // Oracle: byte-identical to the dynamic codec for the equivalent object.
        let dyn_bytes = compact::to_bytes(&pt_object(&p), SchemaId(1), &reg).unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        // Round-trip back into a Pt.
        let mut slot = MaybeUninit::<Pt>::uninit();
        unsafe { decode_with(&program, &typed_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, p);
    }

    #[derive(Debug, PartialEq)]
    struct Outer {
        tag: u8,
        inner: Pt,
        n: u32,
    }

    #[test]
    fn nested_repr_rust_struct_splices_to_flat_copies() {
        let outer_schema = Schema {
            id: SchemaId(2),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "Outer".to_string(),
                fields: vec![
                    field("tag", Primitive::U8),
                    Field {
                        name: "inner".to_string(),
                        schema: SchemaRef::concrete(SchemaId(1)),
                        required: true,
                    },
                    field("n", Primitive::U32),
                ],
            },
        };
        let reg = Registry::new([pt_schema(), outer_schema]);

        let desc = Descriptor {
            schema: SchemaRef::concrete(SchemaId(2)),
            layout: Layout {
                size: size_of::<Outer>(),
                align: align_of::<Outer>(),
            },
            access: Access::Record(RecordAccess {
                fields: vec![
                    FieldAccess {
                        offset: offset_of!(Outer, tag),
                        descriptor: scalar(Primitive::U8, 1, 1),
                    },
                    FieldAccess {
                        offset: offset_of!(Outer, inner),
                        descriptor: pt_descriptor(),
                    },
                    FieldAccess {
                        offset: offset_of!(Outer, n),
                        descriptor: scalar(Primitive::U32, 4, 4),
                    },
                ],
                construct: Construct::InPlace,
            }),
        };

        let program = lower(&desc, &reg).unwrap();
        // tag + (a,b,c,flag spliced from inner) + n = 6 flat copies, no nesting.
        assert_eq!(program.len(), 6);

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
        let typed_bytes = unsafe { encode_with(&program, (&raw const o).cast::<u8>()) };

        // Oracle vs the dynamic codec over the equivalent nested object.
        let mut outer_obj = VObject::new();
        outer_obj.insert(VString::new("tag"), Value::from(o.tag));
        outer_obj.insert(VString::new("inner"), pt_object(&o.inner));
        outer_obj.insert(VString::new("n"), Value::from(o.n));
        let dyn_bytes = compact::to_bytes(&outer_obj.into(), SchemaId(2), &reg).unwrap();
        assert_eq!(typed_bytes, dyn_bytes);

        let mut slot = MaybeUninit::<Outer>::uninit();
        unsafe { decode_with(&program, &typed_bytes, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, o);
    }
}
