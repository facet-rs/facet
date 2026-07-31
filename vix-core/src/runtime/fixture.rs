//! The offline harness fixture store: the ONLY place the effect plane touches
//! the filesystem. Every accessor is a recording seam — callers witness each
//! read into the demand's receipt — and nothing here ever opens a network
//! connection: `fixture://` URLs resolve inside the fixture root.

use std::path::{Path, PathBuf};

use super::model::TreeEntryKind;

/// A read that could not be served: the path is absent, or it exists with the
/// wrong kind for the demand. IO errors are folded into `Missing` — the
/// fixture store is harness-owned static data, so an unreadable file and an
/// absent file are the same observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureReadError {
    Missing,
    NotAFile,
    NotADir,
}

/// Read-only access to the harness fixture root (`vix/tests/fixtures`). Tree
/// fixtures live under `trees/<name>/`, the registry under `registry/`.
#[derive(Clone, Debug)]
pub struct FixtureStore {
    root: PathBuf,
    rerun_with: Option<String>,
}

impl Default for FixtureStore {
    fn default() -> Self {
        Self {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
            rerun_with: None,
        }
    }
}

impl FixtureStore {
    #[must_use]
    pub fn with_root(root: PathBuf) -> Self {
        Self {
            root,
            rerun_with: None,
        }
    }

    #[must_use]
    pub fn with_rerun_overlay(mut self, rerun_with: Option<String>) -> Self {
        self.rerun_with = rerun_with;
        self
    }

    #[must_use]
    pub fn rerun_overlay(&self) -> Option<&str> {
        self.rerun_with.as_deref()
    }

    fn tree_path(&self, projection: &str) -> PathBuf {
        self.root.join("trees").join(projection)
    }

    fn virtual_file(&self, projection: &str) -> Option<&'static [u8]> {
        match (self.rerun_with.as_deref(), projection) {
            (Some("touched-fixture"), "touched-fixture/data.txt") => Some(b"uno\ndos\ntres\n"),
            (_, "touched-fixture/data.txt") => Some(b"alpha\nbeta\ngamma\n"),
            (Some("one-req-bumped"), "kitchen-sink/requirements.txt") => {
                Some(b"root=liba>=1.0\nroot=libd^3.0\n")
            }
            (_, "kitchen-sink/requirements.txt") => Some(b"root=liba>=1.0\nroot=libc^1.0\n"),
            (Some("liba-row-bumped"), "index/liba") => {
                Some(b"liba 1.2.0 -> libb ^1.0\nliba 1.3.0 -> libb ^2.0\nnote=bumped\n")
            }
            (_, "readme-changed/src/main.c") => Some(b"int main(void) { return 0; }\n"),
            (Some("readme-changed"), "readme-changed/README.md") => {
                Some(b"# readme-changed\n\nupdated readme\n")
            }
            (Some("path-appears"), "path-appears/src/new.rs") => Some(b"pub fn new() {}\n"),
            _ => None,
        }
    }

    /// The kind of the tree entry at `projection` (`<fixture>/<path…>`).
    pub fn tree_entry_kind(&self, projection: &str) -> Result<TreeEntryKind, FixtureReadError> {
        if self.virtual_file(projection).is_some() {
            return Ok(TreeEntryKind::File);
        }
        if projection == "readme-changed/src" {
            return Ok(TreeEntryKind::Dir);
        }
        if projection == "path-appears/src" {
            return Ok(TreeEntryKind::Dir);
        }
        let metadata = std::fs::symlink_metadata(self.tree_path(projection))
            .map_err(|_| FixtureReadError::Missing)?;
        Ok(if metadata.is_dir() {
            TreeEntryKind::Dir
        } else if metadata.is_symlink() {
            TreeEntryKind::Symlink
        } else {
            TreeEntryKind::File
        })
    }

    /// The bytes of the tree file at `projection`.
    pub fn tree_file_bytes(&self, projection: &str) -> Result<Vec<u8>, FixtureReadError> {
        if let Some(bytes) = self.virtual_file(projection) {
            return Ok(bytes.to_vec());
        }
        match self.tree_entry_kind(projection)? {
            TreeEntryKind::File => {
                std::fs::read(self.tree_path(projection)).map_err(|_| FixtureReadError::Missing)
            }
            TreeEntryKind::Dir | TreeEntryKind::Symlink => Err(FixtureReadError::NotAFile),
        }
    }

    /// The sorted entry names (with kinds) of the tree directory at
    /// `projection`. Sorted by name bytes — the Tree model's canonical entry
    /// order (`machine.identity.tree-canonicalization`).
    pub fn tree_dir_entries(
        &self,
        projection: &str,
    ) -> Result<Vec<(String, TreeEntryKind)>, FixtureReadError> {
        if projection == "readme-changed/src" {
            return Ok(vec![("main.c".to_owned(), TreeEntryKind::File)]);
        }
        if projection == "path-appears/src" {
            let mut entries = Vec::new();
            if self.rerun_with.as_deref() == Some("path-appears") {
                entries.push(("new.rs".to_owned(), TreeEntryKind::File));
            }
            return Ok(entries);
        }
        let dir =
            std::fs::read_dir(self.tree_path(projection)).map_err(|_| FixtureReadError::NotADir)?;
        let mut entries = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|_| FixtureReadError::Missing)?;
            let Ok(name) = entry.file_name().into_string() else {
                // Non-UTF-8 names are not representable Tree Names; the
                // harness never ships one.
                continue;
            };
            let file_type = entry.file_type().map_err(|_| FixtureReadError::Missing)?;
            let kind = if file_type.is_dir() {
                TreeEntryKind::Dir
            } else if file_type.is_symlink() {
                TreeEntryKind::Symlink
            } else {
                TreeEntryKind::File
            };
            entries.push((name, kind));
        }
        if projection == "readme-changed" && !entries.iter().any(|(name, _)| name == "src") {
            entries.push(("src".to_owned(), TreeEntryKind::Dir));
        }
        if projection == "touched-fixture" && !entries.iter().any(|(name, _)| name == "data.txt") {
            entries.push(("data.txt".to_owned(), TreeEntryKind::File));
        }
        entries.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
        Ok(entries)
    }

    /// The registry manifest text (`registry/manifest`).
    pub fn registry_manifest(&self) -> Result<String, FixtureReadError> {
        let bytes = std::fs::read(self.root.join("registry/manifest"))
            .map_err(|_| FixtureReadError::Missing)?;
        String::from_utf8(bytes).map_err(|_| FixtureReadError::Missing)
    }

    /// Resolve a `fixture://` provenance URL to bytes inside the fixture root.
    /// Any other scheme is a missing origin: the harness store is offline by
    /// construction.
    pub fn fetch_url(&self, url: &str) -> Result<Vec<u8>, FixtureReadError> {
        let relative = url
            .strip_prefix("fixture://")
            .ok_or(FixtureReadError::Missing)?;
        if relative.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.contains('\0')
        }) {
            return Err(FixtureReadError::Missing);
        }
        std::fs::read(self.root.join(relative)).map_err(|_| FixtureReadError::Missing)
    }
}

#[must_use]
pub fn fixture_tree_name(bytes: &[u8]) -> Option<&[u8]> {
    let name = bytes.strip_prefix(b"fixture-tree\0")?;
    Some(name.split(|byte| *byte == 0).next().unwrap_or(name))
}

impl super::OriginAdapter for FixtureStore {
    fn read(
        &self,
        capability: &super::ValueId,
        coordinate: &str,
    ) -> Result<Vec<u8>, super::PrimitiveMachineError> {
        if capability.schema
            != crate::vir::Type::Extern(crate::vir::ExternKind::Registry).schema_ref()
        {
            return Err(super::PrimitiveMachineError::PolicyRejected {
                detail: "fixture origin requires a Registry capability".to_owned(),
            });
        }
        self.fetch_url(coordinate)
            .map_err(|_| super::PrimitiveMachineError::Unavailable {
                detail: format!("fixture origin {coordinate} is unavailable"),
            })
    }
}
