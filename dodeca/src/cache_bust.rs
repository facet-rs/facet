//! Cache busting for static assets
//!
//! Hashes file content and embeds in filename for optimal browser caching.
//! Example: `main.css` → `main.a1b2c3d4.css`

use rapidhash::fast::RapidHasher;
use std::hash::Hasher;

/// Generate a short hash from content for cache busting
/// Returns 8 hex characters (32 bits of the hash)
pub fn content_hash(content: &[u8]) -> String {
    let mut hasher = RapidHasher::default();
    hasher.write(content);
    let hash = hasher.finish();
    // Take first 8 hex chars (32 bits) - enough for uniqueness, short for URLs
    format!("{:08x}", (hash >> 32) as u32)
}

/// Generate cache-busted filename
/// `fonts/Inter.woff2` + hash `a1b2c3d4` → `fonts/Inter.a1b2c3d4.woff2`
pub fn cache_busted_path(path: &str, hash: &str) -> String {
    if let Some(dot_pos) = path.rfind('.') {
        format!("{}.{}{}", &path[..dot_pos], hash, &path[dot_pos..])
    } else {
        format!("{path}.{hash}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash() {
        let hash1 = content_hash(b"hello world");
        let hash2 = content_hash(b"hello world");
        let hash3 = content_hash(b"different content");

        assert_eq!(hash1.len(), 8);
        assert_eq!(hash1, hash2); // deterministic
        assert_ne!(hash1, hash3); // different content = different hash
    }

    #[test]
    fn test_cache_busted_path() {
        assert_eq!(
            cache_busted_path("main.css", "a1b2c3d4"),
            "main.a1b2c3d4.css"
        );
        assert_eq!(
            cache_busted_path("fonts/Inter.woff2", "deadbeef"),
            "fonts/Inter.deadbeef.woff2"
        );
        assert_eq!(
            cache_busted_path("fonts/Inter-Bold.woff2", "12345678"),
            "fonts/Inter-Bold.12345678.woff2"
        );
        assert_eq!(
            cache_busted_path("noextension", "abcd1234"),
            "noextension.abcd1234"
        );
    }
}
