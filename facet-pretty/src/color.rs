//! Color generation utilities for pretty-printing

use core::hash::{Hash, Hasher};
use std::hash::DefaultHasher;

/// RGB color representation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RGB {
    /// Red component (0-255)
    pub r: u8,
    /// Green component (0-255)
    pub g: u8,
    /// Blue component (0-255)
    pub b: u8,
}

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

/// A color generator that produces unique colors based on a hash value
#[derive(Clone)]
pub struct ColorGenerator {
    base_hue: f32,
    saturation: f32,
    lightness: f32,
}

impl Default for ColorGenerator {
    fn default() -> Self {
        Self {
            base_hue: 210.0,
            saturation: 0.7,
            lightness: 0.6,
        }
    }
}

impl ColorGenerator {
    /// Create a new color generator with default settings
    pub fn new() -> Self {
        Self::default()
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
    pub fn generate_color(&self, hash: u64) -> RGB {
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
    fn hsl_to_rgb(&self, h: f32, s: f32, l: f32) -> RGB {
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
    fn test_color_generator_default() {
        let generator = ColorGenerator::default();
        assert_eq!(generator.base_hue, 210.0);
        assert_eq!(generator.saturation, 0.7);
        assert_eq!(generator.lightness, 0.6);
    }

    #[test]
    fn test_color_generator_with_methods() {
        let generator = ColorGenerator::new()
            .with_base_hue(180.0)
            .with_saturation(0.5)
            .with_lightness(0.7);

        assert_eq!(generator.base_hue, 180.0);
        assert_eq!(generator.saturation, 0.5);
        assert_eq!(generator.lightness, 0.7);
    }

    #[test]
    fn test_saturation_clamping() {
        let generator = ColorGenerator::new().with_saturation(1.5);
        assert_eq!(generator.saturation, 1.0);

        let generator = ColorGenerator::new().with_saturation(-0.5);
        assert_eq!(generator.saturation, 0.0);
    }

    #[test]
    fn test_lightness_clamping() {
        let generator = ColorGenerator::new().with_lightness(1.5);
        assert_eq!(generator.lightness, 1.0);

        let generator = ColorGenerator::new().with_lightness(-0.5);
        assert_eq!(generator.lightness, 0.0);
    }

    #[test]
    fn test_generate_color() {
        let generator = ColorGenerator::default();

        // Same hash should produce same color
        let color1 = generator.generate_color(42);
        let color2 = generator.generate_color(42);
        assert_eq!(color1, color2);

        // Different hashes should produce different colors
        let color3 = generator.generate_color(100);
        assert_ne!(color1, color3);
    }

    #[test]
    fn test_generate_color_for() {
        let generator = ColorGenerator::default();

        // Same value should produce same color
        let color1 = generator.generate_color_for(&"test");
        let color2 = generator.generate_color_for(&"test");
        assert_eq!(color1, color2);

        // Different values should produce different colors
        let color3 = generator.generate_color_for(&"other");
        assert_ne!(color1, color3);
    }
}
