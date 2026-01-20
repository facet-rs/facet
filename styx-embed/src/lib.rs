//! Embed Styx schemas in binaries for zero-execution discovery.
//!
//! This crate provides macros to embed schemas in your binary, and functions to
//! extract them without executing the binary. This enables tooling (LSP, CLI) to
//! discover schemas safely.
//!
//! # Embedding schemas
//!
//! Each schema must have a `meta { id ... }` block. The ID is used to generate
//! a unique static name, allowing multiple schemas to coexist in the same binary.
//!
//! ## Inline strings
//!
//! ```rust,ignore
//! styx_embed::embed_inline!(r#"
//! meta { id my-config, version 1.0.0 }
//! schema { @ @object{ host @string, port @int } }
//! "#);
//! ```
//!
//! ## From files
//!
//! ```rust,ignore
//! // Single file (path relative to crate root)
//! styx_embed::embed_file!("schema.styx");
//!
//! // Multiple files (each becomes its own embedded schema)
//! styx_embed::embed_files!("config.styx", "plugin.styx");
//! ```
//!
//! ## Generated from types (build script pattern)
//!
//! For schemas derived from Rust types using facet-styx, use a build script:
//!
//! ```rust,ignore
//! // build.rs
//! fn main() {
//!     facet_styx::GenerateSchema::<MyConfig>::new()
//!         .crate_name("myapp-config")
//!         .version("1")
//!         .cli("myapp")
//!         .write("schema.styx");
//! }
//!
//! // src/main.rs
//! styx_embed::embed_outdir_file!("schema.styx");
//! ```
//!
//! This keeps the schema in sync with your types automatically.
//!
//! # Binary format (V2)
//!
//! Each embedded schema is stored as its own blob:
//!
//! ```text
//! STYX_SCHEMA_V2\0\0           // 16 bytes magic
//! <decompressed_len:u32le>
//! <compressed_len:u32le>
//! <blake3:32bytes>             // hash of decompressed content
//! <lz4 compressed schema>
//! ```
//!
//! Multiple schemas in a binary means multiple blobs, each with its own magic header.
//! The schema's `meta { id ... }` is used to identify which schema is which.
//!
//! # Extracting schemas
//!
//! ```rust,ignore
//! use styx_embed::extract_schemas;
//!
//! let schemas = extract_schemas(binary_bytes)?;
//! for schema in schemas {
//!     println!("{}", schema);
//! }
//! ```

// Re-export the proc macros
pub use styx_embed_macros::{
    embed_file, embed_files, embed_inline, embed_outdir_file, embed_schema, embed_schemas,
};

/// Magic bytes that identify an embedded Styx schema (V2 format).
/// 16 bytes: "STYX_SCHEMA_V2\0\0"
pub const MAGIC_V2: &[u8; 16] = b"STYX_SCHEMA_V2\0\0";

/// Magic bytes for legacy V1 format (multiple schemas per blob).
/// 16 bytes: "STYX_SCHEMAS_V1\0"
pub const MAGIC_V1: &[u8; 16] = b"STYX_SCHEMAS_V1\0";

/// Error type for schema extraction.
#[derive(Debug)]
pub enum ExtractError {
    /// Magic bytes not found in binary.
    NotFound,
    /// Binary is truncated or malformed.
    Truncated,
    /// LZ4 decompression failed.
    DecompressFailed,
    /// BLAKE3 hash mismatch (data corruption or false positive match).
    HashMismatch,
    /// Decompressed data is not valid UTF-8.
    InvalidUtf8,
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::NotFound => write!(f, "no embedded styx schemas found"),
            ExtractError::Truncated => write!(f, "embedded schema data is truncated"),
            ExtractError::DecompressFailed => write!(f, "LZ4 decompression failed"),
            ExtractError::HashMismatch => write!(f, "BLAKE3 hash mismatch"),
            ExtractError::InvalidUtf8 => write!(f, "schema is not valid UTF-8"),
        }
    }
}

impl std::error::Error for ExtractError {}

/// Compress a schema and return the blob (for testing).
pub fn compress_schema(schema: &str) -> Vec<u8> {
    let decompressed = schema.as_bytes();
    let hash = blake3::hash(decompressed);
    let compressed = lz4_flex::compress_prepend_size(decompressed);

    let mut blob = Vec::with_capacity(16 + 4 + 4 + 32 + compressed.len());
    blob.extend_from_slice(MAGIC_V2);
    blob.extend_from_slice(&(decompressed.len() as u32).to_le_bytes());
    blob.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    blob.extend_from_slice(hash.as_bytes());
    blob.extend_from_slice(&compressed);
    blob
}

/// Build the complete embedded blob for a single schema (V2 format).
pub fn build_embedded_blob(schema: &str) -> Vec<u8> {
    compress_schema(schema)
}

/// Extract all schemas from binary data.
///
/// Scans for magic bytes and extracts all embedded schemas found.
/// In V2 format, each schema has its own blob with its own magic header.
///
/// Returns an error only if no schemas are found at all.
pub fn extract_schemas(data: &[u8]) -> Result<Vec<String>, ExtractError> {
    let mut schemas = Vec::new();
    let mut search_start = 0;

    // Find all V2 blobs
    while let Some(magic_pos) = find_magic_from(data, search_start, MAGIC_V2) {
        match try_extract_v2_at(data, magic_pos) {
            Ok(schema) => {
                schemas.push(schema);
                // Continue searching after this blob
                search_start = magic_pos + MAGIC_V2.len();
            }
            Err(_) => {
                // False positive (e.g., magic in debug symbols), try next
                search_start = magic_pos + 1;
            }
        }
    }

    // Also try legacy V1 format for backwards compatibility
    search_start = 0;
    while let Some(magic_pos) = find_magic_from(data, search_start, MAGIC_V1) {
        match try_extract_v1_at(data, magic_pos) {
            Ok(mut v1_schemas) => {
                schemas.append(&mut v1_schemas);
                search_start = magic_pos + MAGIC_V1.len();
            }
            Err(_) => {
                search_start = magic_pos + 1;
            }
        }
    }

    if schemas.is_empty() {
        Err(ExtractError::NotFound)
    } else {
        Ok(schemas)
    }
}

/// Try to extract a single schema from V2 format at a specific position.
fn try_extract_v2_at(data: &[u8], magic_pos: usize) -> Result<String, ExtractError> {
    let mut pos = magic_pos + MAGIC_V2.len();

    // Read header: decompressed_len (4) + compressed_len (4) + hash (32) = 40 bytes
    if pos + 40 > data.len() {
        return Err(ExtractError::Truncated);
    }

    let decompressed_len =
        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;

    let compressed_len =
        u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
    pos += 4;

    let expected_hash: [u8; 32] = data[pos..pos + 32]
        .try_into()
        .map_err(|_| ExtractError::Truncated)?;
    pos += 32;

    // Read compressed data
    if pos + compressed_len > data.len() {
        return Err(ExtractError::Truncated);
    }
    let compressed = &data[pos..pos + compressed_len];

    // Decompress
    let decompressed = lz4_flex::decompress_size_prepended(compressed)
        .map_err(|_| ExtractError::DecompressFailed)?;

    // Verify length
    if decompressed.len() != decompressed_len {
        return Err(ExtractError::DecompressFailed);
    }

    // Verify hash
    let actual_hash = blake3::hash(&decompressed);
    if actual_hash.as_bytes() != &expected_hash {
        return Err(ExtractError::HashMismatch);
    }

    // Convert to string
    String::from_utf8(decompressed).map_err(|_| ExtractError::InvalidUtf8)
}

/// Try to extract schemas from legacy V1 format at a specific position.
fn try_extract_v1_at(data: &[u8], magic_pos: usize) -> Result<Vec<String>, ExtractError> {
    let mut pos = magic_pos + MAGIC_V1.len();

    // Read count
    if pos + 2 > data.len() {
        return Err(ExtractError::Truncated);
    }
    let count = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
    pos += 2;

    let mut schemas = Vec::with_capacity(count);

    for _ in 0..count {
        // Read header: decompressed_len (4) + compressed_len (4) + hash (32) = 40 bytes
        if pos + 40 > data.len() {
            return Err(ExtractError::Truncated);
        }

        let decompressed_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let compressed_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let expected_hash: [u8; 32] = data[pos..pos + 32]
            .try_into()
            .map_err(|_| ExtractError::Truncated)?;
        pos += 32;

        // Read compressed data
        if pos + compressed_len > data.len() {
            return Err(ExtractError::Truncated);
        }
        let compressed = &data[pos..pos + compressed_len];
        pos += compressed_len;

        // Decompress
        let decompressed = lz4_flex::decompress_size_prepended(compressed)
            .map_err(|_| ExtractError::DecompressFailed)?;

        // Verify length
        if decompressed.len() != decompressed_len {
            return Err(ExtractError::DecompressFailed);
        }

        // Verify hash
        let actual_hash = blake3::hash(&decompressed);
        if actual_hash.as_bytes() != &expected_hash {
            return Err(ExtractError::HashMismatch);
        }

        // Convert to string
        let schema = String::from_utf8(decompressed).map_err(|_| ExtractError::InvalidUtf8)?;
        schemas.push(schema);
    }

    Ok(schemas)
}

/// Find the position of magic bytes in the data, starting from an offset.
fn find_magic_from(data: &[u8], start: usize, magic: &[u8; 16]) -> Option<usize> {
    if start >= data.len() {
        return None;
    }
    data[start..]
        .windows(magic.len())
        .position(|w| w == magic)
        .map(|pos| start + pos)
}

/// Section names used for embedding schemas in different object formats.
mod section_names {
    /// ELF section name (Linux)
    pub const ELF: &str = ".styx_schemas";
    /// Mach-O segment name (macOS)
    pub const MACHO_SEGMENT: &str = "__DATA";
    /// Mach-O section name (macOS)
    pub const MACHO_SECTION: &str = "__styx_schemas";
    /// PE/COFF section name (Windows)
    pub const PE: &str = ".styx";
}

/// Extract schemas from binary data using object format parsing.
///
/// Parses ELF, Mach-O, or PE headers to locate the embedded schema section
/// directly, avoiding a full binary scan. Falls back to magic byte scanning
/// if the object format is unknown or section not found.
pub fn extract_schemas_from_object(data: &[u8]) -> Result<Vec<String>, ExtractError> {
    use goblin::Object;

    // Try to parse as a known object format
    if let Ok(object) = Object::parse(data)
        && let Some(section_data) = find_schema_section(&object, data)
    {
        // Found the section - extract directly from it
        return extract_schemas(section_data);
    }

    // Fall back to magic byte scanning for unknown formats or missing section
    extract_schemas(data)
}

/// Find the schema section in a parsed object file.
fn find_schema_section<'a>(object: &goblin::Object, data: &'a [u8]) -> Option<&'a [u8]> {
    use goblin::Object;

    match object {
        Object::Elf(elf) => find_elf_section(elf, data),
        Object::Mach(mach) => find_macho_section(mach, data),
        Object::PE(pe) => find_pe_section(pe, data),
        _ => None,
    }
}

/// Find the .styx_schemas section in an ELF binary.
fn find_elf_section<'a>(elf: &goblin::elf::Elf, data: &'a [u8]) -> Option<&'a [u8]> {
    for section in &elf.section_headers {
        if let Some(name) = elf.shdr_strtab.get_at(section.sh_name)
            && name == section_names::ELF
        {
            let start = section.sh_offset as usize;
            let size = section.sh_size as usize;
            if start + size <= data.len() {
                return Some(&data[start..start + size]);
            }
        }
    }
    None
}

/// Find the __DATA,__styx_schemas section in a Mach-O binary.
fn find_macho_section<'a>(mach: &goblin::mach::Mach, data: &'a [u8]) -> Option<&'a [u8]> {
    use goblin::mach::Mach;

    match mach {
        Mach::Binary(macho) => find_macho_section_in_binary(macho, data),
        Mach::Fat(fat) => {
            // For fat binaries, try each architecture
            for arch in fat.iter_arches().flatten() {
                let start = arch.offset as usize;
                let size = arch.size as usize;
                if start + size <= data.len() {
                    let arch_data = &data[start..start + size];
                    if let Ok(goblin::Object::Mach(Mach::Binary(macho))) =
                        goblin::Object::parse(arch_data)
                        && let Some(section) = find_macho_section_in_binary(&macho, arch_data)
                    {
                        return Some(section);
                    }
                }
            }
            None
        }
    }
}

/// Find the section in a single Mach-O binary (not fat).
fn find_macho_section_in_binary<'a>(
    macho: &goblin::mach::MachO,
    data: &'a [u8],
) -> Option<&'a [u8]> {
    for segment in &macho.segments {
        if let Ok(name) = segment.name()
            && name == section_names::MACHO_SEGMENT
        {
            for (section, _section_data) in segment.sections().ok()? {
                if let Ok(sect_name) = section.name()
                    && sect_name == section_names::MACHO_SECTION
                {
                    let start = section.offset as usize;
                    let size = section.size as usize;
                    if start + size <= data.len() {
                        return Some(&data[start..start + size]);
                    }
                }
            }
        }
    }
    None
}

/// Find the .styx section in a PE binary.
fn find_pe_section<'a>(pe: &goblin::pe::PE, data: &'a [u8]) -> Option<&'a [u8]> {
    for section in &pe.sections {
        if let Ok(name) = section.name()
            && name == section_names::PE
        {
            let start = section.pointer_to_raw_data as usize;
            let size = section.size_of_raw_data as usize;
            if start + size <= data.len() {
                return Some(&data[start..start + size]);
            }
        }
    }
    None
}

/// Extract schemas from a file by memory-mapping it.
///
/// Uses object format parsing to locate the schema section directly.
/// Falls back to magic byte scanning if the format is unknown.
pub fn extract_schemas_from_file(
    path: &std::path::Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    use std::fs::File;
    let file = File::open(path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file) }?;
    Ok(extract_schemas_from_object(&mmap)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_single_schema_v2() {
        let schema = r#"meta {
  id test-schema
  version 1.0.0
}

schema {
  @ @object{
    name @string
    port @int
  }
}
"#;

        let blob = build_embedded_blob(schema);
        let extracted = extract_schemas(&blob).unwrap();

        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0], schema);
    }

    #[test]
    fn multiple_v2_blobs() {
        let schema1 = "meta { id s1, version 1.0.0 }\nschema { @ @string }";
        let schema2 = "meta { id s2, version 2.0.0 }\nschema { @ @int }";

        // Concatenate two V2 blobs (simulating multiple embedded schemas)
        let mut data = build_embedded_blob(schema1);
        data.extend(build_embedded_blob(schema2));

        let extracted = extract_schemas(&data).unwrap();

        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted[0], schema1);
        assert_eq!(extracted[1], schema2);
    }

    #[test]
    fn not_found_in_random_data() {
        let data = vec![0u8; 1000];
        assert!(matches!(
            extract_schemas(&data),
            Err(ExtractError::NotFound)
        ));
    }

    #[test]
    fn embedded_in_larger_binary() {
        let schema = "meta { id test, version 1.0.0 }\nschema { @ @bool }";

        // Simulate a binary with stuff before and after
        let mut binary = vec![0xDE, 0xAD, 0xBE, 0xEF]; // header
        binary.extend_from_slice(&[0u8; 1000]); // padding
        binary.extend_from_slice(&build_embedded_blob(schema));
        binary.extend_from_slice(&[0u8; 500]); // trailing data

        let extracted = extract_schemas(&binary).unwrap();
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0], schema);
    }

    #[test]
    fn hash_mismatch_detected() {
        let schema = "meta { id test, version 1.0.0 }\nschema { @ @unit }";
        let mut blob = build_embedded_blob(schema);

        // Corrupt the hash (bytes 16+8 = 24 onwards is the hash)
        let hash_start = MAGIC_V2.len() + 4 + 4;
        blob[hash_start] ^= 0xFF;

        assert!(matches!(
            extract_schemas(&blob),
            Err(ExtractError::NotFound) // No valid schemas found
        ));
    }
}
