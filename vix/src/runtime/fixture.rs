//! The offline harness fixture store: the ONLY place the effect plane touches
//! the filesystem. Every accessor is a recording seam — callers witness each
//! read into the demand's receipt — and nothing here ever opens a network
//! connection: `fixture://` URLs resolve inside the fixture root.

use std::path::{Path, PathBuf};

/// The kind of one directory entry, mirroring the Tree model's `TreeEntry`
/// kinds (`machine.identity.tree-model`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureEntryKind {
    File,
    Dir,
    Symlink,
}

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
            (_, "readme-changed/src/main.c") => Some(b"int main(void) { return 0; }\n"),
            (Some("readme-changed"), "readme-changed/README.md") => {
                Some(b"# readme-changed\n\nupdated readme\n")
            }
            _ => None,
        }
    }

    /// The kind of the tree entry at `projection` (`<fixture>/<path…>`).
    pub fn tree_entry_kind(&self, projection: &str) -> Result<FixtureEntryKind, FixtureReadError> {
        if self.virtual_file(projection).is_some() {
            return Ok(FixtureEntryKind::File);
        }
        if projection == "readme-changed/src" {
            return Ok(FixtureEntryKind::Dir);
        }
        let metadata = std::fs::symlink_metadata(self.tree_path(projection))
            .map_err(|_| FixtureReadError::Missing)?;
        Ok(if metadata.is_dir() {
            FixtureEntryKind::Dir
        } else if metadata.is_symlink() {
            FixtureEntryKind::Symlink
        } else {
            FixtureEntryKind::File
        })
    }

    /// The bytes of the tree file at `projection`.
    pub fn tree_file_bytes(&self, projection: &str) -> Result<Vec<u8>, FixtureReadError> {
        if let Some(bytes) = self.virtual_file(projection) {
            return Ok(bytes.to_vec());
        }
        match self.tree_entry_kind(projection)? {
            FixtureEntryKind::File => {
                std::fs::read(self.tree_path(projection)).map_err(|_| FixtureReadError::Missing)
            }
            FixtureEntryKind::Dir | FixtureEntryKind::Symlink => Err(FixtureReadError::NotAFile),
        }
    }

    /// The sorted entry names (with kinds) of the tree directory at
    /// `projection`. Sorted by name bytes — the Tree model's canonical entry
    /// order (`machine.identity.tree-canonicalization`).
    pub fn tree_dir_entries(
        &self,
        projection: &str,
    ) -> Result<Vec<(String, FixtureEntryKind)>, FixtureReadError> {
        if projection == "readme-changed/src" {
            return Ok(vec![("main.c".to_owned(), FixtureEntryKind::File)]);
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
                FixtureEntryKind::Dir
            } else if file_type.is_symlink() {
                FixtureEntryKind::Symlink
            } else {
                FixtureEntryKind::File
            };
            entries.push((name, kind));
        }
        if projection == "readme-changed" && !entries.iter().any(|(name, _)| name == "src") {
            entries.push(("src".to_owned(), FixtureEntryKind::Dir));
        }
        if projection == "touched-fixture" && !entries.iter().any(|(name, _)| name == "data.txt") {
            entries.push(("data.txt".to_owned(), FixtureEntryKind::File));
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

    /// The fixture-root-relative projection a `fixture://` URL reads, for the
    /// receipt's read witness.
    #[must_use]
    pub fn url_projection(url: &str) -> Option<&str> {
        url.strip_prefix("fixture://")
    }
}

/// One extracted archive member, in archive order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TarMember {
    File {
        path: String,
        bytes: Vec<u8>,
        executable: bool,
    },
    Dir {
        path: String,
    },
    Symlink {
        path: String,
        target: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UstarParseError {
    Malformed,
}

impl TarMember {
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::File { path, .. } | Self::Dir { path } | Self::Symlink { path, .. } => path,
        }
    }
}

fn octal_field(bytes: &[u8]) -> Option<u64> {
    let text = core::str::from_utf8(bytes).ok()?;
    let text = text.trim_matches(|c: char| c == '\0' || c == ' ');
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(text, 8).ok()
}

fn header_string(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).ok().map(str::to_owned)
}

/// Parse a plain (uncompressed) ustar archive into its members. Trailing
/// zero-block padding of any length is accepted; PAX/GNU extension records are
/// a malformed archive for this band.
pub fn parse_ustar(bytes: &[u8]) -> Result<Vec<TarMember>, UstarParseError> {
    const BLOCK: usize = 512;
    let mut members = Vec::new();
    let mut offset = 0usize;
    while offset + BLOCK <= bytes.len() {
        let header = &bytes[offset..offset + BLOCK];
        if header.iter().all(|&b| b == 0) {
            // End-of-archive: the remainder must be zero padding.
            if bytes[offset..].iter().all(|&b| b == 0) {
                return Ok(members);
            }
            return Err(UstarParseError::Malformed);
        }
        let mut path = header_string(&header[0..100]).ok_or(UstarParseError::Malformed)?;
        let prefix = header_string(&header[345..500]).unwrap_or_default();
        if !prefix.is_empty() {
            path = format!("{prefix}/{path}");
        }
        let size = octal_field(&header[124..136]).ok_or(UstarParseError::Malformed)? as usize;
        let mode = octal_field(&header[100..108]).ok_or(UstarParseError::Malformed)?;
        let typeflag = header[156];
        let data_start = offset + BLOCK;
        let data_end = data_start
            .checked_add(size)
            .ok_or(UstarParseError::Malformed)?;
        if data_end > bytes.len() {
            return Err(UstarParseError::Malformed);
        }
        match typeflag {
            b'0' | 0 => members.push(TarMember::File {
                path,
                bytes: bytes[data_start..data_end].to_vec(),
                executable: mode & 0o111 != 0,
            }),
            b'5' => members.push(TarMember::Dir {
                path: path.trim_end_matches('/').to_owned(),
            }),
            b'2' => members.push(TarMember::Symlink {
                path,
                target: header_string(&header[157..257]).ok_or(UstarParseError::Malformed)?,
            }),
            // Extension records and exotic member kinds are out of this
            // band's archive model.
            _ => return Err(UstarParseError::Malformed),
        }
        let padded = size.div_ceil(BLOCK) * BLOCK;
        offset = data_start + padded;
    }
    // No end-of-archive marker but block-aligned end is acceptable.
    if offset == bytes.len() {
        Ok(members)
    } else {
        Err(UstarParseError::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_fixture_archive() {
        let store = FixtureStore::default();
        let bytes = store
            .fetch_url("fixture://registry/tokio-1.52.3.crate")
            .expect("fixture archive resolves");
        assert_eq!(bytes.len(), 4096);
        let members = parse_ustar(&bytes).expect("fixture archive parses");
        assert_eq!(members.len(), 1);
        let TarMember::File {
            path,
            bytes,
            executable,
        } = &members[0]
        else {
            panic!("fixture archive holds one file");
        };
        assert_eq!(path, "Cargo.toml");
        assert!(!executable);
        assert!(
            core::str::from_utf8(bytes)
                .expect("manifest is utf-8")
                .contains("name = \"tokio\"")
        );
    }
}
