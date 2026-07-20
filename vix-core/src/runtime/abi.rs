/// Compiler/runtime-owned offset of one word in a Weavy entry frame.
///
/// A slot is valid only for the program layout in the lowering artifact that
/// produced it. Raw byte offsets stay inside the Vix-to-Weavy ABI boundary.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FrameSlot(u32);

impl FrameSlot {
    const WORD_BYTES: usize = size_of::<i64>();

    pub(crate) const fn word_size() -> u32 {
        Self::WORD_BYTES as u32
    }

    pub(crate) const fn word_align() -> usize {
        align_of::<i64>()
    }

    pub(crate) fn for_word(index: usize) -> Option<Self> {
        index
            .checked_mul(Self::WORD_BYTES)
            .and_then(|offset| u32::try_from(offset).ok())
            .map(Self)
    }

    pub(crate) fn frame_size(words: usize) -> Option<usize> {
        words.checked_mul(Self::WORD_BYTES)
    }

    pub const fn byte_offset(self) -> u32 {
        self.0
    }

    pub(crate) const fn word_index(self) -> usize {
        self.0 as usize / Self::WORD_BYTES
    }
}

/// Typed width of an inline value in the Weavy frame ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameWords(u32);

impl FrameWords {
    pub(crate) const ONE: Self = Self(1);

    pub(crate) fn from_usize(words: usize) -> Option<Self> {
        u32::try_from(words).ok().map(Self)
    }

    pub(crate) const fn as_usize(self) -> usize {
        self.0 as usize
    }

    pub(crate) fn byte_size(self) -> Option<u32> {
        self.0.checked_mul(FrameSlot::word_size())
    }
}

/// One statically typed contiguous value region in a Weavy frame.
///
/// All aggregate layout arithmetic is centralized here so VIR lowering deals
/// in value regions rather than untyped byte offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameRegion {
    start: FrameSlot,
    words: FrameWords,
}

impl FrameRegion {
    pub(crate) fn for_words(start_word: usize, words: FrameWords) -> Option<Self> {
        let end_word = start_word.checked_add(words.as_usize())?;
        FrameSlot::for_word(end_word)?;
        Some(Self {
            start: FrameSlot::for_word(start_word)?,
            words,
        })
    }

    pub(crate) const fn start(self) -> FrameSlot {
        self.start
    }

    pub(crate) const fn words(self) -> FrameWords {
        self.words
    }

    pub(crate) fn byte_size(self) -> Option<u32> {
        self.words.byte_size()
    }

    pub(crate) fn word(self, index: usize) -> Option<FrameSlot> {
        if index >= self.words.as_usize() {
            return None;
        }
        let byte_delta = index.checked_mul(FrameSlot::WORD_BYTES)?;
        let byte_delta = u32::try_from(byte_delta).ok()?;
        self.start.0.checked_add(byte_delta).map(FrameSlot)
    }
}
