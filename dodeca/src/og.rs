//! OG (Open Graph) image generation using Typst.
//!
//! Renders Typst templates to SVG and PNG for social media preview images.

use std::collections::HashMap;
use std::sync::OnceLock;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

/// Pixels per point for PNG rendering (2x for retina)
const PIXEL_PER_PT: f32 = 2.0;

/// Result of rendering an OG image
#[derive(Debug, Clone)]
pub struct OgImage {
    /// SVG content (preferred format)
    pub svg: String,
    /// PNG content (fallback format)
    pub png: Vec<u8>,
}

/// A minimal Typst World implementation for OG image generation.
///
/// This world provides:
/// - The standard Typst library
/// - A single source file (the template)
/// - No external file access
/// - System fonts (loaded once)
struct OgWorld {
    /// The main source file
    source: Source,
    /// Font book (metadata about available fonts)
    book: LazyHash<FontBook>,
    /// Loaded fonts
    fonts: Vec<Font>,
    /// Standard library
    library: LazyHash<Library>,
}

impl OgWorld {
    /// Create a new OG world with the given Typst source.
    fn new(source_code: String) -> Self {
        // Create the main source file
        let path = VirtualPath::new("main.typ");
        let id = FileId::new(None, path);
        let source = Source::new(id, source_code);

        // Load fonts (cached globally)
        let (book, fonts) = load_fonts();

        Self {
            source,
            book: LazyHash::new(book),
            fonts,
            library: LazyHash::new(Library::default()),
        }
    }
}

impl World for OgWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rooted_path().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().as_rooted_path().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        // Return current date
        let now = chrono::Local::now();
        Datetime::from_ymd(
            now.format("%Y").to_string().parse().ok()?,
            now.format("%m").to_string().parse().ok()?,
            now.format("%d").to_string().parse().ok()?,
        )
    }
}

/// Load system fonts (cached globally for performance).
fn load_fonts() -> (FontBook, Vec<Font>) {
    static FONTS: OnceLock<(FontBook, Vec<Font>)> = OnceLock::new();

    FONTS
        .get_or_init(|| {
            let mut book = FontBook::new();
            let mut fonts = Vec::new();

            // Search system font directories
            let font_paths = [
                "/usr/share/fonts",
                "/usr/local/share/fonts",
                "~/.fonts",
                "~/.local/share/fonts",
                // macOS
                "/System/Library/Fonts",
                "/Library/Fonts",
                "~/Library/Fonts",
                // Windows
                "C:\\Windows\\Fonts",
            ];

            for base_path in font_paths {
                let expanded = shellexpand::tilde(base_path);
                let path = std::path::Path::new(expanded.as_ref());
                if path.exists() {
                    load_fonts_from_dir(path, &mut book, &mut fonts);
                }
            }

            (book, fonts)
        })
        .clone()
}

/// Recursively load fonts from a directory.
fn load_fonts_from_dir(dir: &std::path::Path, book: &mut FontBook, fonts: &mut Vec<Font>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_fonts_from_dir(&path, book, fonts);
        } else if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                if let Ok(data) = std::fs::read(&path) {
                    let bytes = Bytes::new(data);
                    for font in Font::iter(bytes) {
                        book.push(font.info().clone());
                        fonts.push(font);
                    }
                }
            }
        }
    }
}

/// Render a Typst template to an OG image (SVG + PNG).
///
/// # Arguments
/// * `template` - The Typst template source code
/// * `vars` - Variables to substitute in the template (e.g., title, description)
///
/// # Returns
/// An `OgImage` containing both SVG and PNG representations, or an error message.
pub fn render_og_image(template: &str, vars: &HashMap<String, String>) -> Result<OgImage, String> {
    // Substitute variables in the template
    let source = substitute_vars(template, vars);

    // Create the world and compile
    let world = OgWorld::new(source);
    let result = typst::compile::<PagedDocument>(&world);

    // Handle compilation result
    let document = result.output.map_err(|errors| {
        errors
            .into_iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    // Get the first page
    let page = document
        .pages
        .first()
        .ok_or_else(|| "No pages in document".to_string())?;

    // Render to SVG
    let svg = typst_svg::svg(page);

    // Render to PNG
    let pixmap = typst_render::render(page, PIXEL_PER_PT);
    let png = pixmap
        .encode_png()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;

    Ok(OgImage { svg, png })
}

/// Substitute variables in the template.
///
/// Variables are referenced as `#var_name` in the template.
/// They are replaced with properly escaped Typst strings.
fn substitute_vars(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    for (key, value) in vars {
        // Escape the value for Typst (escape backslashes and quotes)
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        // Replace #var_name with the escaped value as a Typst string
        let pattern = format!("#let {key} = ");
        let replacement = format!("#let {key} = \"{escaped}\"");

        // If the template has a #let declaration, replace it
        if result.contains(&pattern) {
            // Find and replace the whole line
            let lines: Vec<&str> = result.lines().collect();
            let new_lines: Vec<String> = lines
                .iter()
                .map(|line| {
                    if line.trim_start().starts_with(&pattern) {
                        replacement.clone()
                    } else {
                        line.to_string()
                    }
                })
                .collect();
            result = new_lines.join("\n");
        }
    }

    result
}

/// Default OG image template.
///
/// This template creates a 1200x630 image with:
/// - A title (large, centered)
/// - A description (smaller, below title)
/// - A gradient background
pub const DEFAULT_TEMPLATE: &str = r##"#let title = "Page Title"
#let description = ""

#set page(
  width: 1200pt,
  height: 630pt,
  margin: 60pt,
  fill: gradient.linear(
    rgb("#1a1a2e"),
    rgb("#16213e"),
    angle: 135deg,
  ),
)

#set text(
  font: "Inter",
  fill: white,
)

#align(center + horizon)[
  #block(width: 100%)[
    #text(size: 64pt, weight: "bold")[#title]

    #if description != "" [
      #v(20pt)
      #text(size: 28pt, fill: rgb("#a0a0a0"))[#description]
    ]
  ]
]
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_vars() {
        let template = r#"#let title = "default"
#let desc = "default"
Hello #title"#;

        let mut vars = HashMap::new();
        vars.insert("title".to_string(), "My Title".to_string());

        let result = substitute_vars(template, &vars);
        assert!(result.contains(r#"#let title = "My Title""#));
        assert!(result.contains(r#"#let desc = "default""#));
    }

    #[test]
    fn test_escape_special_chars() {
        let template = r#"#let title = "default""#;

        let mut vars = HashMap::new();
        vars.insert("title".to_string(), r#"Test "quoted" text"#.to_string());

        let result = substitute_vars(template, &vars);
        assert!(result.contains(r#"#let title = "Test \"quoted\" text""#));
    }
}
