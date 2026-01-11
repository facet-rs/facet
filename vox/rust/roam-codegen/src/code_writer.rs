//! Code writer with automatic indentation tracking for code generation.
//!
//! This module provides a `CodeWriter` that simplifies generating properly
//! indented code in C-like languages (Swift, TypeScript, Go, Java, etc.).
//!
//! # Features
//!
//! - **RAII-based indentation**: Use `indent()` to get a guard that automatically
//!   manages indentation levels
//! - **No borrow checker fights**: Uses `Rc<Cell<usize>>` internally so indent
//!   guards don't conflict with mutable writes
//! - **C-like syntax helpers**: Built-in support for blocks, comments, parentheses
//! - **Format macros**: `cw_write!` and `cw_writeln!` for formatted output
//!
//! # Basic Example
//!
//! ```
//! use roam_codegen::code_writer::CodeWriter;
//! use roam_codegen::{cw_write, cw_writeln};
//!
//! let mut output = String::new();
//! let mut w = CodeWriter::with_indent_spaces(&mut output, 4);
//!
//! w.writeln("class Example {").unwrap();
//! {
//!     let _indent = w.indent();
//!     w.writeln("private let value: Int").unwrap();
//!     w.blank_line().unwrap();
//!
//!     cw_writeln!(w, "func compute() -> Int {{").unwrap();
//!     {
//!         let _indent = w.indent();
//!         cw_writeln!(w, "return value * 2").unwrap();
//!     }
//!     w.writeln("}").unwrap();
//! }
//! w.writeln("}").unwrap();
//! ```
//!
//! # Using the `block` Helper
//!
//! For common brace-delimited blocks, use the `block` helper:
//!
//! ```
//! use roam_codegen::code_writer::CodeWriter;
//!
//! let mut output = String::new();
//! let mut w = CodeWriter::with_indent_spaces(&mut output, 2);
//!
//! w.block("class Foo", |w| {
//!     w.writeln("let x = 42")?;
//!     w.block("func bar()", |w| {
//!         w.writeln("return x")
//!     })
//! }).unwrap();
//! ```
//!
//! # Comma-Separated Lists
//!
//! ```
//! use roam_codegen::code_writer::CodeWriter;
//!
//! let mut output = String::new();
//! let mut w = CodeWriter::with_indent_spaces(&mut output, 2);
//!
//! w.write("func(").unwrap();
//! w.write_separated(vec!["a: Int", "b: String"], ", ", |w, item| {
//!     w.write(item)
//! }).unwrap();
//! w.write(")").unwrap();
//! // Output: "func(a: Int, b: String)"
//! ```

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

/// A code writer that tracks indentation and provides helpers for generating
/// C-like syntax (used by Swift, TypeScript, Go, etc.)
pub struct CodeWriter<W> {
    writer: W,
    indent_level: Rc<Cell<usize>>,
    indent_string: String,
    at_line_start: Cell<bool>,
}

impl<W: fmt::Write> CodeWriter<W> {
    /// Create a new CodeWriter with the given writer and indent string (e.g., "    " or "\t")
    pub fn new(writer: W, indent_string: String) -> Self {
        Self {
            writer,
            indent_level: Rc::new(Cell::new(0)),
            indent_string,
            at_line_start: Cell::new(true),
        }
    }

    /// Create a new CodeWriter with 4-space indentation
    pub fn with_indent_spaces(writer: W, spaces: usize) -> Self {
        Self::new(writer, " ".repeat(spaces))
    }

    /// Write text without a newline. Adds indentation if at line start.
    pub fn write(&mut self, text: &str) -> fmt::Result {
        if text.is_empty() {
            return Ok(());
        }

        if self.at_line_start.get() && !text.trim().is_empty() {
            for _ in 0..self.indent_level.get() {
                self.writer.write_str(&self.indent_string)?;
            }
            self.at_line_start.set(false);
        }

        self.writer.write_str(text)
    }

    /// Write text followed by a newline. Adds indentation if needed.
    pub fn writeln(&mut self, text: &str) -> fmt::Result {
        self.write(text)?;
        self.writer.write_char('\n')?;
        self.at_line_start.set(true);
        Ok(())
    }

    /// Write an empty line
    pub fn blank_line(&mut self) -> fmt::Result {
        self.writer.write_char('\n')?;
        self.at_line_start.set(true);
        Ok(())
    }

    /// Create an indentation guard. Indentation increases while the guard is alive.
    pub fn indent(&mut self) -> IndentGuard {
        self.indent_level.set(self.indent_level.get() + 1);
        IndentGuard {
            indent_level: Rc::clone(&self.indent_level),
        }
    }

    /// Write a single-line comment (e.g., "// comment")
    pub fn comment(&mut self, comment_prefix: &str, text: &str) -> fmt::Result {
        self.writeln(&format!("{} {}", comment_prefix, text))
    }

    /// Write a doc comment block. Each line is prefixed with the comment marker.
    pub fn doc_comment(&mut self, comment_prefix: &str, text: &str) -> fmt::Result {
        for line in text.lines() {
            self.writeln(&format!("{} {}", comment_prefix, line))?;
        }
        Ok(())
    }

    /// Begin a block with opening brace: writes "header {" and returns indent guard
    pub fn begin_block(&mut self, header: &str) -> Result<IndentGuard, fmt::Error> {
        self.writeln(&format!("{} {{", header))?;
        Ok(self.indent())
    }

    /// End a block with closing brace
    pub fn end_block(&mut self) -> fmt::Result {
        self.writeln("}")
    }

    /// Write a complete block with a closure for the body
    pub fn block<F>(&mut self, header: &str, body: F) -> fmt::Result
    where
        F: FnOnce(&mut Self) -> fmt::Result,
    {
        self.writeln(&format!("{} {{", header))?;
        {
            let _indent = self.indent();
            body(self)?;
        }
        self.writeln("}")
    }

    /// Get the current indentation level
    pub fn indent_level(&self) -> usize {
        self.indent_level.get()
    }

    /// Consume the writer and return the inner writer
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Get a reference to the inner writer
    pub fn inner(&self) -> &W {
        &self.writer
    }

    /// Get a mutable reference to the inner writer
    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Write formatted text (like write! macro)
    ///
    /// Use the `write!` and `writeln!` macros instead of calling this directly.
    #[doc(hidden)]
    pub fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
        let formatted = format!("{}", args);
        self.write(&formatted)
    }

    /// Write formatted text with newline (like writeln! macro)
    ///
    /// Use the `write!` and `writeln!` macros instead of calling this directly.
    #[doc(hidden)]
    pub fn writeln_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
        let formatted = format!("{}", args);
        self.writeln(&formatted)
    }

    /// Write items separated by a delimiter (e.g., comma-separated list)
    pub fn write_separated<I, F>(
        &mut self,
        items: I,
        separator: &str,
        mut write_item: F,
    ) -> fmt::Result
    where
        I: IntoIterator,
        F: FnMut(&mut Self, I::Item) -> fmt::Result,
    {
        let mut first = true;
        for item in items {
            if !first {
                self.write(separator)?;
            }
            write_item(self, item)?;
            first = false;
        }
        Ok(())
    }

    /// Write items separated by delimiter with newlines (one item per line)
    pub fn write_separated_lines<I, F>(
        &mut self,
        items: I,
        separator: &str,
        mut write_item: F,
    ) -> fmt::Result
    where
        I: IntoIterator,
        F: FnMut(&mut Self, I::Item) -> fmt::Result,
    {
        let mut first = true;
        for item in items {
            if !first {
                self.writeln(separator)?;
            }
            write_item(self, item)?;
            first = false;
        }
        Ok(())
    }

    /// Conditionally write content
    pub fn write_if<F>(&mut self, condition: bool, f: F) -> fmt::Result
    where
        F: FnOnce(&mut Self) -> fmt::Result,
    {
        if condition { f(self) } else { Ok(()) }
    }

    /// Write a parenthesized list (e.g., "(a, b, c)")
    pub fn write_parens<F>(&mut self, f: F) -> fmt::Result
    where
        F: FnOnce(&mut Self) -> fmt::Result,
    {
        self.write("(")?;
        f(self)?;
        self.write(")")
    }

    /// Write a bracketed list (e.g., "[a, b, c]")
    pub fn write_brackets<F>(&mut self, f: F) -> fmt::Result
    where
        F: FnOnce(&mut Self) -> fmt::Result,
    {
        self.write("[")?;
        f(self)?;
        self.write("]")
    }

    /// Write an angle-bracketed list (e.g., "<T, U>")
    pub fn write_angles<F>(&mut self, f: F) -> fmt::Result
    where
        F: FnOnce(&mut Self) -> fmt::Result,
    {
        self.write("<")?;
        f(self)?;
        self.write(">")
    }
}

/// RAII guard that maintains indentation level
///
/// Uses `Rc<Cell<usize>>` to independently manage indent level without any borrows.
pub struct IndentGuard {
    indent_level: Rc<Cell<usize>>,
}

impl Drop for IndentGuard {
    fn drop(&mut self) {
        let current = self.indent_level.get();
        self.indent_level.set(current.saturating_sub(1));
    }
}

/// Write formatted text to a CodeWriter (like std::write!)
///
/// # Example
/// ```ignore
/// write!(w, "let x = {}", 42)?;
/// ```
#[macro_export]
macro_rules! cw_write {
    ($writer:expr, $($arg:tt)*) => {
        $writer.write_fmt(format_args!($($arg)*))
    };
}

/// Write formatted text with newline to a CodeWriter (like std::writeln!)
///
/// # Example
/// ```ignore
/// writeln!(w, "let x = {}", 42)?;
/// ```
#[macro_export]
macro_rules! cw_writeln {
    ($writer:expr, $($arg:tt)*) => {
        $writer.writeln_fmt(format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_writing() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.writeln("hello").unwrap();
        w.writeln("world").unwrap();

        assert_eq!(output, "hello\nworld\n");
    }

    #[test]
    fn test_indentation() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.writeln("level 0").unwrap();
        {
            let _indent = w.indent();
            w.writeln("level 1").unwrap();
            {
                let _indent = w.indent();
                w.writeln("level 2").unwrap();
            }
            w.writeln("level 1 again").unwrap();
        }
        w.writeln("level 0 again").unwrap();

        assert_eq!(
            output,
            "level 0\n  level 1\n    level 2\n  level 1 again\nlevel 0 again\n"
        );
    }

    #[test]
    fn test_block_helper() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.block("class Foo", |w| {
            w.writeln("let x = 42")?;
            w.block("func bar()", |w| w.writeln("return x"))
        })
        .unwrap();

        assert_eq!(
            output,
            "class Foo {\n  let x = 42\n  func bar() {\n    return x\n  }\n}\n"
        );
    }

    #[test]
    fn test_begin_end_block() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.writeln("before").unwrap();
        {
            let _guard = w.begin_block("if true").unwrap();
            w.writeln("inside").unwrap();
        }
        w.end_block().unwrap();
        w.writeln("after").unwrap();

        assert_eq!(output, "before\nif true {\n  inside\n}\nafter\n");
    }

    #[test]
    fn test_comments() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.comment("//", "Single line comment").unwrap();
        w.doc_comment("///", "Doc comment\nwith multiple lines")
            .unwrap();

        assert_eq!(
            output,
            "// Single line comment\n/// Doc comment\n/// with multiple lines\n"
        );
    }

    #[test]
    fn test_blank_lines() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.writeln("line 1").unwrap();
        w.blank_line().unwrap();
        w.writeln("line 2").unwrap();

        assert_eq!(output, "line 1\n\nline 2\n");
    }

    #[test]
    fn test_write_separated() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        let items = vec!["a", "b", "c"];
        w.write_separated(items, ", ", |w, item| w.write(item))
            .unwrap();

        assert_eq!(output, "a, b, c");
    }

    #[test]
    fn test_write_separated_lines() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.writeln("items:").unwrap();
        {
            let _indent = w.indent();
            let items = vec!["first", "second", "third"];
            w.write_separated_lines(items, ",", |w, item| w.write(item))
                .unwrap();
        }

        assert_eq!(output, "items:\n  first,\n  second,\n  third");
    }

    #[test]
    fn test_write_parens() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.write("func").unwrap();
        w.write_parens(|w| {
            w.write_separated(vec!["a: Int", "b: String"], ", ", |w, item| w.write(item))
        })
        .unwrap();

        assert_eq!(output, "func(a: Int, b: String)");
    }

    #[test]
    fn test_write_if() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        w.write_if(true, |w| w.writeln("shown")).unwrap();
        w.write_if(false, |w| w.writeln("hidden")).unwrap();
        w.write_if(true, |w| w.writeln("also shown")).unwrap();

        assert_eq!(output, "shown\nalso shown\n");
    }

    #[test]
    fn test_write_fmt() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        let name = "test";
        let value = 42;
        w.write_fmt(format_args!("let {} = {}", name, value))
            .unwrap();

        assert_eq!(output, "let test = 42");
    }

    #[test]
    fn test_complex_code_generation() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 4);

        w.doc_comment("///", "A sample class").unwrap();
        w.block("class Example", |w| {
            w.writeln("private let value: Int").unwrap();
            w.blank_line().unwrap();
            w.write("init")?;
            w.write_parens(|w| w.write("value: Int"))?;
            w.writeln(" {")?;
            {
                let _indent = w.indent();
                w.writeln("self.value = value")?;
            }
            w.writeln("}")?;
            w.blank_line()?;
            w.block("func compute() -> Int", |w| w.writeln("return value * 2"))
        })
        .unwrap();

        let expected = "\
/// A sample class
class Example {
    private let value: Int

    init(value: Int) {
        self.value = value
    }

    func compute() -> Int {
        return value * 2
    }
}
";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_macros() {
        let mut output = String::new();
        let mut w = CodeWriter::with_indent_spaces(&mut output, 2);

        let name = "counter";
        let value = 42;

        cw_writeln!(w, "let {} = {}", name, value).unwrap();
        cw_write!(w, "println!(\"value: {{}}\"").unwrap();
        cw_writeln!(w, ", {})", name).unwrap();

        assert_eq!(
            output,
            "let counter = 42\nprintln!(\"value: {}\", counter)\n"
        );
    }
}
