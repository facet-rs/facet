//! Write-ahead log (WAL) for incremental cache persistence.
//!
//! This module provides append-only persistence that writes only changes
//! instead of rewriting the entire cache. The WAL builds on top of periodic
//! snapshots created by the regular `save_cache` function.
//!
//! # Architecture
//!
//! - **Base snapshot**: Created via `save_cache`, contains full cache state at a revision
//! - **WAL entries**: Appended after snapshot, records changes since base revision
//! - **Compaction**: Periodically create new snapshot and discard old WAL
//!
//! # Limitations
//!
//! **Interned ingredients**: Interned values are NOT included in incremental WAL entries.
//! They are only persisted in base snapshots. This is because interned values are typically
//! small and infrequently changing, so the overhead of incremental tracking doesn't justify
//! the complexity. If you're using interned ingredients, be aware that new intern operations
//! between snapshots will not be persisted to the WAL.
//!
//! # Usage
//!
//! ```rust,ignore
//! // Enable WAL for a database
//! db.enable_wal("cache.wal").await?;
//!
//! // Changes are automatically appended
//! db.set_input(key, value);  // <- Appended to WAL
//!
//! // Explicit flush (normally happens automatically)
//! db.flush_wal().await?;
//!
//! // Compact when WAL grows too large
//! db.compact_wal().await?;
//!
//! // On next startup, load snapshot + replay WAL
//! db.load_from_cache("cache.bin").await?;
//! db.replay_wal("cache.wal").await?;
//! ```

use crate::{PicanteError, PicanteResult};
use facet::Facet;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Format version for the WAL file format.
/// Increment when making breaking changes to the format.
const WAL_FORMAT_VERSION: u32 = 1;

/// Magic bytes at the start of WAL files for validation.
const WAL_MAGIC: &[u8; 8] = b"PICANTE\0";

/// Header written at the start of every WAL file.
#[derive(Debug, Clone, Facet)]
pub struct WalHeader {
    /// Format version for compatibility checking
    pub format_version: u32,
    /// The revision of the base snapshot this WAL builds upon
    pub base_revision: u64,
}

/// A single entry in the write-ahead log.
#[derive(Debug, Clone, Facet)]
pub struct WalEntry {
    /// The revision when this change occurred
    pub revision: u64,
    /// The ingredient kind that owns this entry
    pub kind_id: u32,
    /// The operation performed
    pub operation: WalOperation,
}

/// Operations that can be recorded in the WAL.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum WalOperation {
    /// Insert or update a key-value pair
    Set {
        /// Serialized key
        key: Vec<u8>,
        /// Serialized value
        value: Vec<u8>,
    },
    /// Delete a key
    Delete {
        /// Serialized key
        key: Vec<u8>,
    },
}

/// Writer for append-only WAL operations.
///
/// Buffers writes in memory and flushes periodically for performance.
pub struct WalWriter {
    path: PathBuf,
    writer: BufWriter<File>,
    base_revision: u64,
    entries_since_flush: usize,
    /// Flush after this many entries (default: 100)
    pub auto_flush_threshold: usize,
}

impl WalWriter {
    /// Default auto-flush threshold (number of entries before auto-flush).
    ///
    /// This value of 100 balances write performance with data durability.
    /// Lower values increase durability but may reduce write throughput.
    /// Higher values improve performance but increase risk of data loss on crash.
    pub const DEFAULT_AUTO_FLUSH_THRESHOLD: usize = 100;

    /// Create a new WAL file with default settings.
    ///
    /// Uses the default auto-flush threshold. For custom settings, use
    /// `create_with_threshold()`.
    ///
    /// If a file already exists at this path, it will be truncated.
    pub fn create(path: impl AsRef<Path>, base_revision: u64) -> PicanteResult<Self> {
        Self::create_with_threshold(path, base_revision, Self::DEFAULT_AUTO_FLUSH_THRESHOLD)
    }

    /// Create a new WAL file with a custom auto-flush threshold.
    ///
    /// The `auto_flush_threshold` determines how many entries are buffered
    /// before automatically flushing to disk:
    ///
    /// - **Lower values (e.g., 10-50)**: Better durability, less data loss on crash,
    ///   but more disk I/O overhead
    /// - **Higher values (e.g., 200-1000)**: Better write performance, but more
    ///   entries could be lost if the process crashes before flush
    ///
    /// If a file already exists at this path, it will be truncated.
    pub fn create_with_threshold(
        path: impl AsRef<Path>,
        base_revision: u64,
        auto_flush_threshold: usize,
    ) -> PicanteResult<Self> {
        let path = path.as_ref().to_path_buf();

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| {
                Arc::new(PicanteError::Cache {
                    message: format!("Failed to create WAL file at {}: {}", path.display(), e),
                })
            })?;

        let mut writer = BufWriter::new(file);

        // Write magic bytes
        writer.write_all(WAL_MAGIC).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to write WAL magic bytes: {}", e),
            })
        })?;

        // Write header
        let header = WalHeader {
            format_version: WAL_FORMAT_VERSION,
            base_revision,
        };
        let header_bytes = facet_postcard::to_vec(&header).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "WAL header",
                message: format!("{}", e),
            })
        })?;

        // Write header length as u32, then header
        let header_len = header_bytes.len() as u32;
        writer.write_all(&header_len.to_le_bytes()).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to write WAL header length: {}", e),
            })
        })?;
        writer.write_all(&header_bytes).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to write WAL header: {}", e),
            })
        })?;

        writer.flush().map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to flush WAL header: {}", e),
            })
        })?;

        Ok(Self {
            path,
            writer,
            base_revision,
            entries_since_flush: 0,
            auto_flush_threshold,
        })
    }

    /// Append a WAL entry to the log.
    ///
    /// The entry is buffered and will be flushed when `auto_flush_threshold`
    /// is reached or when `flush()` is called explicitly.
    pub fn append(&mut self, entry: WalEntry) -> PicanteResult<()> {
        // Serialize the entry
        let entry_bytes = facet_postcard::to_vec(&entry).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "WAL entry",
                message: format!("{}", e),
            })
        })?;

        // Write entry length as u32, then entry
        let entry_len = entry_bytes.len() as u32;
        self.writer
            .write_all(&entry_len.to_le_bytes())
            .map_err(|e| {
                Arc::new(PicanteError::Cache {
                    message: format!("Failed to write WAL entry length: {}", e),
                })
            })?;
        self.writer.write_all(&entry_bytes).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to write WAL entry: {}", e),
            })
        })?;

        self.entries_since_flush += 1;

        // Auto-flush if threshold reached
        if self.entries_since_flush >= self.auto_flush_threshold {
            self.flush()?;
        }

        Ok(())
    }

    /// Flush buffered entries to disk.
    pub fn flush(&mut self) -> PicanteResult<()> {
        self.writer.flush().map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to flush WAL: {}", e),
            })
        })?;
        self.entries_since_flush = 0;
        Ok(())
    }

    /// Get the base revision this WAL builds upon.
    pub fn base_revision(&self) -> u64 {
        self.base_revision
    }

    /// Get the path to the WAL file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WalWriter {
    fn drop(&mut self) {
        // Best-effort flush on drop
        if let Err(e) = self.flush() {
            tracing::warn!(
                path = %self.path.display(),
                error = %e,
                "Failed to flush WAL on drop - unflushed entries may be lost"
            );
        }
    }
}

/// Reader for replaying WAL entries.
#[derive(Debug)]
pub struct WalReader {
    #[allow(dead_code)] // Kept for future diagnostics
    path: PathBuf,
    reader: BufReader<File>,
    header: WalHeader,
}

impl WalReader {
    /// Open an existing WAL file for reading.
    pub fn open(path: impl AsRef<Path>) -> PicanteResult<Self> {
        let path = path.as_ref().to_path_buf();

        let file = File::open(&path).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to open WAL file at {}: {}", path.display(), e),
            })
        })?;

        let mut reader = BufReader::new(file);

        // Read and validate magic bytes
        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to read WAL magic bytes: {}", e),
            })
        })?;

        if &magic != WAL_MAGIC {
            return Err(Arc::new(PicanteError::Cache {
                message: format!(
                    "Invalid WAL magic bytes (expected {:?}, got {:?})",
                    WAL_MAGIC, magic
                ),
            }));
        }

        // Read header length
        let mut header_len_bytes = [0u8; 4];
        reader.read_exact(&mut header_len_bytes).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to read WAL header length: {}", e),
            })
        })?;
        let header_len = u32::from_le_bytes(header_len_bytes) as usize;

        // Sanity check: header should not be larger than 1 MB
        const MAX_HEADER_LEN: usize = 1_000_000;
        if header_len > MAX_HEADER_LEN {
            return Err(Arc::new(PicanteError::Cache {
                message: format!(
                    "WAL header length ({} bytes) exceeds maximum allowed ({} bytes) - file may be corrupted",
                    header_len, MAX_HEADER_LEN
                ),
            }));
        }

        // Read header
        let mut header_bytes = vec![0u8; header_len];
        reader.read_exact(&mut header_bytes).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to read WAL header: {}", e),
            })
        })?;

        let header: WalHeader = facet_postcard::from_slice(&header_bytes).map_err(|e| {
            Arc::new(PicanteError::Decode {
                what: "WAL header",
                message: format!("{}", e),
            })
        })?;

        // Validate format version
        if header.format_version != WAL_FORMAT_VERSION {
            return Err(Arc::new(PicanteError::Cache {
                message: format!(
                    "Unsupported WAL format version (expected {}, got {})",
                    WAL_FORMAT_VERSION, header.format_version
                ),
            }));
        }

        Ok(Self {
            path,
            reader,
            header,
        })
    }

    /// Get the header information.
    pub fn header(&self) -> &WalHeader {
        &self.header
    }

    /// Read the next entry from the WAL.
    ///
    /// Returns `Ok(None)` when EOF is reached.
    pub fn next_entry(&mut self) -> PicanteResult<Option<WalEntry>> {
        // Try to read entry length
        let mut entry_len_bytes = [0u8; 4];
        match self.reader.read_exact(&mut entry_len_bytes) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Reached end of file
                return Ok(None);
            }
            Err(e) => {
                return Err(Arc::new(PicanteError::Cache {
                    message: format!("Failed to read WAL entry length: {}", e),
                }));
            }
        }

        let entry_len = u32::from_le_bytes(entry_len_bytes) as usize;

        // Sanity check: entry should not be larger than 100 MB
        // (A reasonable upper bound for a single cache entry)
        const MAX_ENTRY_LEN: usize = 100_000_000;
        if entry_len > MAX_ENTRY_LEN {
            return Err(Arc::new(PicanteError::Cache {
                message: format!(
                    "WAL entry length ({} bytes) exceeds maximum allowed ({} bytes) - file may be corrupted",
                    entry_len, MAX_ENTRY_LEN
                ),
            }));
        }

        // Read entry
        let mut entry_bytes = vec![0u8; entry_len];
        self.reader.read_exact(&mut entry_bytes).map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("Failed to read WAL entry: {}", e),
            })
        })?;

        let entry: WalEntry = facet_postcard::from_slice(&entry_bytes).map_err(|e| {
            Arc::new(PicanteError::Decode {
                what: "WAL entry",
                message: format!("{}", e),
            })
        })?;

        Ok(Some(entry))
    }

    /// Iterate over all entries in the WAL.
    pub fn entries(&mut self) -> WalEntryIterator<'_> {
        WalEntryIterator { reader: self }
    }
}

/// Iterator over WAL entries.
pub struct WalEntryIterator<'a> {
    reader: &'a mut WalReader,
}

impl<'a> Iterator for WalEntryIterator<'a> {
    type Item = PicanteResult<WalEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.next_entry() {
            Ok(Some(entry)) => Some(Ok(entry)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

// Tests will be added in a separate test file that has access to tempfile
