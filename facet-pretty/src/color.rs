//! Color palettes and terminal theme detection for pretty-printing.
//!
//! The palette is inspired by [Melange](https://github.com/savq/melange-nvim)
//! by Sergio Alquérèque — a warm, low-contrast colour scheme with matched
//! dark and light variants. By default ([`Theme::Auto`]) the terminal
//! background is detected once and the appropriate variant is used.

use core::hash::{Hash, Hasher};
use owo_colors::Rgb;
use std::hash::DefaultHasher;
use std::sync::LazyLock;

/// A complete set of semantic colours used by the pretty-printer.
///
/// Each field maps a syntactic role to an RGB colour. Two faithful Melange
/// variants are provided as [`Palette::MELANGE_DARK`] and
/// [`Palette::MELANGE_LIGHT`]; custom palettes can be supplied via
/// [`Theme::Custom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Default text (struct/enum field names, plain identifiers).
    pub foreground: Rgb,
    /// Type names (struct, enum, container names) — rendered bold.
    pub type_name: Rgb,
    /// Struct field / map key labels.
    pub field_name: Rgb,
    /// String literals.
    pub string: Rgb,
    /// Numeric literals.
    pub number: Rgb,
    /// Keywords such as `true`, `false`, `null`.
    pub keyword: Rgb,
    /// Comments and elided/omitted markers.
    pub comment: Rgb,
    /// Punctuation and structural delimiters (braces, colons, commas).
    pub punctuation: Rgb,
    /// Redacted values and error markers.
    pub error: Rgb,
    /// Removed content (diffs).
    pub deletion: Rgb,
    /// Added content (diffs).
    pub insertion: Rgb,
    /// Distinct accents used to give each scalar type its own hue.
    pub accents: [Rgb; 6],
}

impl Palette {
    /// Melange dark — for terminals with a dark background.
    ///
    /// Values are taken verbatim from the official `melange-nvim` dark
    /// palette and mapped to syntactic roles following its highlight groups.
    pub const MELANGE_DARK: Self = Self {
        foreground: Rgb(236, 225, 215), // a.fg  #ECE1D7
        type_name: Rgb(123, 150, 149),  // c.cyan #7B9695 (Type)
        field_name: Rgb(236, 225, 215), // a.fg  #ECE1D7 (Identifier)
        string: Rgb(163, 169, 206),     // b.blue #A3A9CE (String)
        number: Rgb(207, 155, 194),     // b.magenta #CF9BC2 (Number)
        keyword: Rgb(207, 155, 194),    // b.magenta #CF9BC2 (Boolean)
        comment: Rgb(193, 167, 142),    // a.com #C1A78E (Comment)
        punctuation: Rgb(134, 116, 98), // a.ui  #867462 (muted UI)
        error: Rgb(212, 119, 102),      // b.red #D47766
        deletion: Rgb(189, 129, 131),   // c.red #BD8183
        insertion: Rgb(133, 182, 149),  // b.green #85B695
        accents: [
            Rgb(212, 119, 102), // b.red     #D47766
            Rgb(235, 192, 109), // b.yellow  #EBC06D
            Rgb(133, 182, 149), // b.green   #85B695
            Rgb(137, 179, 182), // b.cyan    #89B3B6
            Rgb(163, 169, 206), // b.blue    #A3A9CE
            Rgb(207, 155, 194), // b.magenta #CF9BC2
        ],
    };

    /// Melange light — for terminals with a light background.
    ///
    /// Values are taken verbatim from the official `melange-nvim` light
    /// palette and mapped to syntactic roles following its highlight groups.
    pub const MELANGE_LIGHT: Self = Self {
        foreground: Rgb(84, 67, 58),     // a.fg  #54433A
        type_name: Rgb(115, 151, 151),   // c.cyan #739797 (Type)
        field_name: Rgb(84, 67, 58),     // a.fg  #54433A (Identifier)
        string: Rgb(70, 90, 164),        // b.blue #465AA4 (String)
        number: Rgb(144, 65, 128),       // b.magenta #904180 (Number)
        keyword: Rgb(144, 65, 128),      // b.magenta #904180 (Boolean)
        comment: Rgb(125, 102, 88),      // a.com #7D6658 (Comment)
        punctuation: Rgb(169, 138, 120), // a.ui  #A98A78 (muted UI)
        error: Rgb(191, 0, 33),          // b.red #BF0021
        deletion: Rgb(199, 123, 139),    // c.red #C77B8B
        insertion: Rgb(58, 104, 74),     // b.green #3A684A
        accents: [
            Rgb(191, 0, 33),   // b.red     #BF0021
            Rgb(160, 109, 0),  // b.yellow  #A06D00
            Rgb(58, 104, 74),  // b.green   #3A684A
            Rgb(61, 101, 104), // b.cyan    #3D6568
            Rgb(70, 90, 164),  // b.blue    #465AA4
            Rgb(144, 65, 128), // b.magenta #904180
        ],
    };

    /// Pick a stable accent colour for a hash value.
    ///
    /// Used to give each distinct scalar shape its own hue while staying
    /// within the palette's aesthetic.
    pub fn accent(&self, hash: u64) -> Rgb {
        self.accents[(hash % self.accents.len() as u64) as usize]
    }
}

/// Which colour palette the pretty-printer should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Theme {
    /// Detect the terminal background once and pick the matching Melange
    /// variant (light or dark). Falls back to dark when detection is
    /// unavailable.
    #[default]
    Auto,
    /// Always use [`Palette::MELANGE_DARK`].
    Dark,
    /// Always use [`Palette::MELANGE_LIGHT`].
    Light,
    /// Use a caller-supplied palette.
    Custom(Palette),
}

impl Theme {
    /// Resolve this theme to a concrete [`Palette`].
    ///
    /// For [`Theme::Auto`] the result is detected once per process and
    /// cached.
    pub fn palette(&self) -> Palette {
        match self {
            Theme::Auto => detected_palette(),
            Theme::Dark => Palette::MELANGE_DARK,
            Theme::Light => Palette::MELANGE_LIGHT,
            Theme::Custom(palette) => *palette,
        }
    }
}

/// The palette chosen by [`Theme::Auto`], detected once per process.
///
/// Detection order:
/// 1. The `FACET_PRETTY_THEME` environment variable (`dark` / `light`).
/// 2. The terminal background colour, queried via the `terminal-light`
///    crate (only when the `detect-terminal-theme` feature is enabled and
///    stdout is a terminal).
/// 3. Dark, as a safe default.
pub fn detected_palette() -> Palette {
    static DETECTED: LazyLock<Palette> = LazyLock::new(detect);
    *DETECTED
}

fn detect() -> Palette {
    match std::env::var("FACET_PRETTY_THEME") {
        Ok(v) if v.eq_ignore_ascii_case("light") => return Palette::MELANGE_LIGHT,
        Ok(v) if v.eq_ignore_ascii_case("dark") => return Palette::MELANGE_DARK,
        _ => {}
    }

    if terminal_is_light() {
        Palette::MELANGE_LIGHT
    } else {
        Palette::MELANGE_DARK
    }
}

#[cfg(all(feature = "detect-terminal-theme", not(target_arch = "wasm32")))]
fn terminal_is_light() -> bool {
    use std::io::IsTerminal;

    // Querying the terminal writes an OSC escape sequence to stdout and
    // reads the reply from /dev/tty. Only do this when stdout is an
    // interactive terminal, otherwise we would corrupt piped output.
    if !std::io::stdout().is_terminal() {
        return false;
    }

    // luma() returns 0 (black) .. 1 (white); 0.6 is the recommended pivot
    // between "rather dark" and "rather light".
    terminal_light::luma()
        .map(|luma| luma > 0.6)
        .unwrap_or(false)
}

#[cfg(not(all(feature = "detect-terminal-theme", not(target_arch = "wasm32"))))]
fn terminal_is_light() -> bool {
    false
}

/// RGB color representation.
///
/// Deprecated re-export shim: superseded by [`Palette`] /
/// [`owo_colors::Rgb`] after the Melange palette migration. Kept so
/// pre-0.47 callers keep compiling.
#[deprecated(
    since = "0.46.3",
    note = "use `Palette` / `owo_colors::Rgb`; colours now come from `Theme`/`Palette`"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RGB {
    /// Red component (0-255)
    pub r: u8,
    /// Green component (0-255)
    pub g: u8,
    /// Blue component (0-255)
    pub b: u8,
}

#[allow(deprecated)]
impl RGB {
    /// Create a new RGB color
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Write the RGB color as ANSI foreground color code to the formatter
    pub fn write_fg<W: core::fmt::Write>(&self, f: &mut W) -> core::fmt::Result {
        write!(f, "\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    /// Write the RGB color as ANSI background color code to the formatter
    pub fn write_bg<W: core::fmt::Write>(&self, f: &mut W) -> core::fmt::Result {
        write!(f, "\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }
}

/// A color generator that produces unique colors based on a hash value.
///
/// Deprecated re-export shim: colours now come from [`Palette`] / [`Theme`]
/// (`Palette::accent` for per-value hues). Kept so pre-0.47 callers keep
/// compiling.
#[deprecated(
    since = "0.46.3",
    note = "colours now come from `Theme`/`Palette`; use `Palette::accent` for per-value hues"
)]
#[derive(Clone, PartialEq)]
pub struct ColorGenerator {
    base_hue: f32,
    saturation: f32,
    lightness: f32,
}

#[allow(deprecated)]
impl Default for ColorGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(deprecated)]
impl ColorGenerator {
    /// Create a new color generator with default settings
    pub const fn new() -> Self {
        Self {
            base_hue: 210.0,
            saturation: 0.7,
            lightness: 0.6,
        }
    }

    /// Set the base hue (0-360)
    pub const fn with_base_hue(mut self, hue: f32) -> Self {
        self.base_hue = hue;
        self
    }

    /// Set the saturation (0.0-1.0)
    pub const fn with_saturation(mut self, saturation: f32) -> Self {
        self.saturation = saturation.clamp(0.0, 1.0);
        self
    }

    /// Set the lightness (0.0-1.0)
    pub const fn with_lightness(mut self, lightness: f32) -> Self {
        self.lightness = lightness.clamp(0.0, 1.0);
        self
    }

    /// Generate an RGB color based on a hash value
    pub const fn generate_color(&self, hash: u64) -> RGB {
        // Use the hash to generate a hue offset
        let hue_offset = (hash % 360) as f32;
        let hue = (self.base_hue + hue_offset) % 360.0;

        // Convert HSL to RGB
        self.hsl_to_rgb(hue, self.saturation, self.lightness)
    }

    /// Generate an RGB color based on a hashable value
    pub fn generate_color_for<T: Hash>(&self, value: &T) -> RGB {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        let hash = hasher.finish();
        self.generate_color(hash)
    }

    /// Convert HSL color values to RGB
    const fn hsl_to_rgb(&self, h: f32, s: f32, l: f32) -> RGB {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;

        let (r, g, b) = match h as u32 {
            0..=59 => (c, x, 0.0),
            60..=119 => (x, c, 0.0),
            120..=179 => (0.0, c, x),
            180..=239 => (0.0, x, c),
            240..=299 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        RGB::new(
            ((r + m) * 255.0) as u8,
            ((g + m) * 255.0) as u8,
            ((b + m) * 255.0) as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_is_stable_and_in_palette() {
        let p = Palette::MELANGE_DARK;
        assert_eq!(p.accent(42), p.accent(42));
        assert!(p.accents.contains(&p.accent(123_456_789)));
    }

    #[test]
    fn theme_resolves_to_expected_palette() {
        assert_eq!(Theme::Dark.palette(), Palette::MELANGE_DARK);
        assert_eq!(Theme::Light.palette(), Palette::MELANGE_LIGHT);
        let custom = Palette {
            foreground: Rgb(1, 2, 3),
            ..Palette::MELANGE_DARK
        };
        assert_eq!(Theme::Custom(custom).palette(), custom);
    }
}
