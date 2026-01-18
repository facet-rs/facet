//! Proc macros for embedding Styx schemas in binaries.
//!
//! These macros compress schemas at compile time and embed them
//! with a magic header so they can be extracted without execution.

use proc_macro::{Delimiter, Group, Literal, Punct, Spacing, TokenStream, TokenTree};
use unsynn::{Comma, DelimitedVec, Parse, TokenIter};

/// Magic bytes that identify embedded Styx schemas.
const MAGIC: &[u8; 16] = b"STYX_SCHEMAS_V1\0";

/// Compress a schema and return the blob (without magic/count header).
fn compress_schema(schema: &str) -> Vec<u8> {
    let decompressed = schema.as_bytes();
    let hash = blake3::hash(decompressed);
    let compressed = lz4_flex::compress_prepend_size(decompressed);

    let mut blob = Vec::with_capacity(4 + 4 + 32 + compressed.len());
    blob.extend_from_slice(&(decompressed.len() as u32).to_le_bytes());
    blob.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    blob.extend_from_slice(hash.as_bytes());
    blob.extend_from_slice(&compressed);
    blob
}

/// Build the complete embedded blob for multiple schemas.
fn build_embedded_blob(schemas: &[String]) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(MAGIC);
    blob.extend_from_slice(&(schemas.len() as u16).to_le_bytes());

    for schema in schemas {
        blob.extend_from_slice(&compress_schema(schema));
    }

    blob
}

/// Parse a string literal (regular or raw) and return its content.
fn parse_string_literal(lit: &unsynn::Literal) -> Option<String> {
    let s = lit.to_string();

    // Raw string: r#"..."# or r"..."
    if s.starts_with("r") {
        // Find the opening quote pattern (r, r#, r##, etc.)
        let hash_count = s[1..].chars().take_while(|&c| c == '#').count();
        let prefix_len = 1 + hash_count + 1; // 'r' + hashes + '"'
        let suffix_len = 1 + hash_count; // '"' + hashes

        if s.len() >= prefix_len + suffix_len {
            return Some(s[prefix_len..s.len() - suffix_len].to_string());
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

/// Generate the static declaration for embedded schemas.
fn generate_static(blob: Vec<u8>) -> TokenStream {
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
        static __STYX_EMBEDDED_SCHEMAS: [u8; {blob_len}] = "#
    );

    let mut result: TokenStream = output.parse().unwrap();
    let array_group = TokenTree::Group(Group::new(
        Delimiter::Bracket,
        array_contents.into_iter().collect(),
    ));
    result.extend(std::iter::once(array_group));
    result.extend(";".parse::<TokenStream>().unwrap());

    result
}

/// Embed schemas from inline string literals.
///
/// # Example
///
/// ```rust,ignore
/// styx_embed::embed_inline!(r#"
/// meta { id my-schema, version 1.0.0 }
/// schema { @ @string }
/// "#);
///
/// // Multiple schemas:
/// styx_embed::embed_inline!(
///     r#"meta { id s1, version 1.0.0 } schema { @ @string }"#,
///     r#"meta { id s2, version 1.0.0 } schema { @ @int }"#,
/// );
/// ```
#[proc_macro]
pub fn embed_inline(input: TokenStream) -> TokenStream {
    let mut tokens = TokenIter::new(proc_macro2::TokenStream::from(input));

    // Parse comma-separated literals
    let literals: DelimitedVec<unsynn::Literal, Comma> = match Parse::parse(&mut tokens) {
        Ok(l) => l,
        Err(e) => {
            return format!("compile_error!(\"expected string literals: {e}\")")
                .parse()
                .unwrap()
        }
    };

    // Extract string contents
    let mut schemas = Vec::new();
    for delimited in literals.iter() {
        match parse_string_literal(&delimited.value) {
            Some(s) => schemas.push(s),
            None => {
                return "compile_error!(\"expected string literal\")"
                    .parse()
                    .unwrap()
            }
        }
    }

    if schemas.is_empty() {
        return "compile_error!(\"embed_inline! requires at least one schema\")"
            .parse()
            .unwrap();
    }

    generate_static(build_embedded_blob(&schemas))
}

/// Embed schemas from files (reads at compile time).
///
/// # Example
///
/// ```rust,ignore
/// // Single file:
/// styx_embed::embed_file!("schema.styx");
///
/// // With env var (for build script output):
/// styx_embed::embed_file!(concat!(env!("OUT_DIR"), "/schema.styx"));
///
/// // Multiple files:
/// styx_embed::embed_files!(
///     "config.styx",
///     "plugin.styx",
/// );
/// ```
#[proc_macro]
pub fn embed_file(input: TokenStream) -> TokenStream {
    let mut tokens = TokenIter::new(proc_macro2::TokenStream::from(input));

    // Parse a single literal (the path)
    let literal: unsynn::Literal = match Parse::parse(&mut tokens) {
        Ok(l) => l,
        Err(e) => {
            return format!("compile_error!(\"expected file path string: {e}\")")
                .parse()
                .unwrap()
        }
    };

    let path = match parse_string_literal(&literal) {
        Some(s) => s,
        None => {
            return "compile_error!(\"expected string literal for file path\")"
                .parse()
                .unwrap()
        }
    };

    // Read the file
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return format!("compile_error!(\"failed to read {}: {}\")", path, e)
                .parse()
                .unwrap()
        }
    };

    generate_static(build_embedded_blob(&[content]))
}

/// Embed multiple schema files (reads at compile time).
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

    // Parse comma-separated literals
    let literals: DelimitedVec<unsynn::Literal, Comma> = match Parse::parse(&mut tokens) {
        Ok(l) => l,
        Err(e) => {
            return format!("compile_error!(\"expected file path strings: {e}\")")
                .parse()
                .unwrap()
        }
    };

    // Read all files
    let mut schemas = Vec::new();
    for delimited in literals.iter() {
        let path = match parse_string_literal(&delimited.value) {
            Some(s) => s,
            None => {
                return "compile_error!(\"expected string literal for file path\")"
                    .parse()
                    .unwrap()
            }
        };

        match std::fs::read_to_string(&path) {
            Ok(content) => schemas.push(content),
            Err(e) => {
                return format!("compile_error!(\"failed to read {}: {}\")", path, e)
                    .parse()
                    .unwrap()
            }
        }
    }

    if schemas.is_empty() {
        return "compile_error!(\"embed_files! requires at least one file\")"
            .parse()
            .unwrap();
    }

    generate_static(build_embedded_blob(&schemas))
}

/// Embed a schema file from OUT_DIR (for build script output).
///
/// This macro reads `OUT_DIR` from the environment at compile time
/// and joins it with the provided filename.
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

    // Parse a single literal (the filename)
    let literal: unsynn::Literal = match Parse::parse(&mut tokens) {
        Ok(l) => l,
        Err(e) => {
            return format!("compile_error!(\"expected filename string: {e}\")")
                .parse()
                .unwrap()
        }
    };

    let filename = match parse_string_literal(&literal) {
        Some(s) => s,
        None => {
            return "compile_error!(\"expected string literal for filename\")"
                .parse()
                .unwrap()
        }
    };

    // Get OUT_DIR from environment
    let out_dir = match std::env::var("OUT_DIR") {
        Ok(dir) => dir,
        Err(_) => {
            return "compile_error!(\"OUT_DIR not set - this macro must be used in a crate with a build script\")"
                .parse()
                .unwrap()
        }
    };

    // Build full path
    let path = std::path::Path::new(&out_dir).join(&filename);
    let path_str = path.display().to_string();

    // Read the file
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return format!("compile_error!(\"failed to read {}: {}\")", path_str, e)
                .parse()
                .unwrap()
        }
    };

    generate_static(build_embedded_blob(&[content]))
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
