//! Schema cache for the LSP.
//!
//! Caches embedded and crate-sourced schemas to disk for editor integration
//! (go-to-definition, hover links) and offline access.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Get the schema cache directory.
///
/// - Linux/macOS: `$XDG_CACHE_HOME/styx/schemas/` (default: `~/.cache/styx/schemas/`)
/// - Windows: `%LOCALAPPDATA%\styx\cache\schemas\`
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("styx").join("schemas"))
}

/// Get the cache path for an embedded schema.
///
/// Returns `embedded/<cli-name>/<content-hash>.styx`
pub fn embedded_cache_path(cli_name: &str, content: &str) -> Option<PathBuf> {
    let hash = content_hash(content);
    cache_dir().map(|d| {
        d.join("embedded")
            .join(cli_name)
            .join(format!("{}.styx", hash))
    })
}

/// Compute a content hash for cache invalidation.
///
/// Returns first 16 hex chars of SHA-256.
fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8]) // 16 hex chars
}

/// Cache an embedded schema and return the cache file path.
///
/// If the file already exists (same hash), returns the existing path.
pub fn cache_embedded_schema(cli_name: &str, content: &str) -> Option<PathBuf> {
    let path = embedded_cache_path(cli_name, content)?;

    // Already cached?
    if path.exists() {
        return Some(path);
    }

    // Create parent directories
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok()?;
    }

    // Write content
    let mut file = fs::File::create(&path).ok()?;
    file.write_all(content.as_bytes()).ok()?;

    Some(path)
}

/// Get cache statistics.
pub struct CacheStats {
    pub embedded_count: usize,
    pub embedded_size: u64,
    pub crate_count: usize,
    pub crate_size: u64,
}

/// Get cache statistics.
pub fn cache_stats() -> Option<CacheStats> {
    let dir = cache_dir()?;

    let mut stats = CacheStats {
        embedded_count: 0,
        embedded_size: 0,
        crate_count: 0,
        crate_size: 0,
    };

    // Count embedded schemas
    let embedded_dir = dir.join("embedded");
    if embedded_dir.exists() {
        for entry in walkdir(&embedded_dir) {
            if entry.extension().is_some_and(|e| e == "styx") {
                stats.embedded_count += 1;
                stats.embedded_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    // Count crate schemas
    let crates_dir = dir.join("crates");
    if crates_dir.exists() {
        for entry in walkdir(&crates_dir) {
            if entry.extension().is_some_and(|e| e == "styx") {
                stats.crate_count += 1;
                stats.crate_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    Some(stats)
}

/// Simple recursive directory walker.
fn walkdir(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walkdir(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}

/// Clear all cached schemas.
pub fn clear_cache() -> std::io::Result<(usize, u64)> {
    let dir = cache_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "cache directory not found")
    })?;

    let stats = cache_stats();
    let count = stats
        .as_ref()
        .map(|s| s.embedded_count + s.crate_count)
        .unwrap_or(0);
    let size = stats
        .as_ref()
        .map(|s| s.embedded_size + s.crate_size)
        .unwrap_or(0);

    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }

    Ok((count, size))
}
