//! UTF-16 LSP position indexing.

use crate::types::{Position, Range};

/// Per-document byte/line index using LSP's default UTF-16 code-unit columns.
#[derive(Clone, Debug)]
pub struct LineIndex {
    text: String,
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Build an index for `text`.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (idx, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self {
            text: text.to_owned(),
            line_starts,
        }
    }

    /// Convert byte offset to LSP position.
    pub fn offset_to_position(&self, offset: u32) -> Option<Position> {
        let offset = usize::try_from(offset).ok()?;
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next.checked_sub(1)?,
        };
        let line_start = self.line_starts[line];
        let character = self.text[line_start..offset]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>();
        Some(Position {
            line: u32::try_from(line).ok()?,
            character: u32::try_from(character).ok()?,
        })
    }

    /// Convert LSP position to byte offset. Positions inside a surrogate pair
    /// are rejected instead of rounded.
    pub fn position_to_offset(&self, position: Position) -> Option<u32> {
        let line = usize::try_from(position.line).ok()?;
        let wanted = usize::try_from(position.character).ok()?;
        let line_start = *self.line_starts.get(line)?;
        let line_end = self.line_end(line);
        let mut utf16 = 0usize;
        for (byte_delta, ch) in self.text[line_start..line_end].char_indices() {
            if utf16 == wanted {
                return u32::try_from(line_start + byte_delta).ok();
            }
            utf16 += ch.len_utf16();
            if utf16 > wanted {
                return None;
            }
        }
        if utf16 == wanted {
            u32::try_from(line_end).ok()
        } else {
            None
        }
    }

    /// Convert a half-open byte span to an LSP range.
    pub fn range(&self, start: u32, end: u32) -> Option<Range> {
        Some(Range {
            start: self.offset_to_position(start)?,
            end: self.offset_to_position(end)?,
        })
    }

    /// UTF-16 length of a half-open byte span on one line.
    pub fn utf16_len(&self, start: u32, end: u32) -> Option<u32> {
        let start = usize::try_from(start).ok()?;
        let end = usize::try_from(end).ok()?;
        if start > end
            || end > self.text.len()
            || !self.text.is_char_boundary(start)
            || !self.text.is_char_boundary(end)
        {
            return None;
        }
        let start_line = self.offset_to_position(u32::try_from(start).ok()?)?.line;
        let end_line = self.offset_to_position(u32::try_from(end).ok()?)?.line;
        if start_line != end_line {
            return None;
        }
        let len = self.text[start..end]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>();
        u32::try_from(len).ok()
    }

    fn line_end(&self, line: usize) -> usize {
        self.line_starts
            .get(line + 1)
            .map(|next| next.saturating_sub(1))
            .unwrap_or(self.text.len())
    }
}
