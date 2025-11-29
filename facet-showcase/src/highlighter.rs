//! Syntax highlighting support for showcases.

use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, Theme, ThemeSet};
use syntect::html::highlighted_html_for_string;
use syntect::parsing::{SyntaxSet, SyntaxSetBuilder};
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

/// Supported languages for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// JSON format
    Json,
    /// YAML format
    Yaml,
    /// KDL format (requires custom syntax definition)
    Kdl,
    /// Rust code (for type definitions)
    Rust,
}

impl Language {
    /// Returns the file extension used to look up the syntax.
    pub fn extension(self) -> &'static str {
        match self {
            Language::Json => "json",
            Language::Yaml => "yaml",
            Language::Kdl => "kdl",
            Language::Rust => "rs",
        }
    }

    /// Returns a human-readable name for the language.
    pub fn name(self) -> &'static str {
        match self {
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Kdl => "KDL",
            Language::Rust => "Rust",
        }
    }
}

/// Syntax highlighter using Tokyo Night theme.
pub struct Highlighter {
    /// Default syntax set (JSON, YAML, Rust, etc.)
    default_ps: SyntaxSet,
    /// Custom syntax set for KDL (built from .sublime-syntax files)
    kdl_ps: Option<SyntaxSet>,
    /// Tokyo Night theme
    theme: Theme,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    /// Create a new highlighter with Tokyo Night theme.
    pub fn new() -> Self {
        let default_ps = SyntaxSet::load_defaults_newlines();

        // Try to load Tokyo Night theme from the shared themes directory
        let theme = Self::load_tokyo_night_theme();

        Self {
            default_ps,
            kdl_ps: None,
            theme,
        }
    }

    /// Load Tokyo Night theme, falling back to base16-ocean.dark if not found.
    fn load_tokyo_night_theme() -> Theme {
        // Try common locations for the theme
        let possible_paths = [
            // When running from workspace root
            "themes/TokyoNight.tmTheme",
            // When running from a subcrate
            "../themes/TokyoNight.tmTheme",
            // When running from examples
            "../../themes/TokyoNight.tmTheme",
        ];

        for path in possible_paths {
            if let Ok(theme) = ThemeSet::get_theme(path) {
                return theme;
            }
        }

        // Fallback to default theme
        let ts = ThemeSet::load_defaults();
        ts.themes["base16-ocean.dark"].clone()
    }

    /// Add KDL syntax support from a directory containing .sublime-syntax files.
    pub fn with_kdl_syntaxes(mut self, syntax_dir: &str) -> Self {
        let mut builder = SyntaxSetBuilder::new();
        builder.add_plain_text_syntax();
        if builder.add_from_folder(syntax_dir, true).is_ok() {
            self.kdl_ps = Some(builder.build());
        }
        self
    }

    /// Get a reference to the theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Highlight code and return terminal-escaped string.
    pub fn highlight_to_terminal(&self, code: &str, lang: Language) -> String {
        let mut output = String::new();

        let (ps, syntax) = match lang {
            Language::Kdl => {
                if let Some(ref kdl_ps) = self.kdl_ps {
                    // Try "KDL" first (simpler syntax), then "KDL1" (complex syntax)
                    if let Some(syntax) = kdl_ps
                        .find_syntax_by_name("KDL")
                        .or_else(|| kdl_ps.find_syntax_by_name("KDL1"))
                    {
                        (kdl_ps, syntax)
                    } else {
                        // Fallback to plain text
                        return self.plain_text_with_indent(code);
                    }
                } else {
                    return self.plain_text_with_indent(code);
                }
            }
            _ => {
                let syntax = self
                    .default_ps
                    .find_syntax_by_extension(lang.extension())
                    .unwrap_or_else(|| self.default_ps.find_syntax_plain_text());
                (&self.default_ps, syntax)
            }
        };

        let mut h = HighlightLines::new(syntax, &self.theme);
        for line in LinesWithEndings::from(code) {
            let ranges: Vec<(Style, &str)> = h.highlight_line(line, ps).unwrap_or_default();
            let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
            output.push_str("    ");
            output.push_str(&escaped);
        }
        output.push_str("\x1b[0m"); // Reset terminal colors

        // Ensure there's a trailing newline
        if !output.ends_with('\n') {
            output.push('\n');
        }

        output
    }

    /// Highlight code with line numbers for terminal output.
    pub fn highlight_to_terminal_with_line_numbers(&self, code: &str, lang: Language) -> String {
        use owo_colors::OwoColorize;

        let mut output = String::new();

        let (ps, syntax) = match lang {
            Language::Kdl => {
                if let Some(ref kdl_ps) = self.kdl_ps {
                    if let Some(syntax) = kdl_ps
                        .find_syntax_by_name("KDL")
                        .or_else(|| kdl_ps.find_syntax_by_name("KDL1"))
                    {
                        (kdl_ps, syntax)
                    } else {
                        return self.plain_text_with_line_numbers(code);
                    }
                } else {
                    return self.plain_text_with_line_numbers(code);
                }
            }
            _ => {
                let syntax = self
                    .default_ps
                    .find_syntax_by_extension(lang.extension())
                    .unwrap_or_else(|| self.default_ps.find_syntax_plain_text());
                (&self.default_ps, syntax)
            }
        };

        let mut h = HighlightLines::new(syntax, &self.theme);
        for (i, line) in code.lines().enumerate() {
            let line_with_newline = format!("{line}\n");
            let ranges: Vec<(Style, &str)> =
                h.highlight_line(&line_with_newline, ps).unwrap_or_default();
            let escaped = as_24_bit_terminal_escaped(&ranges[..], false);

            output.push_str(&format!(
                "{} {} {}",
                format!("{:3}", i + 1).dimmed(),
                "│".dimmed(),
                escaped
            ));
        }
        output.push_str("\x1b[0m"); // Reset terminal colors

        output
    }

    /// Build a SyntectHighlighter for miette error rendering.
    pub fn build_miette_highlighter(
        &self,
        lang: Language,
    ) -> miette::highlighters::SyntectHighlighter {
        let (syntax_set, _) = match lang {
            Language::Kdl => {
                if let Some(ref kdl_ps) = self.kdl_ps {
                    (kdl_ps.clone(), ())
                } else {
                    (self.default_ps.clone(), ())
                }
            }
            _ => (self.default_ps.clone(), ()),
        };

        miette::highlighters::SyntectHighlighter::new(syntax_set, self.theme.clone(), false)
    }

    /// Highlight code and return HTML with inline styles.
    pub fn highlight_to_html(&self, code: &str, lang: Language) -> String {
        let (ps, syntax) = match lang {
            Language::Kdl => {
                if let Some(ref kdl_ps) = self.kdl_ps {
                    if let Some(syntax) = kdl_ps
                        .find_syntax_by_name("KDL")
                        .or_else(|| kdl_ps.find_syntax_by_name("KDL1"))
                    {
                        (kdl_ps, syntax)
                    } else {
                        return html_escape(code);
                    }
                } else {
                    return html_escape(code);
                }
            }
            _ => {
                let syntax = self
                    .default_ps
                    .find_syntax_by_extension(lang.extension())
                    .unwrap_or_else(|| self.default_ps.find_syntax_plain_text());
                (&self.default_ps, syntax)
            }
        };

        // Use highlighted_html_for_string which produces inline styles
        highlighted_html_for_string(code, ps, syntax, &self.theme)
            .unwrap_or_else(|_| html_escape(code))
    }

    fn plain_text_with_indent(&self, code: &str) -> String {
        let mut output = String::new();
        for line in code.lines() {
            output.push_str("    ");
            output.push_str(line);
            output.push('\n');
        }
        output
    }

    fn plain_text_with_line_numbers(&self, code: &str) -> String {
        use owo_colors::OwoColorize;

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
                if let Some(style) = parse_ansi_style(&seq) {
                    if !style.is_empty() {
                        output.push_str(&format!("<span style=\"{style}\">"));
                        in_span = true;
                    }
                }
            }
        } else if c == '<' {
            output.push_str("&lt;");
        } else if c == '>' {
            output.push_str("&gt;");
        } else if c == '&' {
            output.push_str("&amp;");
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
                } else if i + 1 < parts.len() && parts[i + 1] == "5" {
                    // 256-color palette
                    if i + 2 < parts.len() {
                        if let Ok(n) = parts[i + 2].parse::<u8>() {
                            let color = ansi_256_to_rgb(n);
                            styles.push(format!("color:{color}"));
                        }
                        i += 2;
                    }
                }
            }
            "90" => styles.push("color:#5c6370".to_string()), // Bright black (gray)
            "91" => styles.push("color:#e06c75".to_string()), // Bright red
            "92" => styles.push("color:#98c379".to_string()), // Bright green
            "93" => styles.push("color:#e5c07b".to_string()), // Bright yellow
            "94" => styles.push("color:#61afef".to_string()), // Bright blue
            "95" => styles.push("color:#c678dd".to_string()), // Bright magenta
            "96" => styles.push("color:#56b6c2".to_string()), // Bright cyan
            "97" => styles.push("color:#fff".to_string()),    // Bright white
            _ => {}
        }
        i += 1;
    }

    Some(styles.join(";"))
}

/// Convert ANSI 256-color palette index to hex color.
fn ansi_256_to_rgb(n: u8) -> &'static str {
    match n {
        // Standard colors (0-7)
        0 => "#000000",
        1 => "#800000",
        2 => "#008000",
        3 => "#808000",
        4 => "#000080",
        5 => "#800080",
        6 => "#008080",
        7 => "#c0c0c0",
        // High-intensity colors (8-15)
        8 => "#808080",
        9 => "#e06c75",  // Bright red (used by rustc for errors)
        10 => "#98c379", // Bright green
        11 => "#e5c07b", // Bright yellow
        12 => "#61afef", // Bright blue (used by rustc for line numbers)
        13 => "#c678dd", // Bright magenta
        14 => "#56b6c2", // Bright cyan
        15 => "#ffffff",
        // 216-color cube (16-231)
        16..=231 => {
            // This is a cube where each RGB component goes 0, 95, 135, 175, 215, 255
            // For simplicity, return a reasonable approximation
            "#888888"
        }
        // Grayscale (232-255)
        232..=255 => {
            // Grayscale from dark to light
            "#888888"
        }
    }
}
