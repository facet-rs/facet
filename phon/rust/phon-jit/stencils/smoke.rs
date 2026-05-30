//! Spine smoke stencil, in Rust. `build.rs` compiles this with rustc
//! (`--emit=obj`) — the same LLVM that builds the rest of phon — and extracts the
//! machine code from the object file. No clang, no C.
//!
//! Pure wrapping arithmetic: no panics, no external references, so the compiled
//! function is self-contained with no relocations.

#[no_mangle]
pub extern "C" fn phon_stencil_smoke(x: i64) -> i64 {
    x.wrapping_mul(3).wrapping_add(1)
}
