//! Syntax highlighting support for showcases.

use std::cell::RefCell;

use arborium::theme::Theme;
use arborium::{AnsiHighlighter, Highlighter as ArboriumHighlighter};
use owo_colors::OwoColorize;

const INDENT: &str = "    ";

/// Supported languages for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// JSON format
    Json,
    /// YAML format
    Yaml,
    /// XML format
    Xml,
    /// HTML format
    Html,
    /// Rust code (for type definitions)
    Rust,
    /// Plain text (no syntax highlighting)
    Plain,
}

impl Language {
    /// Returns the file extension used to look up the syntax.
    pub const fn extension(self) -> &'static str {
        match self {
            Language::Json => "json",
            Language::Yaml => "yaml",
            Language::Xml => "xml",
            Language::Html => "html",
            Language::Rust => "rs",
            Language::Plain => "txt",
        }
    }

    /// Returns a human-readable name for the language.
    pub const fn name(self) -> &'static str {
        match self {
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Xml => "XML",
            Language::Html => "HTML",
            Language::Rust => "Rust",
            Language::Plain => "Output",
        }
    }

    const fn arborium_name(self) -> Option<&'static str> {
        match self {
            Language::Json => Some("json"),
            Language::Yaml => Some("yaml"),
            Language::Xml => Some("xml"),
            Language::Html => Some("html"),
            Language::Rust => Some("rust"),
            Language::Plain => None, // No syntax highlighting
        }
    }
}

/// Syntax highlighter using Tokyo Night theme powered by arborium.
pub struct Highlighter {
    html_highlighter: RefCell<ArboriumHighlighter>,
    ansi_highlighter: RefCell<AnsiHighlighter>,
    theme: Theme,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    /// Create a new highlighter with the Tokyo Night theme.
    pub fn new() -> Self {
        let theme = arborium::theme::builtin::tokyo_night().clone();
        Self {
            html_highlighter: RefCell::new(ArboriumHighlighter::new()),
            ansi_highlighter: RefCell::new(AnsiHighlighter::new(theme.clone())),
            theme,
        }
    }

    /// Get a reference to the theme.
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Highlight code and return terminal-escaped string.
    pub fn highlight_to_terminal(&self, code: &str, lang: Language) -> String {
        let Some(lang_name) = lang.arborium_name() else {
            return self.plain_text_with_indent(code);
        };
        let mut hl = self.ansi_highlighter.borrow_mut();
        match hl.highlight(lang_name, code) {
            Ok(output) => {
                // Add indentation to each line
                let mut result = String::new();
                for line in output.lines() {
                    result.push_str(INDENT);
                    result.push_str(line);
                    result.push('\n');
                }
                result
            }
            Err(_) => self.plain_text_with_indent(code),
        }
    }

    /// Highlight code with line numbers for terminal output.
    pub fn highlight_to_terminal_with_line_numbers(&self, code: &str, lang: Language) -> String {
        let Some(lang_name) = lang.arborium_name() else {
            return self.plain_text_with_line_numbers(code);
        };
        let mut hl = self.ansi_highlighter.borrow_mut();
        match hl.highlight(lang_name, code) {
            Ok(output) => {
                let mut result = String::new();
                for (i, line) in output.lines().enumerate() {
                    result.push_str(&format!(
                        "{} {} {}\n",
                        format!("{:3}", i + 1).dimmed(),
                        "│".dimmed(),
                        line
                    ));
                }
                result
            }
            Err(_) => self.plain_text_with_line_numbers(code),
        }
    }

    /// Highlight code and return HTML with inline styles.
    pub fn highlight_to_html(&self, code: &str, lang: Language) -> String {
        let Some(lang_name) = lang.arborium_name() else {
            return wrap_plain_text_html(code, &self.theme);
        };
        let mut hl = self.html_highlighter.borrow_mut();
        match hl.highlight(lang_name, code) {
            Ok(html) => wrap_with_pre(html, &self.theme),
            Err(_) => wrap_plain_text_html(code, &self.theme),
        }
    }

    fn plain_text_with_indent(&self, code: &str) -> String {
        let mut output = String::new();
        for line in code.lines() {
            output.push_str(INDENT);
            output.push_str(line);
            output.push('\n');
        }
        output
    }

    fn plain_text_with_line_numbers(&self, code: &str) -> String {
        let mut output = String::new();
        for (i, line) in code.lines().enumerate() {
            output.push_str(&format!(
                "{} {} {}\n",
                format!("{:3}", i + 1).dimmed(),
                "│".dimmed(),
                line
            ));
        }
        output
    }
}

/// Escape HTML special characters.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Convert ANSI escape codes to HTML spans with inline styles.
/// Uses non-breaking spaces to preserve alignment in monospace output.
pub fn ansi_to_html(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    let mut in_span = false;

    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['

            // Parse the escape sequence
            let mut seq = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_digit() || ch == ';' {
                    seq.push(chars.next().unwrap());
                } else {
                    break;
                }
            }

            // Consume the final character (usually 'm')
            let final_char = chars.next();

            if final_char == Some('m') {
                // Close any existing span
                if in_span {
                    output.push_str("</span>");
                    in_span = false;
                }

                // Parse the style
                if let Some(style) = parse_ansi_style(&seq)
                    && !style.is_empty()
                {
                    output.push_str(&format!("<span style=\"{style}\">"));
                    in_span = true;
                }
            }
        } else if c == '<' {
            output.push_str("&lt;");
        } else if c == '>' {
            output.push_str("&gt;");
        } else if c == '&' {
            output.push_str("&amp;");
        } else if c == '`' {
            // Escape backticks to prevent markdown interpretation
            output.push_str("&#96;");
        } else if c == ' ' {
            // Use non-breaking space to preserve alignment
            output.push('\u{00A0}');
        } else {
            output.push(c);
        }
    }

    if in_span {
        output.push_str("</span>");
    }

    output
}

/// Parse ANSI style codes and return CSS style string.
fn parse_ansi_style(seq: &str) -> Option<String> {
    if seq.is_empty() || seq == "0" {
        return Some(String::new()); // Reset
    }

    let parts: Vec<&str> = seq.split(';').collect();
    let mut styles = Vec::new();

    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "0" => return Some(String::new()), // Reset
            "1" => styles.push("font-weight:bold".to_string()),
            "2" => styles.push("opacity:0.7".to_string()), // Dim
            "3" => styles.push("font-style:italic".to_string()),
            "4" => styles.push("text-decoration:underline".to_string()),
            "30" => styles.push("color:#000".to_string()),
            "31" => styles.push("color:#e06c75".to_string()), // Red
            "32" => styles.push("color:#98c379".to_string()), // Green
            "33" => styles.push("color:#e5c07b".to_string()), // Yellow
            "34" => styles.push("color:#61afef".to_string()), // Blue
            "35" => styles.push("color:#c678dd".to_string()), // Magenta
            "36" => styles.push("color:#56b6c2".to_string()), // Cyan
            "37" => styles.push("color:#abb2bf".to_string()), // White
            "38" => {
                // Extended color
                if i + 1 < parts.len() && parts[i + 1] == "2" {
                    // 24-bit RGB
                    if i + 4 < parts.len() {
                        let r = parts[i + 2];
                        let g = parts[i + 3];
                        let b = parts[i + 4];
                        styles.push(format!("color:rgb({r},{g},{b})"));
                        i += 4;
                    }
                } else if i + 1 < parts.len()
                    && parts[i + 1] == "5"
                    && i + 2 < parts.len()
                    && let Ok(n) = parts[i + 2].parse::<u8>()
                {
                    let color = ansi_256_to_rgb(n);
                    styles.push(format!("color:{color}"));
                    i += 2;
                }
            }
            "39" => styles.push("color:inherit".to_string()),
            "40" => styles.push("background-color:#000".to_string()),
            "41" => styles.push("background-color:#e06c75".to_string()),
            "42" => styles.push("background-color:#98c379".to_string()),
            "43" => styles.push("background-color:#e5c07b".to_string()),
            "44" => styles.push("background-color:#61afef".to_string()),
            "45" => styles.push("background-color:#c678dd".to_string()),
            "46" => styles.push("background-color:#56b6c2".to_string()),
            "47" => styles.push("background-color:#abb2bf".to_string()),
            "48" => {
                if i + 1 < parts.len() && parts[i + 1] == "2" {
                    if i + 4 < parts.len() {
                        let r = parts[i + 2];
                        let g = parts[i + 3];
                        let b = parts[i + 4];
                        styles.push(format!("background-color:rgb({r},{g},{b})"));
                        i += 4;
                    }
                } else if i + 1 < parts.len()
                    && parts[i + 1] == "5"
                    && i + 2 < parts.len()
                    && let Ok(n) = parts[i + 2].parse::<u8>()
                {
                    let color = ansi_256_to_rgb(n);
                    styles.push(format!("background-color:{color}"));
                    i += 2;
                }
            }
            "49" => styles.push("background-color:transparent".to_string()),
            "90" => styles.push("color:#5c6370".to_string()), // Bright black (dim)
            "91" => styles.push("color:#e06c75".to_string()), // Bright red
            "92" => styles.push("color:#98c379".to_string()),
            "93" => styles.push("color:#e5c07b".to_string()), // Bright yellow
            "94" => styles.push("color:#61afef".to_string()),
            "95" => styles.push("color:#c678dd".to_string()), // Bright magenta
            "96" => styles.push("color:#56b6c2".to_string()),
            "97" => styles.push("color:#fff".to_string()), // Bright white
            _ => {}
        }
        i += 1;
    }

    if styles.is_empty() {
        None
    } else {
        Some(styles.join(";"))
    }
}

const fn ansi_256_to_rgb(n: u8) -> &'static str {
    match n {
        0 => "#000000",
        1 => "#800000",
        2 => "#008000",
        3 => "#808000",
        4 => "#000080",
        5 => "#800080",
        6 => "#008080",
        7 => "#c0c0c0",
        8 => "#808080",
        9 => "#ff0000",
        10 => "#00ff00",
        11 => "#ffff00",
        12 => "#0000ff",
        13 => "#ff00ff",
        14 => "#00ffff",
        15 => "#ffffff",
        _ => "#888888",
    }
}

fn wrap_plain_text_html(code: &str, theme: &Theme) -> String {
    wrap_with_pre(html_escape(code), theme)
}

fn wrap_with_pre(content: String, theme: &Theme) -> String {
    // Replace blank lines with <br> to preserve visual spacing.
    // In CommonMark, blank lines terminate HTML blocks, so we must not have
    // any actual blank lines inside our pre elements when embedded in markdown.
    let content = blank_lines_to_br(&content);

    let mut styles = Vec::new();
    if let Some(bg) = theme.background {
        styles.push(format!("background-color:{};", bg.to_hex()));
    }
    if let Some(fg) = theme.foreground {
        styles.push(format!("color:{};", fg.to_hex()));
    }
    styles.push("padding:12px;".to_string());
    styles.push("border-radius:8px;".to_string());
    styles.push(
        "font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace);"
            .to_string(),
    );
    styles.push("font-size:0.9rem;".to_string());
    styles.push("overflow:auto;".to_string());
    format!(
        "<pre style=\"{}\"><code>{}</code></pre>",
        styles.join(" "),
        content
    )
}

/// Replace blank lines (2+ consecutive newlines) with `<br>` tags.
/// This preserves visual spacing while preventing CommonMark from
/// terminating the HTML block at blank lines inside `<pre>` elements.
fn blank_lines_to_br(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut newline_count = 0;

    for c in s.chars() {
        if c == '\n' {
            newline_count += 1;
        } else {
            // Flush accumulated newlines
            if newline_count > 0 {
                result.push('\n');
                // For each extra newline beyond the first, add a <br>
                for _ in 1..newline_count {
                    result.push_str("<br>");
                }
                newline_count = 0;
            }
            result.push(c);
        }
    }

    // Handle trailing newlines
    if newline_count > 0 {
        result.push('\n');
        for _ in 1..newline_count {
            result.push_str("<br>");
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{Language, blank_lines_to_br};

    #[test]
    fn xml_language_metadata_is_exposed() {
        assert_eq!(Language::Xml.name(), "XML");
        assert_eq!(Language::Xml.extension(), "xml");
    }

    #[test]
    fn blank_lines_to_br_preserves_visual_spacing() {
        // Single newlines preserved as-is
        assert_eq!(blank_lines_to_br("a\nb\nc"), "a\nb\nc");

        // Double newlines (blank line) -> newline + <br>
        assert_eq!(blank_lines_to_br("a\n\nb"), "a\n<br>b");

        // Triple newlines -> newline + 2x <br>
        assert_eq!(blank_lines_to_br("a\n\n\nb"), "a\n<br><br>b");

        // Mixed content
        assert_eq!(
            blank_lines_to_br("line1\n\nline2\nline3\n\n\nline4"),
            "line1\n<br>line2\nline3\n<br><br>line4"
        );

        // Empty string
        assert_eq!(blank_lines_to_br(""), "");

        // Only newlines
        assert_eq!(blank_lines_to_br("\n\n\n"), "\n<br><br>");
    }
}
