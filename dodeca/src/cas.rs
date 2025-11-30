//! Content-addressed storage for incremental builds
//!
//! Uses rapidhash for fast hashing and canopydb for persistent storage.
//! Tracks which files have been written and their content hashes to avoid
//! unnecessary disk writes.

use camino::Utf8Path;
use canopydb::Database;
use rapidhash::fast::RapidHasher;
use std::fs;
use std::hash::Hasher;

/// Content-addressed storage for build outputs
pub struct ContentStore {
    db: Database,
}

impl ContentStore {
    /// Open or create a content store at the given path
    pub fn open(path: &Utf8Path) -> color_eyre::Result<Self> {
        // canopydb stores data in a directory
        fs::create_dir_all(path)?;
        let db = Database::new(path.as_std_path())?;
        Ok(Self { db })
    }

    /// Compute the rapidhash of content
    fn hash(content: &[u8]) -> u64 {
        let mut hasher = RapidHasher::default();
        hasher.write(content);
        hasher.finish()
    }

    /// Write content to a file if it has changed since last build.
    /// Returns true if the file was written, false if skipped (unchanged).
    pub fn write_if_changed(&self, path: &Utf8Path, content: &[u8]) -> color_eyre::Result<bool> {
        let hash = Self::hash(content);
        let hash_bytes = hash.to_le_bytes();
        let path_key = path.as_str().as_bytes();

        // Check if we have a stored hash for this path
        let unchanged = {
            let rx = self.db.begin_read()?;
            if let Some(tree) = rx.get_tree(b"hashes")? {
                if let Some(stored) = tree.get(path_key)? {
                    stored.as_ref() == hash_bytes
                } else {
                    false
                }
            } else {
                false
            }
        };

        if unchanged {
            return Ok(false);
        }

        // Hash differs or not stored - write the file
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;

        // Update stored hash
        let tx = self.db.begin_write()?;
        let mut tree = tx.get_or_create_tree(b"hashes")?;
        tree.insert(path_key, &hash_bytes)?;
        drop(tree);
        tx.commit()?;

        Ok(true)
    }
}
