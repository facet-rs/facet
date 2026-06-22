//! Opt-in copy-and-patch JIT support shared by Weavy consumers.
//!
//! This module is still format- and IR-agnostic. Callers own their stencil
//! functions, state ABI, host calls, and lowering policy; Weavy only exposes the
//! neutral mechanics that multiple backends need.

pub use copypatch::patch_branch26;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use copypatch::ExecBuf;

/// Whether this build can allocate and run native copy-and-patch code.
pub const NATIVE_COPY_PATCH_AVAILABLE: bool =
    cfg!(all(target_os = "macos", target_arch = "aarch64"));

/// A copied stencil chain's entry point and associated program stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chain {
    /// Offset into the final code buffer where this chain starts.
    pub entry: usize,
    /// Index into the program-stream table for this chain.
    pub prog_index: usize,
}

/// A reserved word in one chain's program stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgSlot {
    /// Index into the program-stream table.
    pub prog_index: usize,
    /// Word index inside that program stream.
    pub slot: usize,
}

/// Code bytes plus side program streams for a copy-and-patch backend.
#[derive(Debug, Default)]
pub struct StencilLayout {
    code: Vec<u8>,
    progs: Vec<Vec<u64>>,
    stencil_count: usize,
}

impl StencilLayout {
    /// Create an empty stencil layout.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new callable chain at the current code offset.
    pub fn start_chain(&mut self) -> Chain {
        let entry = self.code.len();
        let prog_index = self.progs.len();
        self.progs.push(Vec::new());
        Chain { entry, prog_index }
    }

    /// Append one stencil and return its starting offset.
    pub fn emit_stencil(&mut self, stencil: &[u8]) -> usize {
        let start = self.code.len();
        self.code.extend_from_slice(stencil);
        self.stencil_count += 1;
        start
    }

    /// Current code-buffer length.
    #[must_use]
    pub fn code_len(&self) -> usize {
        self.code.len()
    }

    /// Patch an AArch64 continuation relocation to another offset in this layout.
    pub fn patch_branch26(&mut self, site: usize, target: usize) {
        patch_branch26(&mut self.code, site, target);
    }

    /// Append one word to a chain's program stream.
    pub fn push_prog_word(&mut self, prog_index: usize, value: u64) {
        self.progs[prog_index].push(value);
    }

    /// Mutably borrow a chain's program stream.
    pub fn prog_mut(&mut self, prog_index: usize) -> &mut Vec<u64> {
        &mut self.progs[prog_index]
    }

    /// Reserve one word in a chain's program stream for a later stable pointer.
    pub fn reserve_prog_slot(&mut self, prog_index: usize) -> ProgSlot {
        let slot = self.progs[prog_index].len();
        self.progs[prog_index].push(0);
        ProgSlot { prog_index, slot }
    }

    /// Fill a previously reserved program-stream word.
    pub fn fill_prog_slot(&mut self, slot: ProgSlot, value: u64) {
        self.progs[slot.prog_index][slot.slot] = value;
    }

    /// Borrow a chain's program stream.
    #[must_use]
    pub fn prog(&self, prog_index: usize) -> &[u64] {
        &self.progs[prog_index]
    }

    /// Borrow the copied code bytes.
    #[must_use]
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Number of stencils emitted into this layout.
    #[must_use]
    pub fn stencil_count(&self) -> usize {
        self.stencil_count
    }

    /// Split the layout into executable code bytes and side program streams.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, Vec<Vec<u64>>, usize) {
        (self.code, self.progs, self.stencil_count)
    }
}

#[cfg(test)]
mod tests {
    use super::StencilLayout;

    #[test]
    fn layout_tracks_chains_stencils_and_program_slots() {
        let mut layout = StencilLayout::new();
        let root = layout.start_chain();
        let first = layout.emit_stencil(&[1, 2, 3, 4]);
        layout.push_prog_word(root.prog_index, 7);
        let slot = layout.reserve_prog_slot(root.prog_index);
        layout.fill_prog_slot(slot, 11);
        let child = layout.start_chain();
        let second = layout.emit_stencil(&[5, 6]);

        assert_eq!(root.entry, 0);
        assert_eq!(root.prog_index, 0);
        assert_eq!(first, 0);
        assert_eq!(child.entry, 4);
        assert_eq!(child.prog_index, 1);
        assert_eq!(second, 4);
        assert_eq!(layout.code(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(layout.prog(root.prog_index), &[7, 11]);
        assert_eq!(layout.stencil_count(), 2);
    }
}
