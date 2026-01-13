//! XML escaping utilities.

use std::io::{self, Write};

/// Wraps a `Write` and escapes XML special characters as bytes pass through.
pub struct EscapingWriter<'a> {
    inner: &'a mut dyn Write,
    escape_quotes: bool,
}

impl<'a> EscapingWriter<'a> {
    /// Create an escaping writer for text content.
    /// Escapes: `&` `<` `>`
    pub fn text(inner: &'a mut dyn Write) -> Self {
        Self {
            inner,
            escape_quotes: false,
        }
    }

    /// Create an escaping writer for attribute values.
    /// Escapes: `&` `<` `>` `"`
    pub fn attribute(inner: &'a mut dyn Write) -> Self {
        Self {
            inner,
            escape_quotes: true,
        }
    }
}

impl Write for EscapingWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for &b in buf {
            match b {
                b'&' => self.inner.write_all(b"&amp;")?,
                b'<' => self.inner.write_all(b"&lt;")?,
                b'>' => self.inner.write_all(b"&gt;")?,
                b'"' if self.escape_quotes => self.inner.write_all(b"&quot;")?,
                _ => self.inner.write_all(&[b])?,
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escapes_amp() {
        let mut buf = Vec::new();
        EscapingWriter::text(&mut buf).write_all(b"a & b").unwrap();
        assert_eq!(buf, b"a &amp; b");
    }

    #[test]
    fn text_escapes_lt() {
        let mut buf = Vec::new();
        EscapingWriter::text(&mut buf).write_all(b"a < b").unwrap();
        assert_eq!(buf, b"a &lt; b");
    }

    #[test]
    fn text_escapes_gt() {
        let mut buf = Vec::new();
        EscapingWriter::text(&mut buf).write_all(b"a > b").unwrap();
        assert_eq!(buf, b"a &gt; b");
    }

    #[test]
    fn text_does_not_escape_quotes() {
        let mut buf = Vec::new();
        EscapingWriter::text(&mut buf)
            .write_all(b"a \"quoted\" b")
            .unwrap();
        assert_eq!(buf, b"a \"quoted\" b");
    }

    #[test]
    fn attribute_escapes_amp() {
        let mut buf = Vec::new();
        EscapingWriter::attribute(&mut buf)
            .write_all(b"a & b")
            .unwrap();
        assert_eq!(buf, b"a &amp; b");
    }

    #[test]
    fn attribute_escapes_lt() {
        let mut buf = Vec::new();
        EscapingWriter::attribute(&mut buf)
            .write_all(b"a < b")
            .unwrap();
        assert_eq!(buf, b"a &lt; b");
    }

    #[test]
    fn attribute_escapes_gt() {
        let mut buf = Vec::new();
        EscapingWriter::attribute(&mut buf)
            .write_all(b"a > b")
            .unwrap();
        assert_eq!(buf, b"a &gt; b");
    }

    #[test]
    fn attribute_escapes_quotes() {
        let mut buf = Vec::new();
        EscapingWriter::attribute(&mut buf)
            .write_all(b"a \"quoted\" b")
            .unwrap();
        assert_eq!(buf, b"a &quot;quoted&quot; b");
    }

    #[test]
    fn escapes_all_special_chars() {
        let mut buf = Vec::new();
        EscapingWriter::attribute(&mut buf)
            .write_all(b"<a & \"b\">")
            .unwrap();
        assert_eq!(buf, b"&lt;a &amp; &quot;b&quot;&gt;");
    }

    #[test]
    fn passthrough_normal_chars() {
        let mut buf = Vec::new();
        EscapingWriter::attribute(&mut buf)
            .write_all(b"hello world 123")
            .unwrap();
        assert_eq!(buf, b"hello world 123");
    }

    #[test]
    fn multiple_writes() {
        let mut buf = Vec::new();
        let mut writer = EscapingWriter::attribute(&mut buf);
        writer.write_all(b"a < ").unwrap();
        writer.write_all(b"b & ").unwrap();
        writer.write_all(b"c").unwrap();
        assert_eq!(buf, b"a &lt; b &amp; c");
    }
}
