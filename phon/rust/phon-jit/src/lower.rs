//! Lowering: compile a linear IR program into a flat table of
//! `(stencil, immediates)` entries — the copy-and-patch shape.
//!
//! One entry per op: the stencil is the reused code fragment, the immediates are
//! its patch values. The machine-code backend will `memcpy` each stencil's bytes
//! and patch the immediates in; this stand-in keeps the stencil as a function
//! pointer and calls it (subroutine threading). Both walk the *same* table and
//! produce identical results to the interpreter (`r[exec.jit-optional]`); the
//! difference is only how the stencils are stitched.
//!
//! First cut: the typed [`MemProgram`] (fixed scalars). The dynamic [`Program`]
//! and the inlining/`call-program` decisions (`r[ir.inlining]`) follow once the
//! machine-code toolchain lands.
//!
//! [`Program`]: phon_ir::ir::Program
//! [`MemProgram`]: phon_ir::ir::MemProgram

use phon_ir::ir::{MemOp, MemProgram};
use phon_ir::{PointerThunks, SeqThunks};
use phon_schema::DecodeError;
use phon_schema::bytes::{Reader, write_u32};

use crate::stencil::{self, DecodeCtx, EncodeCtx, Imm};

/// A compiled decode program: a straight table of stencils and their patch
/// values. Build it once with [`compile_decode`], run it over many messages.
pub struct CompiledDecode {
    steps: Vec<DecodeStep>,
}

enum DecodeStep {
    Scalar {
        stencil: unsafe fn(&mut DecodeCtx, &Imm) -> Result<(), DecodeError>,
        imm: Imm,
    },
    Pointer {
        field_offset: usize,
        pointee_size: usize,
        pointee_align: usize,
        thunks: PointerThunks,
        pointee: CompiledDecode,
    },
}

/// A compiled encode program.
///
/// Scalars are threaded through function-pointer stencils as before; a sequence
/// holds its element sub-program inline (a recursive [`CompiledEncode`]) plus the
/// handle thunks, mirroring the native encoder's call-program layout. The portable
/// oracle for the JIT, so it must handle exactly what the JIT does.
pub struct CompiledEncode {
    steps: Vec<EncodeStep>,
}

enum EncodeStep {
    /// A fixed scalar: a stencil plus its `[offset, size, align]` immediates.
    Scalar {
        stencil: unsafe fn(&mut EncodeCtx, &Imm),
        imm: Imm,
    },
    /// An owned sequence: write its `u32` count, then encode each element with the
    /// element sub-program at `data + i*stride`.
    Sequence {
        field_offset: usize,
        stride: usize,
        thunks: SeqThunks,
        element: CompiledEncode,
    },
    /// An owned pointer: borrow the pointee through the local thunk, then encode
    /// the pointee wire shape.
    Pointer {
        field_offset: usize,
        thunks: PointerThunks,
        pointee: CompiledEncode,
    },
}

/// Compile a typed program for decoding: select a stencil per op and capture its
/// immediates.
#[must_use]
pub fn compile_decode(program: &MemProgram) -> CompiledDecode {
    let steps = program
        .iter()
        .map(|op| match op {
            MemOp::Scalar {
                offset,
                size,
                align,
            } => DecodeStep::Scalar {
                stencil: stencil::scalar_decode,
                imm: [*offset, *size, *align],
            },
            MemOp::NativeInt { .. } => {
                panic!("phon-jit: native-sized integer casts are interpreter-only for now")
            }
            MemOp::Sequence(_) => panic!("phon-jit: sequences are interpreter-only for now"),
            MemOp::Set(_) => panic!("phon-jit: sets are interpreter-only for now"),
            MemOp::Bytes(_) => {
                panic!("phon-jit: bulk byte runs (String) are interpreter-only for now")
            }
            MemOp::Borrow(_) => {
                panic!("phon-jit: borrowed leaves (&str/&[u8]) are interpreter-only for now")
            }
            MemOp::Option(_) => panic!("phon-jit: Option is interpreter-only for now"),
            MemOp::Enum(_) => panic!("phon-jit: enums are interpreter-only for now"),
            MemOp::Map(_) => panic!("phon-jit: maps are interpreter-only for now"),
            MemOp::Result(_) => panic!("phon-jit: Result is interpreter-only for now"),
            MemOp::Pointer(p) => DecodeStep::Pointer {
                field_offset: p.field_offset,
                pointee_size: p.pointee_size,
                pointee_align: p.pointee_align,
                thunks: p.thunks,
                pointee: compile_decode(&p.pointee),
            },
            MemOp::Dynamic { .. } => panic!("phon-jit: dynamic Value is interpreter-only for now"),
            MemOp::Opaque(_) => panic!("phon-jit: opaque fields are interpreter-only for now"),
            MemOp::CallBlock { .. } => panic!("phon-jit: recursion is interpreter-only for now"),
            MemOp::SkipWire(_) | MemOp::Default(_) => {
                panic!("phon-jit: compat skip/default are interpreter-only for now")
            }
        })
        .collect();
    CompiledDecode { steps }
}

/// Compile a typed program for encoding.
#[must_use]
pub fn compile_encode(program: &MemProgram) -> CompiledEncode {
    let steps = program
        .iter()
        .map(|op| match op {
            MemOp::Scalar {
                offset,
                size,
                align,
            } => EncodeStep::Scalar {
                stencil: stencil::scalar_encode,
                imm: [*offset, *size, *align],
            },
            MemOp::NativeInt { .. } => {
                panic!("phon-jit: native-sized integer casts are interpreter-only for now")
            }
            MemOp::Sequence(s) => EncodeStep::Sequence {
                field_offset: s.field_offset,
                stride: s.stride,
                thunks: s.thunks,
                element: compile_encode(&s.element),
            },
            MemOp::Set(_) => panic!("phon-jit: sets are interpreter-only for now"),
            MemOp::Bytes(_) => {
                panic!("phon-jit: bulk byte runs (String) are interpreter-only for now")
            }
            MemOp::Borrow(_) => {
                panic!("phon-jit: borrowed leaves (&str/&[u8]) are interpreter-only for now")
            }
            MemOp::Option(_) => panic!("phon-jit: Option is interpreter-only for now"),
            MemOp::Enum(_) => panic!("phon-jit: enums are interpreter-only for now"),
            MemOp::Map(_) => panic!("phon-jit: maps are interpreter-only for now"),
            MemOp::Result(_) => panic!("phon-jit: Result is interpreter-only for now"),
            MemOp::Pointer(p) => EncodeStep::Pointer {
                field_offset: p.field_offset,
                thunks: p.thunks,
                pointee: compile_encode(&p.pointee),
            },
            MemOp::Dynamic { .. } => panic!("phon-jit: dynamic Value is interpreter-only for now"),
            MemOp::Opaque(_) => panic!("phon-jit: opaque fields are interpreter-only for now"),
            MemOp::CallBlock { .. } => panic!("phon-jit: recursion is interpreter-only for now"),
            MemOp::SkipWire(_) | MemOp::Default(_) => {
                panic!("phon-jit: compat skip/default are interpreter-only for now")
            }
        })
        .collect();
    CompiledEncode { steps }
}

impl CompiledDecode {
    /// Decode `bytes` into the value at `base`, rejecting trailing bytes.
    ///
    /// # Safety
    /// `base` must point to writable storage matching the descriptor this program
    /// was lowered from; on `Ok` the bytes it covers are initialized.
    ///
    /// # Errors
    /// [`DecodeError`] for malformed or trailing input.
    pub unsafe fn run(&self, bytes: &[u8], base: *mut u8) -> Result<(), DecodeError> {
        let mut ctx = DecodeCtx {
            reader: Reader::new(bytes),
            base,
        };
        for step in &self.steps {
            // Safety: forwarded; each step writes within the value's layout.
            unsafe { step.run(&mut ctx)? };
        }
        if ctx.reader.remaining() != 0 {
            return Err(DecodeError::TrailingBytes(ctx.reader.remaining()));
        }
        Ok(())
    }
}

impl DecodeStep {
    unsafe fn run(&self, ctx: &mut DecodeCtx<'_>) -> Result<(), DecodeError> {
        match self {
            DecodeStep::Scalar { stencil, imm } => {
                // Safety: forwarded; the scalar op writes within the current base.
                unsafe { (stencil)(ctx, imm) }
            }
            DecodeStep::Pointer {
                field_offset,
                pointee_size,
                pointee_align,
                thunks,
                pointee,
            } => {
                // Safety: the pointer handle lives at `field_offset` and is
                // uninitialized; `init` moves the scratch-decoded pointee into it.
                let pointer = unsafe { ctx.base.add(*field_offset) };
                let (scratch, layout) = alloc_scratch(*pointee_size, *pointee_align)?;
                let previous_base = ctx.base;
                ctx.base = scratch;
                let decode_result = unsafe { pointee.run_in_ctx(ctx) };
                ctx.base = previous_base;

                if let Err(err) = decode_result {
                    free_scratch(scratch, layout);
                    return Err(err);
                }

                // Safety: scratch now holds an initialized pointee value; the thunk
                // constructs the owning pointer by moving that value.
                unsafe { (thunks.init)(thunks.ctx, pointer, scratch) };
                free_scratch(scratch, layout);
                Ok(())
            }
        }
    }
}

impl CompiledDecode {
    unsafe fn run_in_ctx(&self, ctx: &mut DecodeCtx<'_>) -> Result<(), DecodeError> {
        for step in &self.steps {
            // Safety: forwarded from the caller; nested offsets are relative to
            // `ctx.base`, which the caller sets to the current value.
            unsafe { step.run(ctx)? };
        }
        Ok(())
    }
}

impl CompiledEncode {
    /// Encode the value at `base` into compact bytes.
    ///
    /// # Safety
    /// `base` must point to an initialized value matching the descriptor this
    /// program was lowered from.
    #[must_use]
    pub unsafe fn run(&self, base: *const u8) -> Vec<u8> {
        let mut out = Vec::new();
        // Safety: forwarded from this function's contract.
        unsafe { self.run_into(base, &mut out) };
        out
    }

    /// Append this program's encoding of the value at `base` to `out`.
    ///
    /// # Safety
    /// As [`run`](Self::run); additionally `out` accumulates wire bytes in order.
    unsafe fn run_into(&self, base: *const u8, out: &mut Vec<u8>) {
        for step in &self.steps {
            match step {
                EncodeStep::Scalar { stencil, imm } => {
                    // The scalar stencil owns its output; lend it the buffer by
                    // moving it in and back out (no copy — `Vec` move is a pointer
                    // swap).
                    let mut ctx = EncodeCtx {
                        out: core::mem::take(out),
                        base,
                    };
                    // Safety: forwarded; the step reads within the value's layout.
                    unsafe { (stencil)(&mut ctx, imm) };
                    *out = ctx.out;
                }
                EncodeStep::Sequence {
                    field_offset,
                    stride,
                    thunks,
                    element,
                } => {
                    // Safety: the sequence handle lives at `field_offset`.
                    let list = unsafe { base.add(*field_offset) };
                    let n = unsafe { (thunks.len)(thunks.ctx, list) };
                    write_u32(out, n as u32);
                    let data = unsafe { (thunks.data)(thunks.ctx, list) };
                    for i in 0..n {
                        // Safety: element `i` lives at `data + i*stride`.
                        unsafe { element.run_into(data.add(i * stride), out) };
                    }
                }
                EncodeStep::Pointer {
                    field_offset,
                    thunks,
                    pointee,
                } => {
                    // Safety: the owning pointer handle lives at `field_offset`;
                    // the borrow thunk returns the initialized pointee.
                    let pointer = unsafe { base.add(*field_offset) };
                    let pointee_base = unsafe { (thunks.borrow)(thunks.ctx, pointer) };
                    // Safety: the pointee program's offsets are relative to the
                    // pointer target returned by the thunk.
                    unsafe { pointee.run_into(pointee_base, out) };
                }
            }
        }
    }
}

fn alloc_scratch(
    size: usize,
    align: usize,
) -> Result<(*mut u8, Option<std::alloc::Layout>), DecodeError> {
    if size == 0 {
        Ok((align as *mut u8, None))
    } else {
        let layout = std::alloc::Layout::from_size_align(size, align)
            .map_err(|_| DecodeError::Malformed("pointer scratch layout overflow"))?;
        // Safety: size > 0 and the layout was validated.
        let buf = unsafe { std::alloc::alloc(layout) };
        if buf.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Ok((buf, Some(layout)))
    }
}

fn free_scratch(buf: *mut u8, layout: Option<std::alloc::Layout>) {
    if let Some(layout) = layout {
        // Safety: `buf` was allocated by `alloc_scratch` with this exact layout.
        unsafe { std::alloc::dealloc(buf, layout) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phon_ir::ir::{MemOp, PointerOp};

    // A two-field value in memory: u32 at offset 0, u64 at offset 8. On the wire,
    // compact alignment puts the u32 first, pads to 8, then the u64.
    #[repr(C, align(8))]
    struct Mem([u8; 16]);

    #[test]
    fn jit_encode_matches_layout_and_decode_roundtrips() {
        let program: MemProgram = vec![
            MemOp::Scalar {
                offset: 0,
                size: 4,
                align: 4,
            },
            MemOp::Scalar {
                offset: 8,
                size: 8,
                align: 8,
            },
        ];
        let enc = compile_encode(&program);
        let dec = compile_decode(&program);

        let mut mem = Mem([0; 16]);
        mem.0[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        mem.0[8..16].copy_from_slice(&0xAABB_CCDD_EEFF_0011u64.to_le_bytes());

        let bytes = unsafe { enc.run(mem.0.as_ptr()) };
        // u32 (4) + pad (4) + u64 (8) = 16 wire bytes, byte-for-byte.
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[0..4], &0x1122_3344u32.to_le_bytes());
        assert_eq!(&bytes[4..8], &[0, 0, 0, 0]);
        assert_eq!(&bytes[8..16], &0xAABB_CCDD_EEFF_0011u64.to_le_bytes());

        // Decode into fresh storage and confirm it reproduces the value.
        let mut out = Mem([0; 16]);
        unsafe { dec.run(&bytes, out.0.as_mut_ptr()) }.unwrap();
        assert_eq!(out.0, mem.0);
    }

    #[test]
    fn jit_decode_rejects_trailing_bytes() {
        let program: MemProgram = vec![MemOp::Scalar {
            offset: 0,
            size: 4,
            align: 4,
        }];
        let dec = compile_decode(&program);
        let mut out = [0u8; 4];
        // 4 bytes of value + 1 stray byte.
        let err = unsafe { dec.run(&[1, 2, 3, 4, 5], out.as_mut_ptr()) }.unwrap_err();
        assert!(matches!(err, DecodeError::TrailingBytes(1)));
    }

    unsafe extern "C" fn box_u32_borrow(_ctx: *const (), pointer: *const u8) -> *const u8 {
        // Safety: test programs bind this thunk only to initialized `Box<u32>`
        // handles.
        let owner = unsafe { &*pointer.cast::<Box<u32>>() };
        core::ptr::from_ref(&**owner).cast()
    }

    unsafe extern "C" fn box_u32_init(_ctx: *const (), pointer: *mut u8, value: *mut u8) {
        // Safety: `value` points to an initialized scratch `u32` and `pointer`
        // points to uninitialized `Box<u32>` storage.
        let value = unsafe { value.cast::<u32>().read() };
        unsafe { pointer.cast::<Box<u32>>().write(Box::new(value)) };
    }

    fn box_u32_thunks() -> PointerThunks {
        PointerThunks {
            ctx: core::ptr::null(),
            borrow: box_u32_borrow,
            init: box_u32_init,
        }
    }

    fn box_u32_pointer(field_offset: usize) -> MemOp {
        MemOp::Pointer(Box::new(PointerOp {
            field_offset,
            pointee: vec![MemOp::Scalar {
                offset: 0,
                size: core::mem::size_of::<u32>(),
                align: core::mem::align_of::<u32>(),
            }],
            pointee_size: core::mem::size_of::<u32>(),
            pointee_align: core::mem::align_of::<u32>(),
            thunks: box_u32_thunks(),
        }))
    }

    #[repr(C)]
    #[derive(Debug, PartialEq)]
    struct BoxHolder {
        tag: u32,
        inner: Box<u32>,
        tail: u32,
    }

    #[test]
    // r[verify descriptors.thunk-binding]
    // r[verify exec.jit-optional]
    // r[verify ir.stencils]
    fn threaded_pointer_field_uses_pointee_wire_shape() {
        let program: MemProgram = vec![
            MemOp::Scalar {
                offset: core::mem::offset_of!(BoxHolder, tag),
                size: core::mem::size_of::<u32>(),
                align: core::mem::align_of::<u32>(),
            },
            box_u32_pointer(core::mem::offset_of!(BoxHolder, inner)),
            MemOp::Scalar {
                offset: core::mem::offset_of!(BoxHolder, tail),
                size: core::mem::size_of::<u32>(),
                align: core::mem::align_of::<u32>(),
            },
        ];
        let enc = compile_encode(&program);
        let dec = compile_decode(&program);

        let holder = BoxHolder {
            tag: 0x1122_3344,
            inner: Box::new(0xAABB_CCDD),
            tail: 0x5566_7788,
        };
        let bytes = unsafe { enc.run(core::ptr::from_ref(&holder).cast::<u8>()) };
        let mut expected = Vec::new();
        expected.extend_from_slice(&holder.tag.to_le_bytes());
        expected.extend_from_slice(&holder.inner.to_le_bytes());
        expected.extend_from_slice(&holder.tail.to_le_bytes());
        assert_eq!(bytes, expected);

        let mut out = core::mem::MaybeUninit::<BoxHolder>::uninit();
        unsafe { dec.run(&bytes, out.as_mut_ptr().cast::<u8>()) }.unwrap();
        assert_eq!(unsafe { out.assume_init() }, holder);
    }

    #[test]
    // r[verify descriptors.thunk-binding]
    // r[verify exec.jit-optional]
    // r[verify ir.stencils]
    fn threaded_pointer_root_roundtrips_as_pointee() {
        let program: MemProgram = vec![box_u32_pointer(0)];
        let enc = compile_encode(&program);
        let dec = compile_decode(&program);

        let value = Box::new(0x1234_5678u32);
        let bytes = unsafe { enc.run(core::ptr::from_ref(&value).cast::<u8>()) };
        assert_eq!(bytes, (*value).to_le_bytes());

        let mut out = core::mem::MaybeUninit::<Box<u32>>::uninit();
        unsafe { dec.run(&bytes, out.as_mut_ptr().cast::<u8>()) }.unwrap();
        assert_eq!(unsafe { out.assume_init() }, value);
    }
}
