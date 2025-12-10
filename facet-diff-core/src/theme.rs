//! Color themes for diff rendering.

use owo_colors::Rgb;

/// Color theme for diff rendering.
///
/// Defines colors for different kinds of changes. The default uses
/// Tokyo Night colors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffTheme {
    /// Color for deleted content (default: red)
    pub deleted: Rgb,

    /// Color for inserted content (default: green)
    pub inserted: Rgb,

    /// Color for moved content (default: blue)
    pub moved: Rgb,

    /// Color for unchanged/dimmed content (default: gray)
    pub unchanged: Rgb,

    /// Color for keys/field names (default: white)
    pub key: Rgb,

    /// Color for structural elements like braces, brackets (default: white)
    pub structure: Rgb,
}

impl Default for DiffTheme {
    fn default() -> Self {
        Self::TOKYO_NIGHT
    }
}

impl DiffTheme {
    /// Tokyo Night color theme (default).
    pub const TOKYO_NIGHT: Self = Self {
        deleted: Rgb(247, 118, 142),   // red
        inserted: Rgb(158, 206, 106),  // green
        moved: Rgb(122, 162, 247),     // blue
        unchanged: Rgb(86, 95, 137),   // gray
        key: Rgb(192, 202, 245),       // white
        structure: Rgb(192, 202, 245), // white
    };

    /// Get the color for a change kind.
    pub fn color_for(&self, kind: crate::ChangeKind) -> Rgb {
        match kind {
            crate::ChangeKind::Unchanged => self.unchanged,
            crate::ChangeKind::Deleted => self.deleted,
            crate::ChangeKind::Inserted => self.inserted,
            crate::ChangeKind::MovedFrom | crate::ChangeKind::MovedTo => self.moved,
            crate::ChangeKind::Modified => self.deleted, // old value gets deleted color
        }
    }
}
