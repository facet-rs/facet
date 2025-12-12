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
}

/// Save `runtime` and `ingredients` to `path`.
pub async fn save_cache(
    path: impl AsRef<Path>,
    runtime: &Runtime,
    ingredients: &[&dyn PersistableIngredient],
) -> PicanteResult<()> {
    let path = path.as_ref();
    debug!(path = %path.display(), "save_cache: start");

    ensure_unique_kinds(ingredients)?;

    let mut sections = Vec::with_capacity(ingredients.len());
    for ingredient in ingredients {
        let records = ingredient.save_records().await?;
        sections.push(Section {
            kind_id: ingredient.kind().as_u32(),
            kind_name: ingredient.kind_name().to_string(),
            section_type: ingredient.section_type(),
            records,
        });
    }

    let cache = CacheFile {
        format_version: FORMAT_VERSION,
        current_revision: runtime.current_revision().0,
        sections,
    };

    let bytes = facet_postcard::to_vec(&cache).map_err(|e| {
        Arc::new(PicanteError::Encode {
            what: "cache file",
            message: format!("{e:?}"),
        })
    })?;

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
    let path = path.as_ref();
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

    let cache: CacheFile = facet_postcard::from_slice(&bytes).map_err(|e| {
        Arc::new(PicanteError::Decode {
            what: "cache file",
            message: format!("{e:?}"),
        })
    })?;

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
