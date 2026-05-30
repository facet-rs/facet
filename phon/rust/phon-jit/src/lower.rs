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

use phon_ir::ir::{MemOp, MemProgram};
use phon_schema::DecodeError;
use phon_schema::bytes::Reader;

use crate::stencil::{self, DecodeCtx, EncodeCtx, Imm};

/// A compiled decode program: a straight table of stencils and their patch
/// values. Build it once with [`compile_decode`], run it over many messages.
pub struct CompiledDecode {
    steps: Vec<DecodeStep>,
}

struct DecodeStep {
    stencil: unsafe fn(&mut DecodeCtx, &Imm) -> Result<(), DecodeError>,
    imm: Imm,
}

/// A compiled encode program.
pub struct CompiledEncode {
    steps: Vec<EncodeStep>,
}

struct EncodeStep {
    stencil: unsafe fn(&mut EncodeCtx, &Imm),
    imm: Imm,
}

/// Compile a typed program for decoding: select a stencil per op and capture its
/// immediates.
#[must_use]
pub fn compile_decode(program: &MemProgram) -> CompiledDecode {
    let steps = program
        .iter()
        .map(|op| match op {
            MemOp::Scalar { offset, size, align } => DecodeStep {
                stencil: stencil::scalar_decode,
                imm: [*offset, *size, *align],
            },
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
            MemOp::Scalar { offset, size, align } => EncodeStep {
                stencil: stencil::scalar_encode,
                imm: [*offset, *size, *align],
            },
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
            unsafe { (step.stencil)(&mut ctx, &step.imm)? };
        }
        if ctx.reader.remaining() != 0 {
            return Err(DecodeError::TrailingBytes(ctx.reader.remaining()));
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
        let mut ctx = EncodeCtx {
            out: Vec::new(),
            base,
        };
        for step in &self.steps {
            // Safety: forwarded; each step reads within the value's layout.
            unsafe { (step.stencil)(&mut ctx, &step.imm) };
        }
        ctx.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phon_ir::ir::MemOp;

    // A two-field value in memory: u32 at offset 0, u64 at offset 8. On the wire,
    // compact alignment puts the u32 first, pads to 8, then the u64.
    #[repr(C, align(8))]
    struct Mem([u8; 16]);

    #[test]
    fn jit_encode_matches_layout_and_decode_roundtrips() {
        let program: MemProgram = vec![
            MemOp::Scalar { offset: 0, size: 4, align: 4 },
            MemOp::Scalar { offset: 8, size: 8, align: 8 },
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
        let program: MemProgram = vec![MemOp::Scalar { offset: 0, size: 4, align: 4 }];
        let dec = compile_decode(&program);
        let mut out = [0u8; 4];
        // 4 bytes of value + 1 stray byte.
        let err = unsafe { dec.run(&[1, 2, 3, 4, 5], out.as_mut_ptr()) }.unwrap_err();
        assert!(matches!(err, DecodeError::TrailingBytes(1)));
    }
}
