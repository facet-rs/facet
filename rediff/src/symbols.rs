//! Symbols used for diff rendering.

/// Symbols for diff rendering.
///
/// These are the prefixes shown before lines/elements to indicate
/// what kind of change occurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSymbols {
    /// Symbol for deleted content (default: "-")
    pub deleted: &'static str,

    /// Symbol for inserted content (default: "+")
    pub inserted: &'static str,

    /// Symbol for content moved from this position (default: "←")
    pub moved_from: &'static str,

    /// Symbol for content moved to this position (default: "→")
    pub moved_to: &'static str,
}

impl Default for DiffSymbols {
    fn default() -> Self {
        Self::STANDARD
    }
}

impl DiffSymbols {
    /// Standard diff symbols using `-`, `+`, `←`, `→`
    pub const STANDARD: Self = Self {
        deleted: "-",
        inserted: "+",
        moved_from: "\u{2190}", // ←
        moved_to: "\u{2192}",   // →
    };

    /// ASCII-only symbols (for terminals that don't support Unicode)
    pub const ASCII: Self = Self {
        deleted: "-",
        inserted: "+",
        moved_from: "<-",
        moved_to: "->",
    };
}

/// The kind of change for a diff element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Content is unchanged
    Unchanged,
    /// Content was deleted (exists in old, not in new)
    Deleted,
    /// Content was inserted (exists in new, not in old)
    Inserted,
    /// Content was moved from this position to elsewhere
    MovedFrom,
    /// Content was moved to this position from elsewhere
    MovedTo,
    /// Content was modified (value changed)
    Modified,
}

impl ChangeKind {
    /// Get the symbol for this change kind.
    pub const fn symbol(self, symbols: &DiffSymbols) -> Option<&'static str> {
        match self {
            Self::Unchanged => None,
            Self::Deleted | Self::Modified => Some(symbols.deleted),
            Self::Inserted => Some(symbols.inserted),
            Self::MovedFrom => Some(symbols.moved_from),
            Self::MovedTo => Some(symbols.moved_to),
        }
    }

    /// Returns true if this change should be highlighted (not unchanged).
    pub const fn is_changed(self) -> bool {
        !matches!(self, Self::Unchanged)
    }
}
