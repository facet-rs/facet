//! Color themes for diff rendering.

use owo_colors::Rgb;
use palette::{FromColor, Lch, LinSrgb, Mix, Srgb};

/// Color theme for diff rendering.
///
/// Defines colors for different kinds of changes. The default uses
/// colorblind-friendly yellow/blue with type-specific value colors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffTheme {
    /// Foreground color for deleted content (accent color)
    pub deleted: Rgb,

    /// Foreground color for inserted content (accent color)
    pub inserted: Rgb,

    /// Foreground color for moved content (accent color)
    pub moved: Rgb,

    /// Foreground color for unchanged content
    pub unchanged: Rgb,

    /// Foreground color for keys/field names
    pub key: Rgb,

    /// Foreground color for structural elements like braces, brackets
    pub structure: Rgb,

    /// Foreground color for comments and type hints
    pub comment: Rgb,

    // === Value type base colors ===
    /// Base color for string values
    pub string: Rgb,

    /// Base color for numeric values (integers, floats)
    pub number: Rgb,

    /// Base color for boolean values
    pub boolean: Rgb,

    /// Base color for null/None values
    pub null: Rgb,

    /// Subtle background for deleted lines (None = no background)
    pub deleted_line_bg: Option<Rgb>,

    /// Stronger background highlight for changed values on deleted lines
    pub deleted_highlight_bg: Option<Rgb>,

    /// Subtle background for inserted lines (None = no background)
    pub inserted_line_bg: Option<Rgb>,

    /// Stronger background highlight for changed values on inserted lines
    pub inserted_highlight_bg: Option<Rgb>,

    /// Subtle background for moved lines (None = no background)
    pub moved_line_bg: Option<Rgb>,

    /// Stronger background highlight for changed values on moved lines
    pub moved_highlight_bg: Option<Rgb>,
}

impl Default for DiffTheme {
    fn default() -> Self {
        Self::COLORBLIND_WITH_BG
    }
}

impl DiffTheme {
    /// Colorblind-friendly theme - orange vs blue. No backgrounds.
    pub const COLORBLIND_ORANGE_BLUE: Self = Self {
        deleted: Rgb(255, 167, 89),    // #ffa759 warm orange
        inserted: Rgb(97, 175, 239),   // #61afef sky blue
        moved: Rgb(198, 120, 221),     // #c678dd purple/magenta
        unchanged: Rgb(140, 140, 140), // #8c8c8c medium gray (muted)
        key: Rgb(140, 140, 140),       // #8c8c8c medium gray
        structure: Rgb(220, 220, 220), // #dcdcdc light gray (structural elements)
        comment: Rgb(100, 100, 100),   // #646464 dark gray (very muted)
        string: Rgb(152, 195, 121),    // #98c379 green (like One Dark Pro)
        number: Rgb(209, 154, 102),    // #d19a66 orange
        boolean: Rgb(209, 154, 102),   // #d19a66 orange
        null: Rgb(86, 182, 194),       // #56b6c2 cyan
        deleted_line_bg: None,
        deleted_highlight_bg: None,
        inserted_line_bg: None,
        inserted_highlight_bg: None,
        moved_line_bg: None,
        moved_highlight_bg: None,
    };

    /// Colorblind-friendly with line + highlight backgrounds (yellow/blue).
    pub const COLORBLIND_WITH_BG: Self = Self {
        deleted: Rgb(229, 192, 123),   // #e5c07b warm yellow/gold
        inserted: Rgb(97, 175, 239),   // #61afef sky blue
        moved: Rgb(198, 120, 221),     // #c678dd purple/magenta
        unchanged: Rgb(140, 140, 140), // #8c8c8c medium gray (muted)
        key: Rgb(140, 140, 140),       // #8c8c8c medium gray
        structure: Rgb(220, 220, 220), // #dcdcdc light gray (structural elements)
        comment: Rgb(100, 100, 100),   // #646464 dark gray (very muted)
        string: Rgb(152, 195, 121),    // #98c379 green (like One Dark Pro)
        number: Rgb(209, 154, 102),    // #d19a66 orange
        boolean: Rgb(209, 154, 102),   // #d19a66 orange
        null: Rgb(86, 182, 194),       // #56b6c2 cyan
        // Subtle line backgrounds
        deleted_line_bg: Some(Rgb(55, 48, 35)), // medium-dark warm yellow
        inserted_line_bg: Some(Rgb(35, 48, 60)), // medium-dark cool blue
        moved_line_bg: Some(Rgb(50, 40, 60)),   // medium-dark purple
        // Stronger highlight backgrounds for changed values
        deleted_highlight_bg: Some(Rgb(90, 75, 50)), // medium yellow/brown
        inserted_highlight_bg: Some(Rgb(45, 70, 95)), // medium blue
        moved_highlight_bg: Some(Rgb(80, 55, 95)),   // medium purple
    };

    /// Pastel color theme - soft but distinguishable (not colorblind-friendly).
    pub const PASTEL: Self = Self {
        deleted: Rgb(255, 138, 128),   // #ff8a80 saturated coral/salmon
        inserted: Rgb(128, 203, 156),  // #80cb9c saturated mint green
        moved: Rgb(128, 179, 255),     // #80b3ff saturated sky blue
        unchanged: Rgb(140, 140, 140), // #8c8c8c medium gray (muted)
        key: Rgb(140, 140, 140),       // #8c8c8c medium gray
        structure: Rgb(220, 220, 220), // #dcdcdc light gray (structural elements)
        comment: Rgb(100, 100, 100),   // #646464 dark gray (very muted)
        string: Rgb(152, 195, 121),    // #98c379 green
        number: Rgb(209, 154, 102),    // #d19a66 orange
        boolean: Rgb(209, 154, 102),   // #d19a66 orange
        null: Rgb(86, 182, 194),       // #56b6c2 cyan
        deleted_line_bg: None,
        deleted_highlight_bg: None,
        inserted_line_bg: None,
        inserted_highlight_bg: None,
        moved_line_bg: None,
        moved_highlight_bg: None,
    };

    /// One Dark Pro color theme.
    pub const ONE_DARK_PRO: Self = Self {
        deleted: Rgb(224, 108, 117),   // #e06c75 red
        inserted: Rgb(152, 195, 121),  // #98c379 green
        moved: Rgb(97, 175, 239),      // #61afef blue
        unchanged: Rgb(171, 178, 191), // #abb2bf white (normal text)
        key: Rgb(171, 178, 191),       // #abb2bf white
        structure: Rgb(171, 178, 191), // #abb2bf white
        comment: Rgb(92, 99, 112),     // #5c6370 gray (muted)
        string: Rgb(152, 195, 121),    // #98c379 green
        number: Rgb(209, 154, 102),    // #d19a66 orange
        boolean: Rgb(209, 154, 102),   // #d19a66 orange
        null: Rgb(86, 182, 194),       // #56b6c2 cyan
        deleted_line_bg: None,
        deleted_highlight_bg: None,
        inserted_line_bg: None,
        inserted_highlight_bg: None,
        moved_line_bg: None,
        moved_highlight_bg: None,
    };

    /// Tokyo Night color theme.
    pub const TOKYO_NIGHT: Self = Self {
        deleted: Rgb(247, 118, 142),   // red
        inserted: Rgb(158, 206, 106),  // green
        moved: Rgb(122, 162, 247),     // blue
        unchanged: Rgb(192, 202, 245), // white (normal text)
        key: Rgb(192, 202, 245),       // white
        structure: Rgb(192, 202, 245), // white
        comment: Rgb(86, 95, 137),     // gray (muted)
        string: Rgb(158, 206, 106),    // green
        number: Rgb(255, 158, 100),    // orange
        boolean: Rgb(255, 158, 100),   // orange
        null: Rgb(125, 207, 255),      // cyan
        deleted_line_bg: None,
        deleted_highlight_bg: None,
        inserted_line_bg: None,
        inserted_highlight_bg: None,
        moved_line_bg: None,
        moved_highlight_bg: None,
    };

    /// Get the color for a change kind.
    pub const fn color_for(&self, kind: crate::ChangeKind) -> Rgb {
        match kind {
            crate::ChangeKind::Unchanged => self.unchanged,
            crate::ChangeKind::Deleted => self.deleted,
            crate::ChangeKind::Inserted => self.inserted,
            crate::ChangeKind::MovedFrom | crate::ChangeKind::MovedTo => self.moved,
            crate::ChangeKind::Modified => self.deleted, // old value gets deleted color
        }
    }

    /// Blend two colors in linear sRGB space.
    /// `t` ranges from 0.0 (all `a`) to 1.0 (all `b`).
    pub fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
        // Convert to linear sRGB for perceptually correct blending
        let a_lin: LinSrgb =
            Srgb::new(a.0 as f32 / 255.0, a.1 as f32 / 255.0, a.2 as f32 / 255.0).into_linear();
        let b_lin: LinSrgb =
            Srgb::new(b.0 as f32 / 255.0, b.1 as f32 / 255.0, b.2 as f32 / 255.0).into_linear();

        // Mix in linear space
        let mixed = a_lin.mix(b_lin, t);

        // Convert back to sRGB
        let result: Srgb = mixed.into();
        Rgb(
            (result.red * 255.0).round() as u8,
            (result.green * 255.0).round() as u8,
            (result.blue * 255.0).round() as u8,
        )
    }

    /// Brighten and saturate a color for use in highlights.
    /// Increases both lightness and saturation in LCH space.
    pub fn brighten_saturate(rgb: Rgb, lightness_boost: f32, chroma_boost: f32) -> Rgb {
        let srgb = Srgb::new(
            rgb.0 as f32 / 255.0,
            rgb.1 as f32 / 255.0,
            rgb.2 as f32 / 255.0,
        );
        let mut lch = Lch::from_color(srgb);

        // Increase lightness
        lch.l = (lch.l + lightness_boost * 100.0).min(100.0);

        // Increase chroma (saturation-like)
        lch.chroma = (lch.chroma + chroma_boost).min(150.0);

        let result: Srgb = Srgb::from_color(lch);
        Rgb(
            (result.red * 255.0).round() as u8,
            (result.green * 255.0).round() as u8,
            (result.blue * 255.0).round() as u8,
        )
    }

    /// Desaturate a color for use in backgrounds.
    /// Reduces saturation (chroma) in LCH space.
    pub fn desaturate(rgb: Rgb, amount: f32) -> Rgb {
        let srgb = Srgb::new(
            rgb.0 as f32 / 255.0,
            rgb.1 as f32 / 255.0,
            rgb.2 as f32 / 255.0,
        );
        let mut lch = Lch::from_color(srgb);

        // Reduce chroma (saturation)
        lch.chroma *= 1.0 - amount;

        let result: Srgb = Srgb::from_color(lch);
        Rgb(
            (result.red * 255.0).round() as u8,
            (result.green * 255.0).round() as u8,
            (result.blue * 255.0).round() as u8,
        )
    }

    /// Get the key color blended for a deleted context.
    pub fn deleted_key(&self) -> Rgb {
        Self::blend(self.key, self.deleted, 0.5)
    }

    /// Get the key color blended for an inserted context.
    pub fn inserted_key(&self) -> Rgb {
        Self::blend(self.key, self.inserted, 0.5)
    }

    /// Get the structure color blended for a deleted context.
    pub fn deleted_structure(&self) -> Rgb {
        Self::blend(self.structure, self.deleted, 0.4)
    }

    /// Get the structure color blended for an inserted context.
    pub fn inserted_structure(&self) -> Rgb {
        Self::blend(self.structure, self.inserted, 0.4)
    }

    /// Get the comment color blended for a deleted context.
    pub fn deleted_comment(&self) -> Rgb {
        Self::blend(self.comment, self.deleted, 0.35)
    }

    /// Get the comment color blended for an inserted context.
    pub fn inserted_comment(&self) -> Rgb {
        Self::blend(self.comment, self.inserted, 0.35)
    }

    // === Value type blending methods ===

    /// Get the string color blended for a deleted context.
    pub fn deleted_string(&self) -> Rgb {
        Self::blend(self.string, self.deleted, 0.7)
    }

    /// Get the string color blended for an inserted context.
    pub fn inserted_string(&self) -> Rgb {
        Self::blend(self.string, self.inserted, 0.7)
    }

    /// Get the number color blended for a deleted context.
    pub fn deleted_number(&self) -> Rgb {
        Self::blend(self.number, self.deleted, 0.7)
    }

    /// Get the number color blended for an inserted context.
    pub fn inserted_number(&self) -> Rgb {
        Self::blend(self.number, self.inserted, 0.7)
    }

    /// Get the boolean color blended for a deleted context.
    pub fn deleted_boolean(&self) -> Rgb {
        Self::blend(self.boolean, self.deleted, 0.7)
    }

    /// Get the boolean color blended for an inserted context.
    pub fn inserted_boolean(&self) -> Rgb {
        Self::blend(self.boolean, self.inserted, 0.7)
    }

    /// Get the null color blended for a deleted context.
    pub fn deleted_null(&self) -> Rgb {
        Self::blend(self.null, self.deleted, 0.7)
    }

    /// Get the null color blended for an inserted context.
    pub fn inserted_null(&self) -> Rgb {
        Self::blend(self.null, self.inserted, 0.7)
    }

    // === Bright highlight colors for values with highlight backgrounds ===

    /// Get the string color for a deleted highlight (brightened and saturated accent color).
    pub fn deleted_highlight_string(&self) -> Rgb {
        Self::brighten_saturate(self.deleted, 0.15, 0.2)
    }

    /// Get the string color for an inserted highlight (brightened and saturated accent color).
    pub fn inserted_highlight_string(&self) -> Rgb {
        Self::brighten_saturate(self.inserted, 0.15, 0.2)
    }

    /// Get the number color for a deleted highlight (brightened and saturated accent color).
    pub fn deleted_highlight_number(&self) -> Rgb {
        Self::brighten_saturate(self.deleted, 0.15, 0.2)
    }

    /// Get the number color for an inserted highlight (brightened and saturated accent color).
    pub fn inserted_highlight_number(&self) -> Rgb {
        Self::brighten_saturate(self.inserted, 0.15, 0.2)
    }

    /// Get the boolean color for a deleted highlight (brightened and saturated accent color).
    pub fn deleted_highlight_boolean(&self) -> Rgb {
        Self::brighten_saturate(self.deleted, 0.15, 0.2)
    }

    /// Get the boolean color for an inserted highlight (brightened and saturated accent color).
    pub fn inserted_highlight_boolean(&self) -> Rgb {
        Self::brighten_saturate(self.inserted, 0.15, 0.2)
    }

    /// Get the null color for a deleted highlight (brightened and saturated accent color).
    pub fn deleted_highlight_null(&self) -> Rgb {
        Self::brighten_saturate(self.deleted, 0.15, 0.2)
    }

    /// Get the null color for an inserted highlight (brightened and saturated accent color).
    pub fn inserted_highlight_null(&self) -> Rgb {
        Self::brighten_saturate(self.inserted, 0.15, 0.2)
    }

    // === Syntax highlight colors (keys, structure, comments with brightened accents) ===

    /// Get the key color for a deleted highlight (brightened and saturated accent color).
    pub fn deleted_highlight_key(&self) -> Rgb {
        Self::brighten_saturate(self.deleted, 0.15, 0.2)
    }

    /// Get the key color for an inserted highlight (brightened and saturated accent color).
    pub fn inserted_highlight_key(&self) -> Rgb {
        Self::brighten_saturate(self.inserted, 0.15, 0.2)
    }

    /// Get the structure color for a deleted highlight (brightened and saturated accent color).
    pub fn deleted_highlight_structure(&self) -> Rgb {
        Self::brighten_saturate(self.deleted, 0.15, 0.2)
    }

    /// Get the structure color for an inserted highlight (brightened and saturated accent color).
    pub fn inserted_highlight_structure(&self) -> Rgb {
        Self::brighten_saturate(self.inserted, 0.15, 0.2)
    }

    /// Get the comment color for a deleted highlight (brightened and saturated accent color).
    pub fn deleted_highlight_comment(&self) -> Rgb {
        Self::brighten_saturate(self.deleted, 0.15, 0.2)
    }

    /// Get the comment color for an inserted highlight (brightened and saturated accent color).
    pub fn inserted_highlight_comment(&self) -> Rgb {
        Self::brighten_saturate(self.inserted, 0.15, 0.2)
    }

    // === Desaturated background getters ===

    /// Get desaturated deleted line background (more saturated ambient, darker context).
    pub fn desaturated_deleted_line_bg(&self) -> Option<Rgb> {
        self.deleted_line_bg.map(|bg| Self::desaturate(bg, 0.2))
    }

    /// Get desaturated inserted line background (more saturated ambient, darker context).
    pub fn desaturated_inserted_line_bg(&self) -> Option<Rgb> {
        self.inserted_line_bg.map(|bg| Self::desaturate(bg, 0.2))
    }

    /// Get desaturated moved line background (more saturated ambient, darker context).
    pub fn desaturated_moved_line_bg(&self) -> Option<Rgb> {
        self.moved_line_bg.map(|bg| Self::desaturate(bg, 0.2))
    }

    /// Get desaturated deleted highlight background (very desaturated, minimal brightness boost).
    pub fn desaturated_deleted_highlight_bg(&self) -> Option<Rgb> {
        self.deleted_highlight_bg.map(|bg| {
            let brightened = Self::brighten_saturate(bg, 0.02, -5.0);
            Self::desaturate(brightened, 0.75)
        })
    }

    /// Get desaturated inserted highlight background (very desaturated, minimal brightness boost).
    pub fn desaturated_inserted_highlight_bg(&self) -> Option<Rgb> {
        self.inserted_highlight_bg.map(|bg| {
            let brightened = Self::brighten_saturate(bg, 0.02, -5.0);
            Self::desaturate(brightened, 0.75)
        })
    }

    /// Get desaturated moved highlight background (very desaturated, minimal brightness boost).
    pub fn desaturated_moved_highlight_bg(&self) -> Option<Rgb> {
        self.moved_highlight_bg.map(|bg| {
            let brightened = Self::brighten_saturate(bg, 0.02, -5.0);
            Self::desaturate(brightened, 0.75)
        })
    }
}
