//! Search indexing via pagefind
//!
//! Builds a full-text search index from HTML content.
//! Works entirely in memory - no files need to be written to disk.
//!
//! The indexer maintains a persistent PagefindIndex and only updates
//! pages that have changed, making incremental rebuilds fast.

use crate::db::{OutputFile, SiteOutput};
use color_eyre::eyre::eyre;
use pagefind::api::PagefindIndex;
use std::collections::{HashMap, HashSet};

/// Search index files (path -> content)
pub type SearchFiles = HashMap<String, Vec<u8>>;

/// Hash of HTML content for change detection
fn content_hash(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rapidhash::fast::RapidHasher::default();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Incremental search indexer that maintains a persistent PagefindIndex
pub struct SearchIndexer {
    /// The pagefind index - persistent across updates
    index: PagefindIndex,
    /// URLs we've indexed with their content hash (for change detection)
    indexed: HashMap<String, u64>,
}

impl SearchIndexer {
    /// Create a new search indexer
    pub fn new() -> color_eyre::Result<Self> {
        let index = PagefindIndex::new(None).map_err(|e| eyre!("pagefind init: {}", e))?;
        Ok(Self {
            index,
            indexed: HashMap::new(),
        })
    }

    /// Update the index with new site output, returning updated search files
    ///
    /// Only pages that have changed (or are new) are added to the index.
    /// Deleted pages get empty HTML added to effectively remove them from results.
    pub async fn update(&mut self, output: &SiteOutput) -> color_eyre::Result<SearchFiles> {
        // Collect current pages and their hashes
        let mut current_pages: HashMap<String, (u64, &str)> = HashMap::new();

        for file in &output.files {
            if let OutputFile::Html { route, content } = file {
                let url = if route.as_str() == "/" {
                    "/".to_string()
                } else {
                    format!("{}/", route.as_str().trim_end_matches('/'))
                };
                let hash = content_hash(content);
                current_pages.insert(url, (hash, content.as_str()));
            }
        }

        // Find pages that need updating (new or changed)
        let mut to_add: Vec<(&str, &str)> = Vec::new();
        for (url, (hash, content)) in &current_pages {
            match self.indexed.get(url) {
                Some(old_hash) if *old_hash == *hash => {
                    // Content unchanged, skip
                }
                _ => {
                    // New or changed
                    to_add.push((url.as_str(), *content));
                }
            }
        }

        // Find deleted pages (in indexed but not in current)
        let current_urls: HashSet<&str> = current_pages.keys().map(|s| s.as_str()).collect();
        let deleted: Vec<String> = self
            .indexed
            .keys()
            .filter(|url| !current_urls.contains(url.as_str()))
            .cloned()
            .collect();

        // Add new/changed pages
        for (url, content) in &to_add {
            self.index
                .add_html_file(None, Some(url.to_string()), content.to_string())
                .await
                .map_err(|e| eyre!("pagefind add {}: {}", url, e))?;
        }

        // Add empty HTML for deleted pages
        for url in &deleted {
            self.index
                .add_html_file(None, Some(url.to_string()), String::new())
                .await
                .map_err(|e| eyre!("pagefind delete {}: {}", url, e))?;
        }

        // Update our tracking
        for (url, (hash, _)) in current_pages {
            self.indexed.insert(url, hash);
        }
        for url in deleted {
            // Keep deleted URLs in indexed with hash 0 so we don't re-add empty HTML
            self.indexed.insert(url, 0);
        }

        // Rebuild and return files
        let files = self
            .index
            .get_files()
            .await
            .map_err(|e| eyre!("pagefind build: {}", e))?;

        let mut result = HashMap::new();
        for file in files {
            // Pagefind returns filenames like "pagefind.js", we need "/pagefind/pagefind.js"
            let path = format!("/pagefind/{}", file.filename.display());
            result.insert(path, file.contents);
        }

        Ok(result)
    }

    /// Get stats about the index
    #[allow(dead_code)]
    pub fn stats(&self) -> (usize, usize) {
        let total = self.indexed.len();
        let active = self.indexed.values().filter(|h| **h != 0).count();
        (active, total)
    }
}

/// Build a search index from site output (one-shot, for build mode)
pub async fn build_search_index(output: &SiteOutput) -> color_eyre::Result<SearchFiles> {
    let mut indexer = SearchIndexer::new()?;
    indexer.update(output).await
}
