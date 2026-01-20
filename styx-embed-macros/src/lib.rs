//! Proc macros for embedding Styx schemas in binaries.
//!
//! These macros compress schemas at compile time and embed them
//! with a magic header so they can be extracted without execution.
//!
//! Each schema must have a `meta { id ... }` block. The ID is used to
//! generate a unique static name, allowing multiple schemas to coexist
//! in the same binary.

use proc_macro::{Delimiter, Group, Literal, Punct, Spacing, TokenStream, TokenTree};
use unsynn::{Comma, DelimitedVec, Parse, TokenIter};

/// Magic bytes that identify an embedded Styx schema.
/// 16 bytes: "STYX_SCHEMA_V2\0\0"
const MAGIC: &[u8; 16] = b"STYX_SCHEMA_V2\0\0";

/// Extract the schema ID from a parsed styx document.
///
/// Looks for `meta { id <value> }` at the root level.
fn extract_schema_id(schema: &str) -> Result<String, String> {
    let value = styx_tree::parse(schema).map_err(|e| format!("failed to parse schema: {e}"))?;

    let obj = value
        .as_object()
        .ok_or_else(|| "schema root must be an object".to_string())?;

    let meta = obj
        .get("meta")
        .ok_or_else(|| "schema must have a `meta` block".to_string())?;

    let meta_obj = meta
        .as_object()
        .ok_or_else(|| "`meta` must be an object".to_string())?;

    let id_value = meta_obj
        .get("id")
        .ok_or_else(|| "`meta` block must have an `id` field".to_string())?;

    // ID can be a bare identifier or a quoted string
    if let Some(s) = id_value.as_str() {
        return Ok(s.to_string());
    }

    Err("`meta.id` must be a string or identifier".to_string())
}

/// Sanitize an ID for the human-readable part of the symbol name.
///
/// Replaces non-alphanumeric characters with underscores.
fn sanitize_id(id: &str) -> String {
    let mut result = String::with_capacity(id.len());
    for c in id.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
        } else {
            result.push('_');
        }
    }
    // Ensure it doesn't start with a digit
    if result.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        result.insert(0, '_');
    }
    result
}

/// Generate a unique symbol suffix from a schema ID.
///
/// Format: `{sanitized_id}_{hash8}` where hash8 is 8 hex chars of blake3.
/// This gives human-readable symbols with guaranteed uniqueness.
fn id_to_symbol_suffix(id: &str) -> String {
    let sanitized = sanitize_id(id);
    let hash = blake3::hash(id.as_bytes());
    let bytes = hash.as_bytes();
    format!(
        "{}_{:02x}{:02x}{:02x}{:02x}",
        sanitized, bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

/// Build the embedded blob for a single schema.
///
/// Format (V2 - single schema per blob):
/// ```text
/// STYX_SCHEMA_V2\0\0           // 16 bytes magic
/// <decompressed_len:u32le>
/// <compressed_len:u32le>
/// <blake3:32bytes>             // hash of decompressed content
/// <lz4 compressed schema>
/// ```
fn build_embedded_blob(schema: &str) -> Vec<u8> {
    let decompressed = schema.as_bytes();
    let hash = blake3::hash(decompressed);
    let compressed = lz4_flex::compress_prepend_size(decompressed);

    let mut blob = Vec::with_capacity(16 + 4 + 4 + 32 + compressed.len());
    blob.extend_from_slice(MAGIC);
    blob.extend_from_slice(&(decompressed.len() as u32).to_le_bytes());
    blob.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    blob.extend_from_slice(hash.as_bytes());
    blob.extend_from_slice(&compressed);
    blob
}

/// Parse a string literal (regular or raw) and return its content.
fn parse_string_literal(lit: &unsynn::Literal) -> Option<String> {
    let s = lit.to_string();

    // Raw string: r#"..."# or r"..."
    if let Some(after_r) = s.strip_prefix("r") {
        // Find the opening quote pattern (r#, r##, etc.)
        let hash_count = after_r.chars().take_while(|&c| c == '#').count();
        let prefix_len = hash_count + 1; // hashes + '"'
        let suffix_len = 1 + hash_count; // '"' + hashes

        if after_r.len() >= prefix_len + suffix_len {
            return Some(after_r[prefix_len..after_r.len() - suffix_len].to_string());
        }
    }

    // Regular string: "..."
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        // Handle basic escapes
        let mut result = String::new();
        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some('\\') => result.push('\\'),
                    Some('"') => result.push('"'),
                    Some('0') => result.push('\0'),
                    Some(other) => {
                        result.push('\\');
                        result.push(other);
                    }
                    None => result.push('\\'),
                }
            } else {
                result.push(c);
            }
        }
        return Some(result);
    }

    None
}

/// Generate the static declaration for an embedded schema.
fn generate_static(schema: &str) -> Result<TokenStream, String> {
    let id = extract_schema_id(schema)?;
    let suffix = id_to_symbol_suffix(&id);
    let blob = build_embedded_blob(schema);
    let blob_len = blob.len();

    // Generate: [u8; N] = [b0, b1, b2, ...];
    let mut array_contents = Vec::new();
    for (i, byte) in blob.iter().enumerate() {
        array_contents.push(TokenTree::Literal(Literal::u8_unsuffixed(*byte)));
        if i < blob.len() - 1 {
            array_contents.push(TokenTree::Punct(Punct::new(',', Spacing::Alone)));
        }
    }

    let output = format!(
        r#"
        #[used]
        #[unsafe(no_mangle)]
        #[cfg_attr(target_os = "macos", unsafe(link_section = "__DATA,__styx_schemas"))]
        #[cfg_attr(target_os = "linux", unsafe(link_section = ".styx_schemas"))]
        #[cfg_attr(target_os = "windows", unsafe(link_section = ".styx"))]
        static __STYX_SCHEMA_{suffix}: [u8; {blob_len}] = "#
    );

    let mut result: TokenStream = output.parse().unwrap();
    let array_group = TokenTree::Group(Group::new(
        Delimiter::Bracket,
        array_contents.into_iter().collect(),
    ));
    result.extend(std::iter::once(array_group));
    result.extend(";".parse::<TokenStream>().unwrap());

    Ok(result)
}

/// Embed a schema from an inline string literal.
///
/// The schema must have a `meta { id ... }` block.
///
/// # Example
///
/// ```rust,ignore
/// styx_embed::embed_inline!(r#"
/// meta { id my-schema, version 1.0.0 }
/// schema { @ @string }
/// "#);
/// ```
#[proc_macro]
pub fn embed_inline(input: TokenStream) -> TokenStream {
    let mut tokens = TokenIter::new(proc_macro2::TokenStream::from(input));

    let literal: unsynn::Literal = match Parse::parse(&mut tokens) {
        Ok(l) => l,
        Err(e) => {
            return format!("compile_error!(\"expected string literal: {e}\")")
                .parse()
                .unwrap();
        }
    };

    let schema = match parse_string_literal(&literal) {
        Some(s) => s,
        None => {
            return "compile_error!(\"expected string literal\")".parse().unwrap();
        }
    };

    match generate_static(&schema) {
        Ok(ts) => ts,
        Err(e) => format!("compile_error!(\"{}\")", e.replace('"', "\\\""))
            .parse()
            .unwrap(),
    }
}

/// Embed a schema from a file (reads at compile time).
///
/// The schema must have a `meta { id ... }` block.
///
/// # Example
///
/// ```rust,ignore
/// styx_embed::embed_file!("schema.styx");
/// ```
#[proc_macro]
pub fn embed_file(input: TokenStream) -> TokenStream {
    let mut tokens = TokenIter::new(proc_macro2::TokenStream::from(input));

    let literal: unsynn::Literal = match Parse::parse(&mut tokens) {
        Ok(l) => l,
        Err(e) => {
            return format!("compile_error!(\"expected file path string: {e}\")")
                .parse()
                .unwrap();
        }
    };

    let path = match parse_string_literal(&literal) {
        Some(s) => s,
        None => {
            return "compile_error!(\"expected string literal for file path\")"
                .parse()
                .unwrap();
        }
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return format!("compile_error!(\"failed to read {}: {}\")", path, e)
                .parse()
                .unwrap();
        }
    };

    match generate_static(&content) {
        Ok(ts) => ts,
        Err(e) => format!("compile_error!(\"{}\")", e.replace('"', "\\\""))
            .parse()
            .unwrap(),
    }
}

/// Embed multiple schema files (reads at compile time).
///
/// Each schema must have a `meta { id ... }` block. Each generates
/// its own static with a unique name derived from the ID.
///
/// # Example
///
/// ```rust,ignore
/// styx_embed::embed_files!(
///     "config.styx",
///     "plugin.styx",
/// );
/// ```
#[proc_macro]
pub fn embed_files(input: TokenStream) -> TokenStream {
    let mut tokens = TokenIter::new(proc_macro2::TokenStream::from(input));

    let literals: DelimitedVec<unsynn::Literal, Comma> = match Parse::parse(&mut tokens) {
        Ok(l) => l,
        Err(e) => {
            return format!("compile_error!(\"expected file path strings: {e}\")")
                .parse()
                .unwrap();
        }
    };

    let mut result = TokenStream::new();

    for delimited in literals.iter() {
        let path = match parse_string_literal(&delimited.value) {
            Some(s) => s,
            None => {
                return "compile_error!(\"expected string literal for file path\")"
                    .parse()
                    .unwrap();
            }
        };

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                return format!("compile_error!(\"failed to read {}: {}\")", path, e)
                    .parse()
                    .unwrap();
            }
        };

        match generate_static(&content) {
            Ok(ts) => result.extend(ts),
            Err(e) => {
                return format!("compile_error!(\"{}\")", e.replace('"', "\\\""))
                    .parse()
                    .unwrap()
            }
        }
    }

    if result.is_empty() {
        return "compile_error!(\"embed_files! requires at least one file\")"
            .parse()
            .unwrap();
    }

    result
}

/// Embed a schema file from OUT_DIR (for build script output).
///
/// The schema must have a `meta { id ... }` block.
///
/// # Example
///
/// ```rust,ignore
/// // In build.rs:
/// // facet_styx::generate_schema::<Config>("schema.styx");
///
/// // In src/main.rs:
/// styx_embed::embed_outdir_file!("schema.styx");
/// ```
#[proc_macro]
pub fn embed_outdir_file(input: TokenStream) -> TokenStream {
    let mut tokens = TokenIter::new(proc_macro2::TokenStream::from(input));

    let literal: unsynn::Literal = match Parse::parse(&mut tokens) {
        Ok(l) => l,
        Err(e) => {
            return format!("compile_error!(\"expected filename string: {e}\")")
                .parse()
                .unwrap();
        }
    };

    let filename = match parse_string_literal(&literal) {
        Some(s) => s,
        None => {
            return "compile_error!(\"expected string literal for filename\")"
                .parse()
                .unwrap();
        }
    };

    let out_dir = match std::env::var("OUT_DIR") {
        Ok(dir) => dir,
        Err(_) => {
            return "compile_error!(\"OUT_DIR not set - this macro must be used in a crate with a build script\")"
                .parse()
                .unwrap()
        }
    };

    let path = std::path::Path::new(&out_dir).join(&filename);
    let path_str = path.display().to_string();

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return format!("compile_error!(\"failed to read {}: {}\")", path_str, e)
                .parse()
                .unwrap();
        }
    };

    match generate_static(&content) {
        Ok(ts) => ts,
        Err(e) => format!("compile_error!(\"{}\")", e.replace('"', "\\\""))
            .parse()
            .unwrap(),
    }
}

// Keep the old names as aliases for compatibility
#[proc_macro]
pub fn embed_schema(input: TokenStream) -> TokenStream {
    embed_inline(input)
}

#[proc_macro]
pub fn embed_schemas(input: TokenStream) -> TokenStream {
    embed_inline(input)
}
