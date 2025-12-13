//! Cache persistence for Picante ingredients.

use crate::error::{PicanteError, PicanteResult};
use crate::key::QueryKindId;
use crate::revision::Revision;
use crate::runtime::Runtime;
use facet::Facet;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

const FORMAT_VERSION: u32 = 1;

/// Controls how Picante behaves when a cache file can't be decoded/validated.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum OnCorruptCache {
    /// Return an error from the load function.
    Error,
    /// Ignore the cache and return `Ok(false)`.
    Ignore,
    /// Delete the cache file (best effort) and return `Ok(false)`.
    Delete,
}

/// Options for loading a cache file.
#[derive(Debug, Clone)]
pub struct CacheLoadOptions {
    /// If set, rejects cache files larger than this.
    pub max_bytes: Option<usize>,
    /// Policy for decode/validation failures.
    pub on_corrupt: OnCorruptCache,
}

impl Default for CacheLoadOptions {
    fn default() -> Self {
        Self {
            max_bytes: None,
            on_corrupt: OnCorruptCache::Error,
        }
    }
}

/// Options for saving a cache file.
#[derive(Debug, Clone, Default)]
pub struct CacheSaveOptions {
    /// If set, best-effort truncates records to fit within this many bytes.
    ///
    /// Truncation prefers dropping derived records over input/interned records.
    pub max_bytes: Option<usize>,
    /// If set, truncates each section to at most this many records.
    pub max_records_per_section: Option<usize>,
    /// If set, records larger than this are skipped (best effort).
    pub max_record_bytes: Option<usize>,
}

/// Top-level cache file payload (encoded with `facet-postcard`).
#[derive(Debug, Clone, Facet)]
pub struct CacheFile {
    /// Cache format version.
    pub format_version: u32,
    /// The database's current revision at the time of the snapshot.
    pub current_revision: u64,
    /// Per-ingredient sections.
    pub sections: Vec<Section>,
}

/// A per-ingredient cache section.
#[derive(Debug, Clone, Facet)]
pub struct Section {
    /// Stable ingredient kind id.
    pub kind_id: u32,
    /// Human-readable name (debugging / mismatch detection).
    pub kind_name: String,
    /// Whether this section is for an input or a derived query.
    pub section_type: SectionType,
    /// Ingredient-defined records (each record is its own `facet-postcard` blob).
    pub records: Vec<Vec<u8>>,
}

/// Section type for persistence.
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Facet)]
pub enum SectionType {
    /// Key-value input storage.
    Input,
    /// Memoized derived query cells.
    Derived,
    /// Interned value tables.
    Interned,
}

/// An ingredient that can be saved to / loaded from a cache file.
pub trait PersistableIngredient: Send + Sync {
    /// Stable kind id (must be unique within a database).
    fn kind(&self) -> QueryKindId;
    /// Debug name (used for mismatch detection).
    fn kind_name(&self) -> &'static str;
    /// Whether this ingredient stores inputs or derived values.
    fn section_type(&self) -> SectionType;
    /// Clear all in-memory data for this ingredient.
    fn clear(&self);
    /// Serialize this ingredient's records.
    fn save_records(&self) -> BoxFuture<'_, PicanteResult<Vec<Vec<u8>>>>;
    /// Load this ingredient from raw record bytes.
    fn load_records(&self, records: Vec<Vec<u8>>) -> PicanteResult<()>;
    /// Restore any runtime-side state derived from loaded records.
    fn restore_runtime_state<'a>(
        &'a self,
        _runtime: &'a Runtime,
    ) -> BoxFuture<'a, PicanteResult<()>> {
        Box::pin(async { Ok(()) })
    }
}

/// Save `runtime` and `ingredients` to `path`.
pub async fn save_cache(
    path: impl AsRef<Path>,
    runtime: &Runtime,
    ingredients: &[&dyn PersistableIngredient],
) -> PicanteResult<()> {
    save_cache_with_options(path, runtime, ingredients, &CacheSaveOptions::default()).await
}

/// Save `runtime` and `ingredients` to `path` with cache size limits.
pub async fn save_cache_with_options(
    path: impl AsRef<Path>,
    runtime: &Runtime,
    ingredients: &[&dyn PersistableIngredient],
    options: &CacheSaveOptions,
) -> PicanteResult<()> {
    let path = path.as_ref();
    debug!(path = %path.display(), "save_cache: start");

    ensure_unique_kinds(ingredients)?;

    let mut sections = Vec::with_capacity(ingredients.len());
    for ingredient in ingredients {
        let mut records = ingredient.save_records().await?;
        if let Some(max) = options.max_record_bytes {
            let before = records.len();
            records.retain(|r| r.len() <= max);
            let dropped = before - records.len();
            if dropped != 0 {
                warn!(
                    kind = ingredient.kind().as_u32(),
                    dropped,
                    max_record_bytes = max,
                    "save_cache: skipped oversized records"
                );
            }
        }
        sections.push(Section {
            kind_id: ingredient.kind().as_u32(),
            kind_name: ingredient.kind_name().to_string(),
            section_type: ingredient.section_type(),
            records,
        });
    }

    let mut cache = CacheFile {
        format_version: FORMAT_VERSION,
        current_revision: runtime.current_revision().0,
        sections,
    };

    if let Some(max) = options.max_records_per_section {
        for section in &mut cache.sections {
            if section.records.len() > max {
                section.records.truncate(max);
            }
        }
    }

    if let Some(max_bytes) = options.max_bytes {
        shrink_cache_to_fit(&mut cache, max_bytes)?;
    }

    let bytes = encode_cache_file(&cache)?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            Arc::new(PicanteError::Cache {
                message: format!("create_dir_all {}: {e}", parent.display()),
            })
        })?;
    }

    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, &bytes).await.map_err(|e| {
        Arc::new(PicanteError::Cache {
            message: format!("write {}: {e}", tmp.display()),
        })
    })?;

    tokio::fs::rename(&tmp, path).await.map_err(|e| {
        Arc::new(PicanteError::Cache {
            message: format!("rename {} -> {}: {e}", tmp.display(), path.display()),
        })
    })?;

    info!(
        path = %path.display(),
        bytes = bytes.len(),
        rev = runtime.current_revision().0,
        "save_cache: done"
    );
    Ok(())
}

/// Load `runtime` and `ingredients` from `path`.
///
/// Returns `Ok(false)` if the cache file does not exist.
pub async fn load_cache(
    path: impl AsRef<Path>,
    runtime: &Runtime,
    ingredients: &[&dyn PersistableIngredient],
) -> PicanteResult<bool> {
    load_cache_with_options(path, runtime, ingredients, &CacheLoadOptions::default()).await
}

/// Load `runtime` and `ingredients` from `path` with a corruption policy.
///
/// Returns `Ok(false)` if the cache file does not exist, is ignored, or is deleted.
pub async fn load_cache_with_options(
    path: impl AsRef<Path>,
    runtime: &Runtime,
    ingredients: &[&dyn PersistableIngredient],
    options: &CacheLoadOptions,
) -> PicanteResult<bool> {
    match load_cache_inner(path.as_ref(), runtime, ingredients, options).await {
        Ok(v) => Ok(v),
        Err(e) => match options.on_corrupt {
            OnCorruptCache::Error => Err(e),
            OnCorruptCache::Ignore => {
                warn!(error = %e, "load_cache: ignoring corrupt cache");
                Ok(false)
            }
            OnCorruptCache::Delete => {
                warn!(error = %e, "load_cache: deleting corrupt cache");
                let path = path.as_ref();
                let _ = tokio::fs::remove_file(path).await;
                Ok(false)
            }
        },
    }
}

async fn load_cache_inner(
    path: &Path,
    runtime: &Runtime,
    ingredients: &[&dyn PersistableIngredient],
    options: &CacheLoadOptions,
) -> PicanteResult<bool> {
    debug!(path = %path.display(), "load_cache: start");

    ensure_unique_kinds(ingredients)?;

    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(Arc::new(PicanteError::Cache {
                message: format!("read {}: {e}", path.display()),
            }));
        }
    };

    if let Some(max) = options.max_bytes
        && bytes.len() > max
    {
        return Err(Arc::new(PicanteError::Cache {
            message: format!("cache file too large ({} bytes > max {max})", bytes.len()),
        }));
    }

    let cache: CacheFile = decode_cache_file(&bytes)?;

    if cache.format_version != FORMAT_VERSION {
        return Err(Arc::new(PicanteError::Cache {
            message: format!(
                "unsupported cache format version {}; expected {}",
                cache.format_version, FORMAT_VERSION
            ),
        }));
    }

    // Build lookup for provided ingredients.
    let mut by_kind: HashMap<u32, &dyn PersistableIngredient> = HashMap::new();
    for ingredient in ingredients {
        by_kind.insert(ingredient.kind().as_u32(), *ingredient);
    }

    runtime.clear_dependency_graph();

    // Clear first so we don't blend partial state.
    for ingredient in ingredients {
        ingredient.clear();
    }

    for section in cache.sections {
        let Some(ingredient) = by_kind.get(&section.kind_id).copied() else {
            warn!(
                kind_id = section.kind_id,
                kind_name = %section.kind_name,
                "load_cache: ignoring unknown section"
            );
            continue;
        };

        if section.kind_name != ingredient.kind_name() {
            return Err(Arc::new(PicanteError::Cache {
                message: format!(
                    "kind name mismatch for id {}: file has `{}`, runtime has `{}`",
                    section.kind_id,
                    section.kind_name,
                    ingredient.kind_name()
                ),
            }));
        }

        if section.section_type != ingredient.section_type() {
            return Err(Arc::new(PicanteError::Cache {
                message: format!(
                    "section type mismatch for id {} (`{}`)",
                    section.kind_id, section.kind_name
                ),
            }));
        }

        ingredient.load_records(section.records)?;
    }

    for ingredient in ingredients {
        ingredient.restore_runtime_state(runtime).await?;
    }

    runtime.set_current_revision(Revision(cache.current_revision));

    info!(
        path = %path.display(),
        bytes = bytes.len(),
        rev = runtime.current_revision().0,
        "load_cache: done"
    );
    Ok(true)
}

fn ensure_unique_kinds(ingredients: &[&dyn PersistableIngredient]) -> PicanteResult<()> {
    let mut seen = std::collections::HashSet::<u32>::new();
    for i in ingredients {
        let id = i.kind().as_u32();
        if !seen.insert(id) {
            return Err(Arc::new(PicanteError::Cache {
                message: format!("duplicate ingredient kind id {id}"),
            }));
        }
    }
    Ok(())
}

fn encode_cache_file(cache: &CacheFile) -> PicanteResult<Vec<u8>> {
    facet_postcard::to_vec(cache).map_err(|e| {
        Arc::new(PicanteError::Encode {
            what: "cache file",
            message: format!("{e:?}"),
        })
    })
}

fn decode_cache_file(bytes: &[u8]) -> PicanteResult<CacheFile> {
    facet_postcard::from_slice(bytes).map_err(|e| {
        Arc::new(PicanteError::Decode {
            what: "cache file",
            message: format!("{e:?}"),
        })
    })
}

fn shrink_cache_to_fit(cache: &mut CacheFile, max_bytes: usize) -> PicanteResult<()> {
    // Encode once to learn the real non-record overhead.
    let bytes = encode_cache_file(cache)?;
    if bytes.len() <= max_bytes {
        return Ok(());
    }

    let record_bytes = cache
        .sections
        .iter()
        .map(|s| s.records.iter().map(|r| r.len()).sum::<usize>())
        .sum::<usize>();

    let overhead = bytes.len().checked_sub(record_bytes).unwrap_or(bytes.len());

    if overhead >= max_bytes {
        return Err(Arc::new(PicanteError::Cache {
            message: format!("cache overhead ({overhead} bytes) exceeds max_bytes ({max_bytes})"),
        }));
    }

    let mut budget_for_records = max_bytes - overhead;

    // Sort records so we can pop the largest cheaply.
    for section in &mut cache.sections {
        section.records.sort_by_key(|r| r.len());
    }

    let mut current_record_bytes = record_bytes;
    while current_record_bytes > budget_for_records {
        if !drop_one_record(cache, SectionType::Derived, &mut current_record_bytes)
            && !drop_one_record(cache, SectionType::Input, &mut current_record_bytes)
            && !drop_one_record(cache, SectionType::Interned, &mut current_record_bytes)
        {
            break;
        }
    }

    // Verify we fit; if we still don't (varint/count overhead), iterate a few times.
    for _ in 0..3 {
        let bytes = encode_cache_file(cache)?;
        if bytes.len() <= max_bytes {
            info!(
                before_bytes = bytes.len(),
                max_bytes, "save_cache: cache truncated to fit"
            );
            return Ok(());
        }

        // Recompute overhead and shrink a bit more.
        let record_bytes = cache
            .sections
            .iter()
            .map(|s| s.records.iter().map(|r| r.len()).sum::<usize>())
            .sum::<usize>();
        let overhead = bytes.len().saturating_sub(record_bytes);
        if overhead >= max_bytes {
            break;
        }
        budget_for_records = max_bytes - overhead;
        current_record_bytes = record_bytes;

        while current_record_bytes > budget_for_records {
            if !drop_one_record(cache, SectionType::Derived, &mut current_record_bytes)
                && !drop_one_record(cache, SectionType::Input, &mut current_record_bytes)
                && !drop_one_record(cache, SectionType::Interned, &mut current_record_bytes)
            {
                break;
            }
        }
    }

    let bytes = encode_cache_file(cache)?;
    if bytes.len() > max_bytes {
        return Err(Arc::new(PicanteError::Cache {
            message: format!(
                "cache remains too large after truncation ({} > {max_bytes})",
                bytes.len()
            ),
        }));
    }

    Ok(())
}

fn drop_one_record(
    cache: &mut CacheFile,
    ty: SectionType,
    current_record_bytes: &mut usize,
) -> bool {
    let mut best: Option<(usize, usize)> = None; // (section_idx, record_len)
    for (idx, section) in cache.sections.iter().enumerate() {
        if section.section_type != ty {
            continue;
        }
        let Some(len) = section.records.last().map(|r| r.len()) else {
            continue;
        };
        if best.is_none_or(|(_, best_len)| len > best_len) {
            best = Some((idx, len));
        }
    }

    let Some((idx, len)) = best else {
        return false;
    };

    let section = &mut cache.sections[idx];
    section.records.pop();
    *current_record_bytes = current_record_bytes.saturating_sub(len);
    true
}
