//! Hex-dump rendering and diffing for byte buffers.
//!
//! When both sides of a diff are `[u8]`-like (e.g. `Vec<u8>`, `&[u8]`,
//! `[u8; N]`), rediff renders them as a classic `xxd`-style hex dump and
//! diffs them row by row, instead of doing an element-wise decimal diff.

use facet_reflect::Peek;

/// Number of bytes shown per hex-dump row.
const ROW: usize = 16;

/// The role a hex-dump line plays in a byte diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HexLineKind {
    /// A row present in the `from` buffer that changed or was removed.
    Removed,
    /// A row present in the `to` buffer that changed or was added.
    Added,
    /// A collapsed run of `usize` identical rows.
    Collapsed(usize),
}

/// A single rendered line of a hex-dump diff.
#[derive(Debug, Clone)]
pub(crate) struct HexLine {
    /// What kind of line this is (drives the marker and color).
    pub kind: HexLineKind,
    /// The rendered text (without the leading `-`/`+` marker).
    pub text: String,
}

/// Extract a contiguous byte buffer from a `Peek`, if it is one.
///
/// Handles `&[u8]`, `Vec<u8>`, `[u8; N]` and slices, plus dynamic
/// values whose kind is `Bytes`.
pub(crate) fn peek_to_bytes(peek: Peek<'_, '_>) -> Option<Vec<u8>> {
    if let Some(bytes) = peek.as_bytes() {
        return Some(bytes.to_vec());
    }

    if let Ok(list) = peek.into_list_like()
        && list.def().t().is_type::<u8>()
    {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            out.push(*item.get::<u8>().ok()?);
        }
        return Some(out);
    }

    if let Ok(dyn_val) = peek.into_dynamic_value()
        && let Some(bytes) = dyn_val.as_bytes()
    {
        return Some(bytes.to_vec());
    }

    None
}

/// Returns `true` if both peeks are byte buffers (and at least one is
/// non-trivially a byte buffer, so we don't hijack empty generic lists).
pub(crate) fn is_byte_peek(peek: Peek<'_, '_>) -> bool {
    if peek.as_bytes().is_some() {
        return true;
    }
    if let Ok(list) = peek.into_list_like() {
        return list.def().t().is_type::<u8>();
    }
    if let Ok(dyn_val) = peek.into_dynamic_value() {
        return dyn_val.as_bytes().is_some();
    }
    false
}

/// Render a single hex-dump row: `00000000  de ad be ef …  |dead|`.
///
/// `offset` is the byte offset of the first byte in `bytes`, which must
/// contain at most [`ROW`] elements.
fn format_row(offset: usize, bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut s = String::with_capacity(8 + 2 + ROW * 3 + 4 + ROW);
    write!(s, "{offset:08x} ").unwrap();

    for i in 0..ROW {
        // Extra space between the two 8-byte halves for readability.
        if i % 8 == 0 {
            s.push(' ');
        }
        match bytes.get(i) {
            Some(b) => write!(s, "{b:02x} ").unwrap(),
            None => s.push_str("   "),
        }
    }

    s.push_str(" |");
    for &b in bytes {
        s.push(if (0x20..=0x7e).contains(&b) {
            b as char
        } else {
            '.'
        });
    }
    s.push('|');
    s
}

/// Diff two byte buffers row by row, collapsing runs of identical rows.
///
/// Rows are aligned by offset (like `diff <(xxd a) <(xxd b)`): a changed
/// 16-byte window shows the old row as [`HexLineKind::Removed`] followed
/// by the new row as [`HexLineKind::Added`].
pub(crate) fn diff_hex(from: &[u8], to: &[u8]) -> Vec<HexLine> {
    let mut lines = Vec::new();
    let row_count = from.len().div_ceil(ROW).max(to.len().div_ceil(ROW));

    let mut equal_run = 0usize;
    let flush_equal = |lines: &mut Vec<HexLine>, run: &mut usize| {
        if *run > 0 {
            let n = *run;
            let label = if n == 1 { "row" } else { "rows" };
            lines.push(HexLine {
                kind: HexLineKind::Collapsed(n),
                text: format!(".. {n} unchanged {label}"),
            });
            *run = 0;
        }
    };

    for row in 0..row_count {
        let start = row * ROW;
        let fr = from.get(start..(start + ROW).min(from.len()).max(start));
        let tr = to.get(start..(start + ROW).min(to.len()).max(start));

        match (fr, tr) {
            (Some(f), Some(t)) if f == t => {
                equal_run += 1;
            }
            (f, t) => {
                flush_equal(&mut lines, &mut equal_run);
                if let Some(f) = f.filter(|f| !f.is_empty()) {
                    lines.push(HexLine {
                        kind: HexLineKind::Removed,
                        text: format_row(start, f),
                    });
                }
                if let Some(t) = t.filter(|t| !t.is_empty()) {
                    lines.push(HexLine {
                        kind: HexLineKind::Added,
                        text: format_row(start, t),
                    });
                }
            }
        }
    }

    flush_equal(&mut lines, &mut equal_run);
    lines
}

/// Render a hex-dump diff of two buffers as a multi-line string.
///
/// Each line is prefixed with `-`/`+`/` ` and, when `colors` is set,
/// colored with the same Tokyo Night palette used elsewhere in rediff.
/// The block has no trailing newline.
pub(crate) fn render_hex_diff(from: &[u8], to: &[u8], colors: bool) -> String {
    use facet_pretty::tokyo_night;
    use owo_colors::OwoColorize;

    let mut out = String::new();
    for (i, line) in diff_hex(from, to).iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let marker = match line.kind {
            HexLineKind::Removed => "- ",
            HexLineKind::Added => "+ ",
            HexLineKind::Collapsed(_) => "  ",
        };
        let row = format!("{marker}{}", line.text);
        if colors {
            let color = match line.kind {
                HexLineKind::Removed => tokyo_night::DELETION,
                HexLineKind::Added => tokyo_night::INSERTION,
                HexLineKind::Collapsed(_) => tokyo_night::MUTED,
            };
            out.push_str(&row.color(color).to_string());
        } else {
            out.push_str(&row);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_byte_change() {
        let lines = diff_hex(&[0xde, 0xad, 0xbe, 0xef], &[0xde, 0xad, 0xca, 0xfe]);
        let kinds: Vec<_> = lines.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, vec![HexLineKind::Removed, HexLineKind::Added]);
        assert!(lines[0].text.contains("de ad be ef"));
        assert!(lines[1].text.contains("de ad ca fe"));
    }

    #[test]
    fn collapses_equal_rows() {
        let from: Vec<u8> = (0..64).collect();
        let mut to = from.clone();
        *to.last_mut().unwrap() = 0xff;
        let lines = diff_hex(&from, &to);
        // 3 identical leading rows collapse, then the changed row pair.
        assert_eq!(lines[0].kind, HexLineKind::Collapsed(3));
        assert_eq!(lines[1].kind, HexLineKind::Removed);
        assert_eq!(lines[2].kind, HexLineKind::Added);
    }

    #[test]
    fn ascii_gutter() {
        let lines = diff_hex(b"hello", b"world");
        assert!(lines[0].text.ends_with("|hello|"));
        assert!(lines[1].text.ends_with("|world|"));
    }
}
