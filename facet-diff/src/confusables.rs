//! Detection and display of visually confusable strings.
//!
//! When two strings look identical but differ in invisible or confusable characters
//! (like NBSP vs regular space), this module helps identify and display those differences.

use std::fmt::Write;

/// Normalize a string to its "visual canonical form" for comparison.
///
/// This converts all visually similar characters to a canonical form:
/// - All whitespace-like characters become regular space
/// - Zero-width characters are removed
/// - Line endings are normalized to \n
pub fn visual_normalize(s: &str) -> String {
    let mut result = String::with_capacity(s.len());

    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Various space characters → regular space
            '\u{00A0}' // NO-BREAK SPACE
            | '\u{2000}' // EN QUAD
            | '\u{2001}' // EM QUAD
            | '\u{2002}' // EN SPACE
            | '\u{2003}' // EM SPACE
            | '\u{2004}' // THREE-PER-EM SPACE
            | '\u{2005}' // FOUR-PER-EM SPACE
            | '\u{2006}' // SIX-PER-EM SPACE
            | '\u{2007}' // FIGURE SPACE
            | '\u{2008}' // PUNCTUATION SPACE
            | '\u{2009}' // THIN SPACE
            | '\u{200A}' // HAIR SPACE
            | '\u{202F}' // NARROW NO-BREAK SPACE
            | '\u{205F}' // MEDIUM MATHEMATICAL SPACE
            | '\u{3000}' // IDEOGRAPHIC SPACE
            => result.push(' '),

            // Zero-width characters → removed
            '\u{200B}' // ZERO WIDTH SPACE
            | '\u{200C}' // ZERO WIDTH NON-JOINER
            | '\u{200D}' // ZERO WIDTH JOINER
            | '\u{FEFF}' // BOM / ZERO WIDTH NO-BREAK SPACE
            | '\u{2060}' // WORD JOINER
            => {}

            // Normalize line endings
            '\r' => {
                // \r\n → \n, \r alone → \n
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                result.push('\n');
            }

            // Everything else passes through
            _ => result.push(c),
        }
    }

    result
}

/// Check if two strings are "visually confusable" - they look the same but differ.
pub fn are_visually_confusable(a: &str, b: &str) -> bool {
    a != b && visual_normalize(a) == visual_normalize(b)
}

/// Information about a character difference at a specific position.
#[derive(Debug, Clone)]
pub struct CharDiff {
    /// Position in the string (character index, not byte)
    pub position: usize,
    /// The character in the "from" string
    pub from_char: char,
    /// The character in the "to" string (may be different position due to zero-width chars)
    pub to_char: char,
}

/// Find all positions where two visually confusable strings differ.
///
/// Returns None if the strings are not visually confusable (either equal or visually different).
pub fn find_confusable_differences(from: &str, to: &str) -> Option<Vec<CharDiff>> {
    if !are_visually_confusable(from, to) {
        return None;
    }

    let mut diffs = Vec::new();
    let from_chars: Vec<char> = from.chars().collect();
    let to_chars: Vec<char> = to.chars().collect();

    // Walk through both strings tracking visual position
    let mut from_idx = 0;
    let mut to_idx = 0;
    let mut visual_pos = 0;

    while from_idx < from_chars.len() || to_idx < to_chars.len() {
        let from_c = from_chars.get(from_idx).copied();
        let to_c = to_chars.get(to_idx).copied();

        match (from_c, to_c) {
            (Some(fc), Some(tc)) => {
                let fc_visual = visual_char(fc);
                let tc_visual = visual_char(tc);

                match (fc_visual, tc_visual) {
                    (Some(fv), Some(tv)) if fv == tv => {
                        // Both have visual representation and they match
                        if fc != tc {
                            // But the actual characters differ!
                            diffs.push(CharDiff {
                                position: visual_pos,
                                from_char: fc,
                                to_char: tc,
                            });
                        }
                        from_idx += 1;
                        to_idx += 1;
                        visual_pos += 1;
                    }
                    (None, Some(_)) => {
                        // from has zero-width, to has visible
                        // This shouldn't happen for confusable strings, but handle it
                        from_idx += 1;
                    }
                    (Some(_), None) => {
                        // from has visible, to has zero-width
                        to_idx += 1;
                    }
                    (None, None) => {
                        // Both zero-width
                        from_idx += 1;
                        to_idx += 1;
                    }
                    _ => {
                        // Visual mismatch - shouldn't happen for confusable strings
                        from_idx += 1;
                        to_idx += 1;
                        visual_pos += 1;
                    }
                }
            }
            (Some(fc), None) => {
                // from has extra character
                if visual_char(fc).is_some() {
                    visual_pos += 1;
                }
                from_idx += 1;
            }
            (None, Some(tc)) => {
                // to has extra character
                if visual_char(tc).is_some() {
                    visual_pos += 1;
                }
                to_idx += 1;
            }
            (None, None) => break,
        }
    }

    Some(diffs)
}

/// Get the visual representation of a character, or None if it's zero-width.
fn visual_char(c: char) -> Option<char> {
    match c {
        // Zero-width characters
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{2060}' => None,

        // Space-like characters all normalize to space
        '\u{00A0}' | '\u{2000}' | '\u{2001}' | '\u{2002}' | '\u{2003}' | '\u{2004}'
        | '\u{2005}' | '\u{2006}' | '\u{2007}' | '\u{2008}' | '\u{2009}' | '\u{200A}'
        | '\u{202F}' | '\u{205F}' | '\u{3000}' => Some(' '),

        // Everything else is itself
        _ => Some(c),
    }
}

/// Get a human-readable name for a character, especially for invisible/confusable ones.
pub fn char_name(c: char) -> &'static str {
    match c {
        ' ' => "SPACE",
        '\t' => "TAB",
        '\n' => "LINE FEED",
        '\r' => "CARRIAGE RETURN",
        '\u{00A0}' => "NO-BREAK SPACE",
        '\u{2000}' => "EN QUAD",
        '\u{2001}' => "EM QUAD",
        '\u{2002}' => "EN SPACE",
        '\u{2003}' => "EM SPACE",
        '\u{2004}' => "THREE-PER-EM SPACE",
        '\u{2005}' => "FOUR-PER-EM SPACE",
        '\u{2006}' => "SIX-PER-EM SPACE",
        '\u{2007}' => "FIGURE SPACE",
        '\u{2008}' => "PUNCTUATION SPACE",
        '\u{2009}' => "THIN SPACE",
        '\u{200A}' => "HAIR SPACE",
        '\u{200B}' => "ZERO WIDTH SPACE",
        '\u{200C}' => "ZERO WIDTH NON-JOINER",
        '\u{200D}' => "ZERO WIDTH JOINER",
        '\u{202F}' => "NARROW NO-BREAK SPACE",
        '\u{205F}' => "MEDIUM MATHEMATICAL SPACE",
        '\u{2060}' => "WORD JOINER",
        '\u{3000}' => "IDEOGRAPHIC SPACE",
        '\u{FEFF}' => "BYTE ORDER MARK",
        _ => "",
    }
}

/// Format a character for display, showing its escape sequence and name if special.
pub fn format_char(c: char) -> String {
    let name = char_name(c);
    if !name.is_empty() {
        format!("'\\u{{{:04X}}}' ({})", c as u32, name)
    } else if c.is_control() || !c.is_ascii_graphic() && !c.is_ascii_whitespace() {
        format!("'\\u{{{:04X}}}'", c as u32)
    } else {
        format!("'{}'", c)
    }
}

/// Format a detailed explanation of the differences between two confusable strings.
pub fn format_confusable_diff(from: &str, to: &str) -> Option<String> {
    let diffs = find_confusable_differences(from, to)?;

    if diffs.is_empty() {
        return None;
    }

    let mut output = String::new();
    writeln!(
        output,
        "(strings appear identical but differ in {} position{})",
        diffs.len(),
        if diffs.len() == 1 { "" } else { "s" }
    )
    .ok()?;

    for diff in &diffs {
        writeln!(
            output,
            "  [{}]: {} → {}",
            diff.position,
            format_char(diff.from_char),
            format_char(diff.to_char)
        )
        .ok()?;
    }

    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visual_normalize_nbsp() {
        let with_nbsp = "hello\u{00A0}world";
        let with_space = "hello world";
        assert_eq!(visual_normalize(with_nbsp), visual_normalize(with_space));
    }

    #[test]
    fn test_visual_normalize_zero_width() {
        let with_zwsp = "hello\u{200B}world";
        let without = "helloworld";
        assert_eq!(visual_normalize(with_zwsp), visual_normalize(without));
    }

    #[test]
    fn test_visual_normalize_line_endings() {
        assert_eq!(visual_normalize("a\r\nb"), visual_normalize("a\nb"));
        assert_eq!(visual_normalize("a\rb"), visual_normalize("a\nb"));
    }

    #[test]
    fn test_are_visually_confusable() {
        // NBSP vs space
        assert!(are_visually_confusable("hello\u{00A0}world", "hello world"));

        // Different content is not confusable
        assert!(!are_visually_confusable("hello", "world"));

        // Identical strings are not confusable (they're equal)
        assert!(!are_visually_confusable("hello", "hello"));
    }

    #[test]
    fn test_find_confusable_differences() {
        let from = "bug\u{00A0}report";
        let to = "bug report";

        let diffs = find_confusable_differences(from, to).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].position, 3);
        assert_eq!(diffs[0].from_char, '\u{00A0}');
        assert_eq!(diffs[0].to_char, ' ');
    }

    #[test]
    fn test_find_multiple_confusable_differences() {
        let from = "a\u{00A0}b\u{00A0}c";
        let to = "a b c";

        let diffs = find_confusable_differences(from, to).unwrap();
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0].position, 1);
        assert_eq!(diffs[1].position, 3);
    }

    #[test]
    fn test_char_name() {
        assert_eq!(char_name('\u{00A0}'), "NO-BREAK SPACE");
        assert_eq!(char_name(' '), "SPACE");
        assert_eq!(char_name('a'), "");
    }

    #[test]
    fn test_format_char() {
        assert_eq!(format_char(' '), "'\\u{0020}' (SPACE)");
        assert_eq!(format_char('\u{00A0}'), "'\\u{00A0}' (NO-BREAK SPACE)");
        assert_eq!(format_char('a'), "'a'");
    }

    #[test]
    fn test_format_confusable_diff() {
        let from = "bug\u{00A0}report";
        let to = "bug report";

        let output = format_confusable_diff(from, to).unwrap();
        assert!(output.contains("appear identical"));
        assert!(output.contains("NO-BREAK SPACE"));
        assert!(output.contains("SPACE"));
    }
}
