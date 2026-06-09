//! Hex-dump rendering and byte-level diffing for byte buffers.
//!
//! When both sides of a diff are `[u8]`-like (e.g. `Vec<u8>`, `&[u8]`,
//! `[u8; N]`), rediff renders them as a classic `xxd`-style hex dump and
//! diffs them at the *byte* level (Myers' algorithm), so it can show
//! exactly which bytes were inserted, deleted or changed — not just which
//! 16-byte rows differ.

use facet_reflect::Peek;

/// Number of bytes shown per hex-dump row.
const ROW: usize = 16;

/// Above this combined size we skip Myers and fall back to a coarse
/// common-prefix/suffix edit script to keep cost bounded.
const MYERS_LIMIT: usize = 1 << 16;

/// A single edit-script operation over the two byte buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// `len` bytes common to both buffers.
    Equal(usize),
    /// `len` bytes present only in the `from` buffer.
    Delete(usize),
    /// `len` bytes present only in the `to` buffer.
    Insert(usize),
}

/// How a single byte cell relates to the other buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cls {
    /// Byte is identical in both buffers (rendered muted).
    Equal,
    /// Byte exists only in `from` (rendered as a deletion).
    Deleted,
    /// Byte exists only in `to` (rendered as an insertion).
    Inserted,
}

/// One byte of a rendered hex-dump row, tagged with its diff class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HexCell {
    /// The byte value.
    pub byte: u8,
    /// Whether this byte is unchanged, deleted or inserted.
    pub cls: Cls,
}

/// Which side of the diff a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// A row from the `from` buffer (`-` gutter).
    Removed,
    /// A row from the `to` buffer (`+` gutter).
    Added,
}

/// A line of hex-dump diff output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HexLine {
    /// A rendered hex-dump row.
    Row {
        /// `from` (`-`) or `to` (`+`) side.
        kind: RowKind,
        /// Absolute byte offset of the first cell.
        offset: usize,
        /// Up to 16 byte cells, each individually classified.
        cells: Vec<HexCell>,
    },
    /// A collapsed run of `usize` unchanged 16-byte rows.
    Collapsed(usize),
}

/// Extract a contiguous byte buffer from a `Peek`, if it is one.
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

/// Returns `true` if the peek is a byte buffer.
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

/// Compute a byte-level edit script between `a` and `b`.
///
/// Uses Myers' O(ND) algorithm, falling back to a coarse
/// common-prefix/suffix script for very large inputs.
fn diff_ops(a: &[u8], b: &[u8]) -> Vec<Op> {
    let n = a.len();
    let m = b.len();

    // Trim the common prefix/suffix first: it's cheap and dramatically
    // shrinks the Myers grid for the common "few bytes changed" case.
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();

    let ma = &a[prefix..n - suffix];
    let mb = &b[prefix..m - suffix];

    let mut ops = Vec::new();
    let push = |ops: &mut Vec<Op>, op: Op| match (ops.last_mut(), op) {
        (Some(Op::Equal(l)), Op::Equal(k)) => *l += k,
        (Some(Op::Delete(l)), Op::Delete(k)) => *l += k,
        (Some(Op::Insert(l)), Op::Insert(k)) => *l += k,
        _ => ops.push(op),
    };

    if prefix > 0 {
        push(&mut ops, Op::Equal(prefix));
    }

    if ma.len() + mb.len() > MYERS_LIMIT {
        // Coarse fallback: replace the whole differing middle.
        if !ma.is_empty() {
            push(&mut ops, Op::Delete(ma.len()));
        }
        if !mb.is_empty() {
            push(&mut ops, Op::Insert(mb.len()));
        }
    } else {
        for op in myers(ma, mb) {
            push(&mut ops, op);
        }
    }

    if suffix > 0 {
        push(&mut ops, Op::Equal(suffix));
    }
    ops
}

/// Myers' shortest-edit-script algorithm over byte slices.
fn myers(a: &[u8], b: &[u8]) -> Vec<Op> {
    let n = a.len() as isize;
    let m = b.len() as isize;
    if n == 0 && m == 0 {
        return Vec::new();
    }
    let max = (n + m) as usize;
    let offset = max as isize;
    let mut v = vec![0isize; 2 * max + 1];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut d_final = 0isize;

    'search: for d in 0..=max as isize {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d
                || (k != d && v[(k - 1 + offset) as usize] < v[(k + 1 + offset) as usize])
            {
                v[(k + 1 + offset) as usize]
            } else {
                v[(k - 1 + offset) as usize] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(k + offset) as usize] = x;
            if x >= n && y >= m {
                d_final = d;
                break 'search;
            }
            k += 2;
        }
    }

    // Backtrack the trace into a (reversed) op list.
    let mut ops_rev: Vec<Op> = Vec::new();
    let push = |ops: &mut Vec<Op>, op: Op| match (ops.last_mut(), op) {
        (Some(Op::Equal(l)), Op::Equal(k)) => *l += k,
        (Some(Op::Delete(l)), Op::Delete(k)) => *l += k,
        (Some(Op::Insert(l)), Op::Insert(k)) => *l += k,
        _ => ops.push(op),
    };

    let mut x = n;
    let mut y = m;
    for d in (0..=d_final).rev() {
        let vv = &trace[d as usize];
        let k = x - y;
        let prev_k = if k == -d
            || (k != d && vv[(k - 1 + offset) as usize] < vv[(k + 1 + offset) as usize])
        {
            k + 1
        } else {
            k - 1
        };
        let prev_x = vv[(prev_k + offset) as usize];
        let prev_y = prev_x - prev_k;

        while x > prev_x && y > prev_y {
            push(&mut ops_rev, Op::Equal(1));
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            if x == prev_x {
                push(&mut ops_rev, Op::Insert(1));
                y -= 1;
            } else {
                push(&mut ops_rev, Op::Delete(1));
                x -= 1;
            }
        }
    }

    ops_rev.reverse();
    // Re-coalesce: reversing can leave adjacent same-kind ops split.
    let mut ops = Vec::with_capacity(ops_rev.len());
    for op in ops_rev {
        push(&mut ops, op);
    }
    ops
}

/// A region of change, expanded to enclosing 16-byte row boundaries.
struct Hunk {
    fa: usize,
    fb: usize,
    ta: usize,
    tb: usize,
}

/// Build a byte-level hex-dump diff: per-byte-classified `-`/`+` rows
/// around each change, with fully-unchanged rows collapsed.
pub(crate) fn diff_hex(from: &[u8], to: &[u8]) -> Vec<HexLine> {
    if from == to {
        return Vec::new();
    }

    let ops = diff_ops(from, to);

    // Per-byte classification of each buffer.
    let mut del = vec![false; from.len()];
    let mut ins = vec![false; to.len()];
    {
        let (mut fi, mut ti) = (0usize, 0usize);
        for op in &ops {
            match *op {
                Op::Equal(l) => {
                    fi += l;
                    ti += l;
                }
                Op::Delete(l) => {
                    del[fi..fi + l].fill(true);
                    fi += l;
                }
                Op::Insert(l) => {
                    ins[ti..ti + l].fill(true);
                    ti += l;
                }
            }
        }
    }

    // Minimal change blocks (maximal runs of non-Equal ops), in both
    // coordinate spaces, then snapped out to 16-byte row boundaries.
    let mut hunks: Vec<Hunk> = Vec::new();
    {
        let (mut fi, mut ti) = (0usize, 0usize);
        let mut open: Option<(usize, usize, usize, usize)> = None;
        let close = |hunks: &mut Vec<Hunk>, blk: (usize, usize, usize, usize)| {
            let (fs, fe, ts, te) = blk;
            let fa = (fs / ROW) * ROW;
            let fb = (fe.div_ceil(ROW) * ROW).min(from.len());
            let ta = (ts / ROW) * ROW;
            let tb = (te.div_ceil(ROW) * ROW).min(to.len());
            // Merge with the previous hunk if the aligned ranges touch.
            if let Some(last) = hunks.last_mut()
                && fa <= last.fb
                && ta <= last.tb
            {
                last.fb = last.fb.max(fb);
                last.tb = last.tb.max(tb);
            } else {
                hunks.push(Hunk { fa, fb, ta, tb });
            }
        };

        for op in &ops {
            match *op {
                Op::Equal(l) => {
                    if let Some(blk) = open.take() {
                        close(&mut hunks, blk);
                    }
                    fi += l;
                    ti += l;
                }
                Op::Delete(l) => {
                    let e = open.get_or_insert((fi, fi, ti, ti));
                    e.1 = fi + l;
                    fi += l;
                }
                Op::Insert(l) => {
                    let e = open.get_or_insert((fi, fi, ti, ti));
                    e.3 = ti + l;
                    ti += l;
                }
            }
        }
        if let Some(blk) = open.take() {
            close(&mut hunks, blk);
        }
    }

    // Emit collapsed gaps + per-hunk `-`/`+` rows.
    let mut lines = Vec::new();
    let mut prev_fb = 0usize;
    for h in &hunks {
        let gap_rows = (h.fa - prev_fb) / ROW;
        if gap_rows > 0 {
            lines.push(HexLine::Collapsed(gap_rows));
        }
        emit_rows(&mut lines, RowKind::Removed, &from[h.fa..h.fb], h.fa, &del);
        emit_rows(&mut lines, RowKind::Added, &to[h.ta..h.tb], h.ta, &ins);
        prev_fb = h.fb;
    }
    let trailing = from.len().saturating_sub(prev_fb).div_ceil(ROW);
    if trailing > 0 {
        lines.push(HexLine::Collapsed(trailing));
    }
    lines
}

/// Chunk a classified byte range into 16-byte [`HexLine::Row`]s.
fn emit_rows(
    lines: &mut Vec<HexLine>,
    kind: RowKind,
    bytes: &[u8],
    start: usize,
    changed: &[bool],
) {
    for (i, chunk) in bytes.chunks(ROW).enumerate() {
        let offset = start + i * ROW;
        let cells = chunk
            .iter()
            .enumerate()
            .map(|(j, &byte)| {
                let idx = offset + j;
                let cls = if changed.get(idx).copied().unwrap_or(false) {
                    match kind {
                        RowKind::Removed => Cls::Deleted,
                        RowKind::Added => Cls::Inserted,
                    }
                } else {
                    Cls::Equal
                };
                HexCell { byte, cls }
            })
            .collect();
        lines.push(HexLine::Row {
            kind,
            offset,
            cells,
        });
    }
}

/// Render one row's hex + ASCII text into `out`, applying `paint` per cell.
///
/// `paint(s, cls)` returns `s` styled for the given class (identity for
/// the no-color path).
pub(crate) fn write_row(
    out: &mut String,
    offset: usize,
    cells: &[HexCell],
    mut paint: impl FnMut(&str, Cls) -> String,
) {
    use std::fmt::Write;

    write!(out, "{offset:08x} ").unwrap();
    for i in 0..ROW {
        if i % 8 == 0 {
            out.push(' ');
        }
        match cells.get(i) {
            Some(c) => {
                out.push_str(&paint(&format!("{:02x}", c.byte), c.cls));
                out.push(' ');
            }
            None => out.push_str("   "),
        }
    }
    out.push_str(" |");
    for c in cells {
        let ch = if (0x20..=0x7e).contains(&c.byte) {
            (c.byte as char).to_string()
        } else {
            ".".to_string()
        };
        out.push_str(&paint(&ch, c.cls));
    }
    out.push('|');
}

/// Render a full hex-dump diff of two buffers as a multi-line string.
///
/// Each row is prefixed with `-`/`+`/` ` and, when `colors` is set,
/// changed bytes are highlighted (deleted = red, inserted = green)
/// against muted unchanged bytes. The block has no trailing newline.
pub(crate) fn render_hex_diff(from: &[u8], to: &[u8], colors: bool) -> String {
    use facet_pretty::tokyo_night;
    use owo_colors::OwoColorize;

    let paint = |s: &str, cls: Cls| -> String {
        if !colors {
            return s.to_string();
        }
        match cls {
            Cls::Equal => s.color(tokyo_night::MUTED).to_string(),
            Cls::Deleted => s.color(tokyo_night::DELETION).to_string(),
            Cls::Inserted => s.color(tokyo_night::INSERTION).to_string(),
        }
    };

    let mut out = String::new();
    for (i, line) in diff_hex(from, to).iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match line {
            HexLine::Collapsed(n) => {
                let label = if *n == 1 { "row" } else { "rows" };
                let text = format!("  .. {n} unchanged {label}");
                if colors {
                    out.push_str(&text.color(tokyo_night::MUTED).to_string());
                } else {
                    out.push_str(&text);
                }
            }
            HexLine::Row {
                kind,
                offset,
                cells,
            } => {
                let (marker, marker_color) = match kind {
                    RowKind::Removed => ("- ", tokyo_night::DELETION),
                    RowKind::Added => ("+ ", tokyo_night::INSERTION),
                };
                if colors {
                    out.push_str(&marker.color(marker_color).to_string());
                } else {
                    out.push_str(marker);
                }
                write_row(&mut out, *offset, cells, paint);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(lines: &[HexLine]) -> Vec<(RowKind, Vec<(u8, Cls)>)> {
        lines
            .iter()
            .filter_map(|l| match l {
                HexLine::Row { kind, cells, .. } => {
                    Some((*kind, cells.iter().map(|c| (c.byte, c.cls)).collect()))
                }
                HexLine::Collapsed(_) => None,
            })
            .collect()
    }

    #[test]
    fn single_byte_substitution_highlights_only_that_byte() {
        let rows = classes(&diff_hex(
            &[0xde, 0xad, 0xbe, 0xef],
            &[0xde, 0xad, 0xca, 0xfe],
        ));
        // One `-` row and one `+` row.
        assert_eq!(rows[0].0, RowKind::Removed);
        assert_eq!(rows[1].0, RowKind::Added);
        // de/ad unchanged, be/ef deleted in the `-` row.
        assert_eq!(rows[0].1[0], (0xde, Cls::Equal));
        assert_eq!(rows[0].1[1], (0xad, Cls::Equal));
        assert_eq!(rows[0].1[2], (0xbe, Cls::Deleted));
        assert_eq!(rows[0].1[3], (0xef, Cls::Deleted));
        // ca/fe inserted in the `+` row.
        assert_eq!(rows[1].1[2], (0xca, Cls::Inserted));
        assert_eq!(rows[1].1[3], (0xfe, Cls::Inserted));
    }

    #[test]
    fn single_byte_change_in_large_buffer_is_localized() {
        let from: Vec<u8> = (0u8..48).collect();
        let mut to = from.clone();
        to[0x24] = 0xff; // change one byte in the third row
        let lines = diff_hex(&from, &to);

        // Rows 0 and 1 (offsets 0x00, 0x10) collapse; row 2 changes.
        assert_eq!(lines[0], HexLine::Collapsed(2));
        let rows = classes(&lines);
        assert_eq!(rows[0].0, RowKind::Removed);
        assert_eq!(rows[0].1[4], (0x24, Cls::Deleted)); // 0x20 + 4 = 0x24
        // Every other byte on that row stays Equal.
        assert!(
            rows[0]
                .1
                .iter()
                .enumerate()
                .all(|(i, &(_, c))| (i == 4) == (c == Cls::Deleted))
        );
        assert_eq!(rows[1].0, RowKind::Added);
        assert_eq!(rows[1].1[4], (0xff, Cls::Inserted));
    }

    #[test]
    fn insertion_shifts_instead_of_marking_everything_changed() {
        // Insert one byte near the start; the Myers diff should mark just
        // the inserted byte, not every following byte.
        let from: Vec<u8> = (0u8..32).collect();
        let mut to = from.clone();
        to.insert(4, 0xaa);
        let lines = diff_hex(&from, &to);
        let rows = classes(&lines);

        let removed = rows.iter().find(|(k, _)| *k == RowKind::Removed).unwrap();
        let added = rows.iter().find(|(k, _)| *k == RowKind::Added).unwrap();
        // Exactly one inserted byte in the whole `+` side.
        let inserted: Vec<_> = added
            .1
            .iter()
            .filter(|(_, c)| *c == Cls::Inserted)
            .collect();
        assert_eq!(inserted, vec![&(0xaa, Cls::Inserted)]);
        // Nothing deleted on the `-` side (pure insertion).
        assert!(removed.1.iter().all(|(_, c)| *c != Cls::Deleted));
    }

    #[test]
    fn ascii_gutter_and_offsets() {
        let out = render_hex_diff(b"hello", b"hellp", false);
        assert!(out.contains("00000000 "));
        assert!(out.lines().next().unwrap().ends_with("|hello|"));
        assert!(out.lines().nth(1).unwrap().ends_with("|hellp|"));
    }
}
