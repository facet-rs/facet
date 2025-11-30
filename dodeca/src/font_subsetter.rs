//! Font subsetting module
//!
//! Collects characters used per font-family from rendered HTML,
//! then subsets fonts to only include those glyphs.

use fontcull_klippa::{Plan, SubsetFlags, subset_font};
use fontcull_read_fonts::collections::IntSet;
use fontcull_skrifa::FontRef;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};

/// A parsed @font-face rule
#[derive(Debug, Clone)]
#[allow(dead_code)] // weight and style reserved for font-weight matching
pub struct FontFace {
    /// The font-family name declared in @font-face
    pub family: String,
    /// The URL to the font file (from src)
    pub src: String,
    /// Font weight (e.g., "400", "bold")
    pub weight: Option<String>,
    /// Font style (e.g., "normal", "italic")
    pub style: Option<String>,
}

/// Result of analyzing CSS for font information
#[derive(Debug, Default)]
pub struct FontAnalysis {
    /// Map of font-family name -> characters used
    pub chars_per_font: HashMap<String, HashSet<char>>,
    /// Parsed @font-face rules
    pub font_faces: Vec<FontFace>,
}

/// Analyze HTML and CSS to collect font usage information
pub fn analyze_fonts(html: &str, css: &str) -> FontAnalysis {
    let chars_per_font = collect_chars_per_font(html, css);
    let font_faces = parse_font_face_rules(css);

    FontAnalysis {
        chars_per_font,
        font_faces,
    }
}

/// Error type for font subsetting
#[derive(Debug)]
pub enum SubsetError {
    /// Failed to parse font file
    FontParse(String),
    /// Failed to subset font
    Subset(String),
    /// Failed to compress to WOFF2
    Woff2(String),
}

impl std::fmt::Display for SubsetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubsetError::FontParse(msg) => write!(f, "failed to parse font: {msg}"),
            SubsetError::Subset(msg) => write!(f, "failed to subset font: {msg}"),
            SubsetError::Woff2(msg) => write!(f, "failed to compress to WOFF2: {msg}"),
        }
    }
}

impl std::error::Error for SubsetError {}

/// Subset a font to only include the specified characters
///
/// Takes raw font data (TTF/OTF/WOFF/WOFF2) and a set of characters,
/// returns the subsetted font as TTF bytes.
pub fn subset_font_data(font_data: &[u8], chars: &HashSet<char>) -> Result<Vec<u8>, SubsetError> {
    use fontcull_skrifa::{GlyphId, Tag};
    use fontcull_write_fonts::types::NameId;

    // Parse the font
    let font = FontRef::new(font_data).map_err(|e| SubsetError::FontParse(format!("{e:?}")))?;

    // Convert chars to unicode codepoints
    let mut unicodes: IntSet<u32> = IntSet::empty();
    for c in chars {
        unicodes.insert(*c as u32);
    }

    // Empty sets for optional parameters
    let empty_gids: IntSet<GlyphId> = IntSet::empty();
    let empty_tags: IntSet<Tag> = IntSet::empty();
    let empty_name_ids: IntSet<NameId> = IntSet::empty();
    let empty_langs: IntSet<u16> = IntSet::empty();

    // Create subsetting plan
    let plan = Plan::new(
        &empty_gids, // glyph IDs - not needed when using unicodes
        &unicodes,   // unicode codepoints to keep
        &font,
        SubsetFlags::default(),
        &empty_tags,     // tables to drop
        &empty_tags,     // layout scripts
        &empty_tags,     // layout features
        &empty_name_ids, // name IDs
        &empty_langs,    // name languages
    );

    // Perform subsetting
    let subsetted = subset_font(&font, &plan).map_err(|e| SubsetError::Subset(format!("{e:?}")))?;

    Ok(subsetted)
}

/// Subset a font and compress to WOFF2
///
/// Takes raw font data and a set of characters,
/// returns the subsetted font as WOFF2 bytes.
pub fn subset_font_to_woff2(
    font_data: &[u8],
    chars: &HashSet<char>,
) -> Result<Vec<u8>, SubsetError> {
    let subsetted = subset_font_data(font_data, chars)?;

    // Compress to WOFF2
    let woff2 = woofwoof::compress(&subsetted, "", 11, true)
        .ok_or_else(|| SubsetError::Woff2("WOFF2 compression failed".to_string()))?;

    Ok(woff2)
}

/// Extracts all text content and maps it to font-families based on CSS rules.
///
/// Returns a map of font-family name -> set of characters used with that font.
pub fn collect_chars_per_font(html: &str, css: &str) -> HashMap<String, HashSet<char>> {
    let document = Html::parse_document(html);
    let font_rules = parse_font_family_rules(css);

    let mut result: HashMap<String, HashSet<char>> = HashMap::new();

    // For each element with text, determine which font-family applies
    // by checking CSS rules in order of specificity (simplified: last match wins)
    let all_elements = Selector::parse("*").unwrap();

    for element in document.select(&all_elements) {
        // Get direct text content (not from children)
        let text: String = element
            .text()
            .next()
            .map(|s| s.to_string())
            .unwrap_or_default();

        if text.trim().is_empty() {
            continue;
        }

        // Find which font-family applies to this element
        let font_family = find_font_family_for_element(&element, &font_rules)
            .unwrap_or_else(|| "sans-serif".to_string());

        // Add characters to that font's set
        let chars = result.entry(font_family).or_default();
        for c in text.chars() {
            chars.insert(c);
        }
    }

    result
}

/// A CSS rule that sets font-family
#[derive(Debug)]
struct FontFamilyRule {
    selector: String,
    font_family: String,
}

/// Parse CSS and extract rules that set font-family
fn parse_font_family_rules(css: &str) -> Vec<FontFamilyRule> {
    let mut rules = Vec::new();

    // Simple CSS parser - find rule blocks and extract font-family
    // This is a simplified parser that handles basic cases
    let chars = css.chars().peekable();
    let mut current_selector = String::new();
    let mut in_block = false;
    let mut block_content = String::new();

    for c in chars {
        if c == '{' {
            in_block = true;
            block_content.clear();
        } else if c == '}' {
            in_block = false;

            // Parse the block content for font-family
            if let Some(font_family) = extract_font_family(&block_content) {
                let selector = current_selector.trim().to_string();
                if !selector.is_empty() && !selector.starts_with('@') {
                    rules.push(FontFamilyRule {
                        selector,
                        font_family,
                    });
                }
            }

            current_selector.clear();
        } else if in_block {
            block_content.push(c);
        } else {
            current_selector.push(c);
        }
    }

    rules
}

/// Extract font-family value from a CSS declaration block
fn extract_font_family(block: &str) -> Option<String> {
    // Look for font-family: value; or font: ... value;
    for declaration in block.split(';') {
        let declaration = declaration.trim();

        if let Some(value) = declaration.strip_prefix("font-family:") {
            return Some(parse_font_family_value(value));
        }

        // Handle shorthand 'font' property (simplified - just look for font-family at end)
        if declaration.starts_with("font:") {
            // The font shorthand is complex; for now just skip it
            // TODO: properly parse font shorthand
        }
    }

    None
}

/// Parse @font-face rules from CSS
fn parse_font_face_rules(css: &str) -> Vec<FontFace> {
    let mut faces = Vec::new();

    // Find all @font-face blocks
    let mut remaining = css;
    while let Some(start) = remaining.find("@font-face") {
        remaining = &remaining[start + "@font-face".len()..];

        // Find the opening brace
        let Some(brace_start) = remaining.find('{') else {
            break;
        };
        remaining = &remaining[brace_start + 1..];

        // Find matching closing brace (handle nested braces)
        let mut depth = 1;
        let mut block_end = 0;
        for (i, c) in remaining.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        block_end = i;
                        break;
                    }
                }
                _ => {}
            }
        }

        if block_end == 0 {
            break;
        }

        let block = &remaining[..block_end];
        remaining = &remaining[block_end + 1..];

        // Parse the @font-face block
        if let Some(face) = parse_font_face_block(block) {
            faces.push(face);
        }
    }

    faces
}

/// Parse a single @font-face block content
fn parse_font_face_block(block: &str) -> Option<FontFace> {
    let mut family = None;
    let mut src = None;
    let mut weight = None;
    let mut style = None;

    for declaration in block.split(';') {
        let declaration = declaration.trim();

        if let Some(value) = declaration.strip_prefix("font-family:") {
            family = Some(parse_font_family_value(value));
        } else if let Some(value) = declaration.strip_prefix("src:") {
            src = parse_font_src(value);
        } else if let Some(value) = declaration.strip_prefix("font-weight:") {
            weight = Some(value.trim().to_string());
        } else if let Some(value) = declaration.strip_prefix("font-style:") {
            style = Some(value.trim().to_string());
        }
    }

    Some(FontFace {
        family: family?,
        src: src?,
        weight,
        style,
    })
}

/// Parse the src property of @font-face
/// Handles: url("/path/to/font.woff2"), url('/path'), url(path)
fn parse_font_src(value: &str) -> Option<String> {
    let value = value.trim();

    // Find url(...) - take the first one if there are multiple (fallbacks)
    let url_start = value.find("url(")?;
    let after_url = &value[url_start + 4..];

    // Find the closing paren
    let url_end = after_url.find(')')?;
    let url_content = &after_url[..url_end];

    // Remove quotes if present
    let url = url_content
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();

    Some(url)
}

/// Parse a font-family value, returning the first (primary) font
fn parse_font_family_value(value: &str) -> String {
    let value = value.trim();

    // font-family can be: "Font Name", 'Font Name', Font-Name, or a list
    // We take the first one
    let first = value.split(',').next().unwrap_or(value).trim();

    // Remove quotes if present
    let first = first.trim_matches('"').trim_matches('\'');

    first.to_string()
}

/// Find which font-family applies to an element based on CSS rules
fn find_font_family_for_element(
    element: &scraper::ElementRef,
    rules: &[FontFamilyRule],
) -> Option<String> {
    let mut matched_font: Option<String> = None;

    // Check each rule (later rules override earlier ones - simplified specificity)
    for rule in rules {
        if let Ok(selector) = Selector::parse(&rule.selector) {
            // Check if this element matches the selector
            if selector.matches(element) {
                matched_font = Some(rule.font_family.clone());
            }
        }
    }

    // If no direct match, check ancestors (font-family is inherited)
    if matched_font.is_none() {
        for ancestor in element.ancestors() {
            if let Some(ancestor_el) = scraper::ElementRef::wrap(ancestor) {
                for rule in rules {
                    if let Ok(selector) = Selector::parse(&rule.selector) {
                        if selector.matches(&ancestor_el) {
                            matched_font = Some(rule.font_family.clone());
                            // Don't break - later rules still override
                        }
                    }
                }
            }
            if matched_font.is_some() {
                break;
            }
        }
    }

    matched_font
}

/// Extract CSS from HTML document (from `<style>` tags and inline styles)
pub fn extract_css_from_html(html: &str) -> String {
    let document = Html::parse_document(html);
    let style_selector = Selector::parse("style").unwrap();

    let mut css = String::new();

    for style in document.select(&style_selector) {
        css.push_str(&style.inner_html());
        css.push('\n');
    }

    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_font_family_rules() {
        let css = r#"
            body { font-family: "Inter", sans-serif; }
            h1 { font-family: 'Playfair Display'; }
            .code { font-family: monospace; }
        "#;

        let rules = parse_font_family_rules(css);
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].selector, "body");
        assert_eq!(rules[0].font_family, "Inter");
        assert_eq!(rules[1].font_family, "Playfair Display");
        assert_eq!(rules[2].font_family, "monospace");
    }

    #[test]
    fn test_collect_chars_basic() {
        let html = r#"
            <html>
            <head>
                <style>
                    body { font-family: "TestFont"; }
                </style>
            </head>
            <body>
                <p>Hello</p>
            </body>
            </html>
        "#;

        let css = extract_css_from_html(html);
        let chars = collect_chars_per_font(html, &css);

        assert!(chars.contains_key("TestFont"));
        let test_font_chars = &chars["TestFont"];
        assert!(test_font_chars.contains(&'H'));
        assert!(test_font_chars.contains(&'e'));
        assert!(test_font_chars.contains(&'l'));
        assert!(test_font_chars.contains(&'o'));
    }

    #[test]
    fn test_different_fonts_for_elements() {
        let html = r#"
            <html>
            <head>
                <style>
                    body { font-family: "BodyFont"; }
                    h1 { font-family: "HeadingFont"; }
                </style>
            </head>
            <body>
                <h1>Title</h1>
                <p>Body text</p>
            </body>
            </html>
        "#;

        let css = extract_css_from_html(html);
        let chars = collect_chars_per_font(html, &css);

        // h1 should use HeadingFont
        assert!(chars.contains_key("HeadingFont"));
        assert!(chars["HeadingFont"].contains(&'T'));

        // p should inherit from body -> BodyFont
        assert!(chars.contains_key("BodyFont"));
        assert!(chars["BodyFont"].contains(&'B'));
    }

    #[test]
    fn test_parse_font_face_rules() {
        let css = r#"
            @font-face {
                font-family: "Inter";
                src: url("/fonts/Inter-Regular.woff2") format("woff2");
                font-weight: 400;
                font-style: normal;
            }

            @font-face {
                font-family: "Inter";
                src: url('/fonts/Inter-Bold.woff2');
                font-weight: 700;
            }

            @font-face {
                font-family: 'Playfair Display';
                src: url(fonts/Playfair.ttf);
            }

            body { font-family: "Inter", sans-serif; }
        "#;

        let faces = parse_font_face_rules(css);
        assert_eq!(faces.len(), 3);

        assert_eq!(faces[0].family, "Inter");
        assert_eq!(faces[0].src, "/fonts/Inter-Regular.woff2");
        assert_eq!(faces[0].weight, Some("400".to_string()));
        assert_eq!(faces[0].style, Some("normal".to_string()));

        assert_eq!(faces[1].family, "Inter");
        assert_eq!(faces[1].src, "/fonts/Inter-Bold.woff2");
        assert_eq!(faces[1].weight, Some("700".to_string()));
        assert_eq!(faces[1].style, None);

        assert_eq!(faces[2].family, "Playfair Display");
        assert_eq!(faces[2].src, "fonts/Playfair.ttf");
    }

    #[test]
    fn test_analyze_fonts_full() {
        let html = r#"
            <html>
            <head>
                <style>
                    @font-face {
                        font-family: "MyFont";
                        src: url("/fonts/MyFont.woff2");
                    }
                    body { font-family: "MyFont"; }
                </style>
            </head>
            <body>
                <p>Hello World</p>
            </body>
            </html>
        "#;

        let css = extract_css_from_html(html);
        let analysis = analyze_fonts(html, &css);

        // Should have the font-face
        assert_eq!(analysis.font_faces.len(), 1);
        assert_eq!(analysis.font_faces[0].family, "MyFont");
        assert_eq!(analysis.font_faces[0].src, "/fonts/MyFont.woff2");

        // Should have collected chars for MyFont
        assert!(analysis.chars_per_font.contains_key("MyFont"));
        let chars = &analysis.chars_per_font["MyFont"];
        assert!(chars.contains(&'H'));
        assert!(chars.contains(&'W'));
    }
}
