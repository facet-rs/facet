/// Compiler/runtime-owned offset of one word in a Weavy entry frame.
///
/// A slot is valid only for the program layout in the lowering artifact that
/// produced it. Raw byte offsets stay inside the Vix-to-Weavy ABI boundary.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FrameSlot(u32);

impl FrameSlot {
    const WORD_BYTES: usize = size_of::<i64>();

    pub(crate) fn for_word(index: usize) -> Option<Self> {
        index
            .checked_mul(Self::WORD_BYTES)
            .and_then(|offset| u32::try_from(offset).ok())
            .map(Self)
    }

    pub(crate) fn frame_size(words: usize) -> Option<usize> {
        words.checked_mul(Self::WORD_BYTES)
    }

    pub(crate) const fn byte_offset(self) -> u32 {
        self.0
    }
}
