//! AArch64 relocation patching.
//!
//! A copy-and-patch stencil leaves its continuation as a relocation: a `B`/`BL`
//! whose 26-bit immediate is a hole. After copying stencils into one buffer, the
//! caller patches each hole so the branch targets the next stencil. Both `site`
//! and `target` are byte offsets within that buffer; since the branch is
//! PC-relative the in-buffer delta equals the in-memory delta, so patching before
//! copying into executable memory is sound.

/// Patch an AArch64 `B`/`BL` (`BRANCH26`) at `site` in `code` so it targets byte
/// offset `target` within the same buffer.
///
/// Preserves the opcode bits and rewrites only the signed 26-bit immediate
/// (in units of instructions). `site` and `target` must be 4-byte aligned and the
/// delta must fit in ±128 MiB (the `BRANCH26` range); both hold for stencils
/// copied into a single buffer.
pub fn patch_branch26(code: &mut [u8], site: usize, target: usize) {
    let instr = u32::from_le_bytes(code[site..site + 4].try_into().unwrap());
    let delta = (target as isize - site as isize) >> 2; // in instructions
    let imm26 = (delta as u32) & 0x03FF_FFFF;
    let patched = (instr & 0xFC00_0000) | imm26;
    code[site..site + 4].copy_from_slice(&patched.to_le_bytes());
}
