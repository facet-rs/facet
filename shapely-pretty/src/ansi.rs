//! ANSI color codes and formatting utilities for direct writing to formatters

use std::fmt::Write;

/// Write ANSI reset code to the formatter
pub fn write_reset<W: Write>(f: &mut W) -> std::fmt::Result {
    f.write_str("\x1b[0m")
}

/// Write ANSI bold formatting to the formatter
pub fn write_bold<W: Write>(f: &mut W) -> std::fmt::Result {
    f.write_str("\x1b[1m")
}

/// ANSI reset code
pub const RESET: &str = "\x1b[0m";

/// ANSI bold formatting code
pub const BOLD: &str = "\x1b[1m";

/// ANSI dim formatting code
pub const DIM: &str = "\x1b[2m";

/// Write ANSI dim formatting to the formatter
pub fn write_dim<W: Write>(f: &mut W) -> std::fmt::Result {
    f.write_str(DIM)
}

/// Write ANSI italic formatting to the formatter
pub fn write_italic<W: Write>(f: &mut W) -> std::fmt::Result {
    f.write_str("\x1b[3m")
}

/// Write ANSI underline formatting to the formatter
pub fn write_underline<W: Write>(f: &mut W) -> std::fmt::Result {
    f.write_str("\x1b[4m")
}

/// Write RGB foreground color code to the formatter
pub fn write_rgb<W: Write>(f: &mut W, r: u8, g: u8, b: u8) -> std::fmt::Result {
    write!(f, "\x1b[38;2;{};{};{}m", r, g, b)
}

/// Write RGB background color code to the formatter
pub fn write_rgb_bg<W: Write>(f: &mut W, r: u8, g: u8, b: u8) -> std::fmt::Result {
    write!(f, "\x1b[48;2;{};{};{}m", r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_rgb() {
        let mut out = String::new();
        write_rgb(&mut out, 255, 0, 0).unwrap();
        assert_eq!(out, "\x1b[38;2;255;0;0m");

        let mut out = String::new();
        write_rgb(&mut out, 0, 255, 0).unwrap();
        assert_eq!(out, "\x1b[38;2;0;255;0m");

        let mut out = String::new();
        write_rgb(&mut out, 0, 0, 255).unwrap();
        assert_eq!(out, "\x1b[38;2;0;0;255m");
    }

    #[test]
    fn test_write_rgb_bg() {
        let mut out = String::new();
        write_rgb_bg(&mut out, 255, 0, 0).unwrap();
        assert_eq!(out, "\x1b[48;2;255;0;0m");

        let mut out = String::new();
        write_rgb_bg(&mut out, 0, 255, 0).unwrap();
        assert_eq!(out, "\x1b[48;2;0;255;0m");

        let mut out = String::new();
        write_rgb_bg(&mut out, 0, 0, 255).unwrap();
        assert_eq!(out, "\x1b[48;2;0;0;255m");
    }

    #[test]
    fn test_write_formatting() {
        let mut out = String::new();
        write_bold(&mut out).unwrap();
        assert_eq!(out, "\x1b[1m");

        let mut out = String::new();
        write_dim(&mut out).unwrap();
        assert_eq!(out, "\x1b[2m");

        let mut out = String::new();
        write_reset(&mut out).unwrap();
        assert_eq!(out, "\x1b[0m");
    }
}
