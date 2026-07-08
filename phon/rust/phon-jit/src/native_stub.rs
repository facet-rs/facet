use phon_ir::ir::{Lowered, MemProgram};
use phon_schema::DecodeError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeProgramStats {
    pub chain_count: usize,
    pub stencil_count: usize,
    pub prog_slot_count: usize,
    pub scalar_run_count: usize,
    pub scalar_run_segment_count: usize,
}

pub struct NativeDecode;

pub struct NativeEncode;

#[must_use]
pub fn available() -> bool {
    false
}

impl NativeDecode {
    #[must_use]
    pub fn compile(_program: &MemProgram) -> NativeDecode {
        unreachable!("phon native JIT is inactive for this build")
    }

    #[must_use]
    pub fn compile_lowered(_lowered: &Lowered) -> NativeDecode {
        unreachable!("phon native JIT is inactive for this build")
    }

    #[must_use]
    pub fn stats(&self) -> NativeProgramStats {
        NativeProgramStats::default()
    }

    /// # Safety
    /// This inactive backend never runs native code.
    pub unsafe fn run(&self, _bytes: &[u8], _base: *mut u8) -> Result<(), DecodeError> {
        unreachable!("phon native JIT is inactive for this build")
    }
}

impl NativeEncode {
    #[must_use]
    pub fn compile(_program: &MemProgram) -> NativeEncode {
        unreachable!("phon native JIT is inactive for this build")
    }

    #[must_use]
    pub fn compile_lowered(_lowered: &Lowered) -> NativeEncode {
        unreachable!("phon native JIT is inactive for this build")
    }

    #[must_use]
    pub fn stats(&self) -> NativeProgramStats {
        NativeProgramStats::default()
    }

    /// # Safety
    /// This inactive backend never runs native code.
    pub unsafe fn run(&self, _base: *const u8) -> Vec<u8> {
        unreachable!("phon native JIT is inactive for this build")
    }
}
