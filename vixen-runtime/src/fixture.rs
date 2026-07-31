//! The offline harness's fixture vocabulary: the `fixture_tree` /
//! `fixture_registry` surface spellings (injected constant surfaces), the
//! [`FixtureStore`] the harness installs as its origin adapter, and the
//! `fixture-tree\0` handle namespace — spelled in exactly one place, this
//! module.
//!
//! These are HARNESS vocabulary, not language vocabulary: they name entries
//! in the harness's fixture root (`vixen-runtime/tests/fixtures`), their
//! values re-verify against the store through the origin seam, and a
//! production embedding declares none of it. `vix-core` spells no fixture
//! name anywhere — the machine sees a declared constant, a declared origin
//! adapter, and a declared tree-handle namespace.
//!
//! The value identities are the coordinate encodings, unchanged from the
//! retired dedicated ops (acceptance 1 of the origin-rail note): a fixture
//! tree is the `Tree`-framed handle `fixture-tree\0<name>` — the identity of
//! its *coordinate*, not its content, which is the point: the store exists
//! to simulate "the world changed under the same name", and the rerun audit
//! re-verifies the receipts against live content. The registry is the
//! `Registry`-framed leaf `fixture-registry`.

use std::path::{Path, PathBuf};

use vix::binding::ConstantSurfaceDecl;
use vix::runtime::{
    OriginAdapter, OriginAdapterDecl, OriginReadError, OriginTreeError, TreeEntryKind, ValueId,
};
use vix::schema::SchemaPattern;
use vix::vir::{ExternKind, Type};

/// The byte prefix of fixture tree handles — the fixture adapter's declared
/// tree-handle namespace ([`FixtureStore::origin_decl`]) and the prefix the
/// `fixture_tree` constant surface encodes. One spelling, one place.
const FIXTURE_TREE_NAMESPACE: &[u8] = b"fixture-tree\0";

/// The `Tree` host type the fixture-tree constant is framed as.
fn fixture_tree_type() -> Type {
    Type::Extern(ExternKind::Host(vix::binding::TREE))
}

/// Encode `fixture_tree(name)`'s one literal into the lazy handle bytes.
fn encode_fixture_tree(args: &[&str]) -> Vec<u8> {
    let mut bytes = FIXTURE_TREE_NAMESPACE.to_vec();
    bytes.extend(args[0].as_bytes());
    bytes
}

fn fixture_registry_type() -> Type {
    Type::Extern(ExternKind::Registry)
}

fn encode_fixture_registry(_args: &[&str]) -> Vec<u8> {
    b"fixture-registry".to_vec()
}

/// The two fixture surfaces, as injected constant-surface declarations.
///
/// `fixture_tree` declares the literal-argument constraint the review
/// amendment demanded (every constant-surface parameter is a declared
/// literal): a computed fixture coordinate is rejected at lowering, so a
/// harness's fixture requirement set stays static.
///
/// The publication shapes mirror the retired ops exactly: `fixture_tree` is
/// an in-frame realized constant (non-root), `fixture_registry` is its own
/// scheduler-published effect island (root) — downstream registry consumers
/// take its value as a published input.
pub const FIXTURE_CONSTANTS: &[ConstantSurfaceDecl] = &[
    ConstantSurfaceDecl {
        name: "fixture_tree",
        literal_params: 1,
        result: fixture_tree_type,
        encode: encode_fixture_tree,
        root: false,
    },
    ConstantSurfaceDecl {
        name: "fixture_registry",
        literal_params: 0,
        result: fixture_registry_type,
        encode: encode_fixture_registry,
        root: true,
    },
];

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

/// Read-only access to the harness fixture root
/// (`vixen-runtime/tests/fixtures`). Tree fixtures live under
/// `trees/<name>/`, the registry under `registry/`. The ONLY place the
/// harness's effect plane touches the filesystem, and nothing here ever
/// opens a network connection: `fixture://` URLs resolve inside the root.
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

    /// The virtual-file overlay: harness data simulating "the world changed
    /// under the same name" between a run and its rerun audit. Declared here,
    /// with the store — never resident in any `Runtime`.
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

    /// The fixture adapter's declaration: `fixture://` coordinates, Registry
    /// capabilities, and the `fixture-tree\0` handle namespace, as data. The
    /// machine routes by this declaration; the adapter re-checks none of it.
    ///
    /// r[impl machine.primitive.origin-routing]
    #[must_use]
    pub fn origin_decl() -> OriginAdapterDecl {
        OriginAdapterDecl {
            name: "fixture".to_owned(),
            schemes: vec!["fixture".to_owned()],
            capability: SchemaPattern::exact(&Type::Extern(ExternKind::Registry).schema_ref()),
            tree_namespace: Some(FIXTURE_TREE_NAMESPACE.to_vec()),
        }
    }
}

impl OriginAdapter for FixtureStore {
    fn read(&self, _capability: &ValueId, coordinate: &str) -> Result<Vec<u8>, OriginReadError> {
        self.fetch_url(coordinate)
            .map_err(|_| OriginReadError::Miss {
                detail: format!("fixture origin {coordinate} is unavailable"),
            })
    }

    fn tree_kind(&self, _handle: &[u8], path: &str) -> Result<TreeEntryKind, OriginTreeError> {
        self.tree_entry_kind(path)
            .map_err(|_| OriginTreeError::Missing)
    }

    fn tree_bytes(&self, handle: &[u8], path: &str) -> Result<Vec<u8>, OriginTreeError> {
        match self.tree_kind(handle, path)? {
            TreeEntryKind::File => self
                .tree_file_bytes(path)
                .map_err(|_| OriginTreeError::Missing),
            found @ (TreeEntryKind::Dir | TreeEntryKind::Symlink) => {
                Err(OriginTreeError::WrongKind { found })
            }
        }
    }

    fn tree_directory(
        &self,
        handle: &[u8],
        path: &str,
    ) -> Result<Vec<(String, TreeEntryKind)>, OriginTreeError> {
        match self.tree_kind(handle, path)? {
            TreeEntryKind::Dir => self
                .tree_dir_entries(path)
                .map_err(|_| OriginTreeError::Missing),
            found @ (TreeEntryKind::File | TreeEntryKind::Symlink) => {
                Err(OriginTreeError::WrongKind { found })
            }
        }
    }
}
