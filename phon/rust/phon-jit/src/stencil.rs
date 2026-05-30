//! Stencils: one small function per IR op, plus the threaded state they operate
//! on.
//!
//! In the finished copy-and-patch backend each of these functions is compiled
//! once at build time and its machine code extracted, with holes where its
//! immediates go (`r[ir.stencils]`). "JIT-compiling" a program is then `memcpy`
//! of the stencil bytes plus patching the holes — never instruction encoding by
//! hand.
//!
//! Until that extraction toolchain exists, [`lower`](crate::lower) stitches these
//! same functions by calling them through a function pointer (subroutine
//! threading). The functions, their immediates, and the state ABI are exactly
//! what the machine-code version will use; only the stitching changes. This is
//! the shape check: prove the ops decompose into stencils over a fixed state ABI
//! before investing in the toolchain.
//!
//! The state ABI mirrors `r[ir.stencils]`: a wire cursor, the value's base
//! pointer, and (later) a context pointer for what can't sit in a register — the
//! decode arena, the depth counter, the uniqueness seen-sets. Errors are returned
//! values, never panics across a stencil boundary.

use phon_schema::DecodeError;
use phon_schema::bytes::{Reader, pad_to, skip_pad};

/// A stencil's patch values — the immediates a copy-and-patch backend burns into
/// the copied code. [`MemOp::Scalar`](phon_ir::ir::MemOp::Scalar) uses
/// `[offset, size, align]`.
pub type Imm = [usize; 3];

/// Decode-side threaded state: the wire cursor and the destination base pointer.
pub struct DecodeCtx<'a> {
    pub reader: Reader<'a>,
    pub base: *mut u8,
}

/// Encode-side threaded state: the growing output and the source base pointer.
pub struct EncodeCtx {
    pub out: Vec<u8>,
    pub base: *const u8,
}

/// Decode a fixed-width scalar: pad the wire to `align`, then copy `size` bytes
/// from the wire into `base + offset`.
///
/// # Safety
/// `ctx.base` must be valid for writes over `[offset, offset + size)`.
///
/// # Errors
/// [`DecodeError`] if the wire is exhausted.
pub unsafe fn scalar_decode(ctx: &mut DecodeCtx, imm: &Imm) -> Result<(), DecodeError> {
    let [offset, size, align] = *imm;
    skip_pad(&mut ctx.reader, align)?;
    let src = ctx.reader.read_slice(size)?;
    // Safety: forwarded; the wire bytes equal the in-memory bytes for a scalar.
    unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), ctx.base.add(offset), size) };
    Ok(())
}

/// Encode a fixed-width scalar: pad the output to `align`, then append `size`
/// bytes read from `base + offset`.
///
/// # Safety
/// `ctx.base` must be valid for reads over `[offset, offset + size)`.
pub unsafe fn scalar_encode(ctx: &mut EncodeCtx, imm: &Imm) {
    let [offset, size, align] = *imm;
    pad_to(&mut ctx.out, align);
    // Safety: forwarded from the caller's contract.
    let src = unsafe { core::slice::from_raw_parts(ctx.base.add(offset), size) };
    ctx.out.extend_from_slice(src);
}
