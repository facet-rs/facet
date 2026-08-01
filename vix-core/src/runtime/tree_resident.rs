//! The Tree *representation* layer: how a semantic [`Tree`] enters and leaves
//! resident bytes. A tree may live at runtime as a ustar archive, a carrier,
//! or the canonical form; everything here decodes any of them to the one
//! semantic value, so identity is representation-independent
//! (`machine.identity.tree-model`). Consumed by the untar/tree-glob/exec
//! primitives and the scheduler — this is value-model machinery, not any
//! backend's code, which is why it lives beside `tree.rs` rather than with
//! the harness's store.

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
/// Trees used to live at runtime as ustar bytes, and harness data still ships that
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

/// The ustar magic at byte offset 257 of the first header block. Recognition
/// vocabulary only — [`parse_ustar`] validates structurally; every producer
/// this machine admits (GNU/POSIX tar, the harness's shipped archives)
/// writes the magic.
const USTAR_MAGIC_OFFSET: usize = 257;
const USTAR_MAGIC: &[u8] = b"ustar";

/// Whether resident bytes are RECOGNIZED as a content-identified tree — a
/// carrier, the canonical form, or a ustar archive — as opposed to an opaque
/// lazily-backed handle owned by an origin adapter's declared namespace
/// (`machine.primitive.origin-routing`: content-identified trees never route
/// to any backend).
///
/// Recognition is by cheap markers, deliberately NOT by parsing:
/// - the carrier magic (`vix-tree\0\x01`) at offset 0;
/// - a canonical entry tag (byte 0/1/2) first — including the empty
///   canonical encoding (empty bytes ARE the empty tree);
/// - the ustar magic (`ustar`) at offset 257.
///
/// What the markers do and do not guarantee: recognition claims only "this
/// routes as content, never to an adapter" — a recognized resident that then
/// fails to PARSE is a loud malformed-content error, not a handle to retry
/// against a backend. Marker recognition and handle namespaces stay disjoint
/// because `OriginAdapterSet::install` REJECTS a declared namespace that
/// carries a content marker or prefixes the carrier magic
/// (`OriginInstallError::ContentMarkedNamespace`); the one residual overlap
/// — a handle whose NAME stretches past offset 257 and spells `ustar` there
/// — routes as content and dies loudly as malformed content, the naming
/// author's own doing. What the seam DOES guarantee is that a routing
/// decision costs a marker probe, not a full archive parse — and that a
/// content read parses once, not twice (route, then read), which full-parse
/// recognition used to do.
#[must_use]
pub fn is_content_identified_tree(bytes: &[u8]) -> bool {
    if Tree::is_carrier(bytes) {
        return true;
    }
    if matches!(bytes.first(), None | Some(0..=2)) {
        return true;
    }
    bytes.len() >= USTAR_MAGIC_OFFSET + USTAR_MAGIC.len()
        && &bytes[USTAR_MAGIC_OFFSET..USTAR_MAGIC_OFFSET + USTAR_MAGIC.len()] == USTAR_MAGIC
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
    use super::*;

    /// A real 4096-byte plain-tar archive (one `Cargo.toml` member), checked
    /// in as core test data: the ustar/carrier machinery is core's, so its
    /// sample bytes are too. The harness ships the same archive in its own
    /// registry data, pinned by hash there.
    const SAMPLE_ARCHIVE: &[u8] = include_bytes!("testdata/tokio-1.52.3.tar");

    #[test]
    fn parses_the_sample_archive() {
        let bytes = SAMPLE_ARCHIVE;
        assert_eq!(bytes.len(), 4096);
        let members = parse_ustar(bytes).expect("sample archive parses");
        assert_eq!(members.len(), 1);
        let TarMember::File {
            path,
            bytes,
            executable,
        } = &members[0]
        else {
            panic!("sample archive holds one file");
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
        let from_archive = tree_from_resident(SAMPLE_ARCHIVE).expect("archive describes a tree");
        let from_carrier =
            tree_from_resident(&from_archive.encode()).expect("carrier describes a tree");
        assert_eq!(from_archive, from_carrier, "same tree, either way in");
        assert_eq!(
            from_archive.tree_hash(),
            from_carrier.tree_hash(),
            "identity is the tree's, not the transport's"
        );
    }

    /// r[verify machine.primitive.origin-routing] — content recognition is a
    /// marker probe, and it recognizes every legitimate representation while
    /// refusing opaque handle bytes: the three content forms route as
    /// content, a namespaced handle does not.
    #[test]
    fn content_recognition_is_by_marker_and_refuses_handle_bytes() {
        // All three legitimate representations of the same tree.
        let tree = tree_from_resident(SAMPLE_ARCHIVE).expect("archive describes a tree");
        assert!(is_content_identified_tree(SAMPLE_ARCHIVE), "ustar magic");
        assert!(is_content_identified_tree(&tree.encode()), "carrier magic");
        assert!(
            is_content_identified_tree(&tree.encode_canonical()),
            "canonical tag byte"
        );
        // The empty canonical encoding IS the empty tree.
        assert!(is_content_identified_tree(b""));

        // An opaque handle in a declared-namespace shape is NOT content —
        // it must route to (or refuse through) the adapter set, never fall
        // into content enumeration.
        assert!(!is_content_identified_tree(b"sample-tree\0small-crate"));
    }

    /// Recognition is not validation: bytes wearing the ustar marker but
    /// failing to parse are a RECOGNIZED, malformed content tree — a loud
    /// parse error, never an unclaimed handle silently retried against a
    /// backend.
    #[test]
    fn a_marked_but_malformed_resident_is_malformed_content_not_a_handle() {
        let mut forged = vec![b'x'; 512];
        forged[257..262].copy_from_slice(b"ustar");
        assert!(
            is_content_identified_tree(&forged),
            "the marker recognizes it as content"
        );
        assert!(
            matches!(
                tree_from_resident(&forged),
                Err(ResidentTreeError::Malformed)
            ),
            "and the parse rejects it loudly"
        );
    }

    /// A carrier is recognized by its magic, so a reader can accept both
    /// representations without guessing.
    #[test]
    fn a_ustar_archive_is_not_mistaken_for_a_carrier() {
        assert!(!Tree::is_carrier(SAMPLE_ARCHIVE));
        let tree = tree_from_resident(SAMPLE_ARCHIVE).expect("archive describes a tree");
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
