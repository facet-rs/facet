//! The Tree *representation* layer: how a semantic [`Tree`] enters and leaves
//! resident bytes. A tree may live at runtime as a ustar archive, a carrier,
//! or the canonical form; everything here decodes any of them to the one
//! semantic value, so identity is representation-independent
//! (`machine.identity.tree-model`). Consumed by the untar/tree-glob/exec
//! primitives and the scheduler — this is value-model machinery, not harness
//! fixture code, which is why it lives beside `tree.rs` rather than in
//! `fixture.rs`.

use super::tree::{Tree, TreeEntry, TreeError};

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

/// Why a value's resident bytes are not a Tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResidentTreeError {
    /// The bytes are neither a Tree carrier nor a parseable ustar archive.
    Malformed,
    /// The members parsed, but they do not describe a Tree — an invalid name, a
    /// path claimed twice, or a write through a non-directory.
    Model(TreeError),
}

impl std::fmt::Display for ResidentTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed => f.write_str("resident bytes are not a Tree"),
            Self::Model(error) => error.fmt(f),
        }
    }
}

/// Build the semantic [`Tree`] from parsed archive members.
///
/// Archive order is discarded on purpose: a Tree is a map, so member order is
/// transport, and two archives listing the same entries in different orders are
/// one value. `insert_path` supplies the safety properties — intermediate
/// directories are created but a non-directory is never *followed*, so a member
/// named `link/evil` cannot write through a `link` symlink that a previous
/// member introduced.
///
/// An absolute symlink target is rejected rather than stored: it is a
/// non-relocatable ambient dependency, and `machine.identity.tree-canonicalization`
/// admits only relative targets into an ordinary Tree.
///
/// r[impl machine.identity.tree-canonicalization]
pub fn tree_from_members(members: Vec<TarMember>) -> Result<Tree, TreeError> {
    let mut tree = Tree::new();
    for member in members {
        match member {
            TarMember::File {
                path,
                bytes,
                executable,
            } => {
                let entry = if executable {
                    TreeEntry::executable(bytes)
                } else {
                    TreeEntry::file(bytes)
                };
                tree.insert_path(&path, entry)?;
            }
            TarMember::Dir { path } => tree.insert_dir(&path)?,
            TarMember::Symlink { path, target } => {
                if target.starts_with('/') {
                    return Err(TreeError::AbsoluteSymlink { path, target });
                }
                tree.insert_path(&path, TreeEntry::symlink(target))?;
            }
        }
    }
    Ok(tree)
}

/// The semantic [`Tree`] of a value's resident bytes, in any representation.
///
/// Trees used to live at runtime as ustar bytes, and fixtures still ship that
/// way; the carrier is what a producer inside the machine writes; the canonical
/// form is what a producer *outside* it writes, since a primitive can only
/// intern bytes it is willing to have hashed (see `Tree::decode_canonical`). All
/// three decode to the same value here, so **identity is
/// representation-independent**: an archive and a carrier that describe the same
/// tree hash equal, which is exactly what
/// `machine.primitive.fetch-returns-a-blob` means when it says two archives
/// unpacking to one tree have one tree identity and two blob identities.
///
/// r[impl machine.identity.tree-model]
pub fn tree_from_resident(bytes: &[u8]) -> Result<Tree, ResidentTreeError> {
    if Tree::is_carrier(bytes) {
        return Tree::decode(bytes).map_err(|_| ResidentTreeError::Malformed);
    }
    // Tried before ustar and after the carrier: the three forms are disjoint on
    // their first byte (kind tag 0/1/2 vs `v` vs a path character), so this
    // cannot shadow an archive.
    if let Ok(tree) = Tree::decode_canonical(bytes) {
        return Ok(tree);
    }
    let members = parse_ustar(bytes).map_err(|_| ResidentTreeError::Malformed)?;
    tree_from_members(members).map_err(ResidentTreeError::Model)
}

/// Whether resident bytes are a content-identified tree — a carrier, the
/// canonical form, or a ustar archive — as opposed to an opaque lazily-backed
/// handle owned by an origin adapter's declared namespace. Content-identified
/// trees carry their own members and never route to any origin backend
/// (`machine.primitive.origin-routing`); the three forms are disjoint from
/// handle namespaces on their leading bytes, so recognition is total.
#[must_use]
pub fn is_content_identified_tree(bytes: &[u8]) -> bool {
    tree_from_resident(bytes).is_ok()
}

/// Canonical tree identity material, derived from the semantic [`Tree`] rather
/// than from the bytes it happened to arrive in.
///
/// Where the rows come from is the whole point: `tree_from_resident` accepts any
/// representation and they all yield one `Tree`, so **the same tree has one
/// identity in every representation**. That is the precondition for flipping a
/// producer to a different carrier without invalidating memo entries. The
/// archive's block layout, padding, and member order never enter.
///
/// Idempotent: canonical bytes in, the same bytes out.
///
/// r[impl machine.identity.tree-model]
pub fn canonical_resident_tree(bytes: &[u8]) -> Result<Vec<u8>, ResidentTreeError> {
    Ok(tree_from_resident(bytes)?.encode_canonical())
}

#[cfg(test)]
mod tests {
    use super::super::fixture::FixtureStore;
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

    /// The property the whole ustar -> carrier migration rests on: a value's
    /// identity is a property of the TREE, not of the bytes it arrived in. An
    /// archive and the carrier of the tree it unpacks to must hash equal, or
    /// flipping a producer over would silently invalidate every memo entry
    /// downstream of it.
    ///
    /// r[verify machine.identity.tree-model]
    #[test]
    fn resident_tree_identity_is_representation_independent() {
        let store = FixtureStore::default();
        let archive = store
            .fetch_url("fixture://registry/tokio-1.52.3.crate")
            .expect("fixture archive resolves");
        let from_archive = tree_from_resident(&archive).expect("archive describes a tree");
        let from_carrier =
            tree_from_resident(&from_archive.encode()).expect("carrier describes a tree");
        assert_eq!(from_archive, from_carrier, "same tree, either way in");
        assert_eq!(
            from_archive.tree_hash(),
            from_carrier.tree_hash(),
            "identity is the tree's, not the transport's"
        );
    }

    /// A carrier is recognized by its magic, so a reader can accept both
    /// representations without guessing.
    #[test]
    fn a_ustar_archive_is_not_mistaken_for_a_carrier() {
        let store = FixtureStore::default();
        let archive = store
            .fetch_url("fixture://registry/tokio-1.52.3.crate")
            .expect("fixture archive resolves");
        assert!(!Tree::is_carrier(&archive));
        let tree = tree_from_resident(&archive).expect("archive describes a tree");
        assert!(Tree::is_carrier(&tree.encode()));
    }

    /// An archive may not introduce a symlink and then write through it: the
    /// member `link/evil` finds `link` occupied by a non-directory and fails
    /// rather than escaping. This is the extraction-time half of
    /// `machine.identity.tree-canonicalization`'s "resolution may not escape the
    /// mount grant".
    ///
    /// r[verify machine.identity.tree-canonicalization]
    #[test]
    fn an_archive_cannot_write_through_a_symlink_it_declared() {
        let members = vec![
            TarMember::Symlink {
                path: "link".to_owned(),
                target: "../outside".to_owned(),
            },
            TarMember::File {
                path: "link/evil".to_owned(),
                bytes: b"pwned".to_vec(),
                executable: false,
            },
        ];
        assert!(matches!(
            tree_from_members(members),
            Err(TreeError::NotADirectory { .. })
        ));
    }

    /// An absolute target is not a relocatable value, so it does not enter an
    /// ordinary Tree at all.
    ///
    /// r[verify machine.identity.tree-canonicalization]
    #[test]
    fn an_absolute_symlink_target_is_refused() {
        let members = vec![TarMember::Symlink {
            path: "libc.so".to_owned(),
            target: "/usr/lib/libc.so.6".to_owned(),
        }];
        assert!(matches!(
            tree_from_members(members),
            Err(TreeError::AbsoluteSymlink { .. })
        ));
    }
}
