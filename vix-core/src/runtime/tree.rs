//! The semantic `Tree` model: a recursive map from one path *segment* to a
//! [`TreeEntry`], mirroring `vx-services`'s `DirectoryNode`/`DirectoryEntryKind`
//! minus every storage field.
//!
//! ```text
//! Tree      = Map<Name, TreeEntry>          // Name is ONE path segment
//! TreeEntry = File    { content: Blob, executable: Bool }
//!           | Dir     (Tree)                 // recursive; empty Dir representable
//!           | Symlink { target: String }
//! ```
//!
//! This is *not* `Map<Path, Blob>`. That spelling could represent neither a
//! directory (in particular an empty one), nor a symlink, nor an executable bit
//! — all three of which the store models and all three of which participate in
//! identity.
//!
//! Identity lives next door in [`super::identity`], the one module allowed to
//! encode and hash (`machine.identity.single-module`); this module owns the
//! *model*: name validation, projection, construction from archives and from
//! disk, and materialization back to disk.
//!
//! r[impl machine.identity.tree-model]

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

/// One path segment of a [`Tree`]: nonempty, valid UTF-8, and not `.`, `..`, a
/// separator, or NUL. Spelling is preserved exactly — no Unicode
/// normalization — and comparison is case-sensitive on every platform, so a
/// Tree built on macOS and a Tree built on Linux are the same value.
///
/// The derived `Ord` is `String`'s, which is Unicode scalar order; UTF-8 byte
/// order is the equivalent storage comparator, so a store may sort raw bytes
/// and get the same rows.
///
/// r[impl machine.identity.tree-canonicalization]
/// r[impl lang.tree.name]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

/// Why a string is not a [`Name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    /// The empty string.
    Empty,
    /// `.` or `..` — a navigation token, not a name.
    Dot,
    /// Contains `/`, `\\`, or NUL.
    Separator,
}

impl Name {
    /// Validate one segment. The excluded set is exactly the spec's: empty,
    /// `.`, `..`, path separators, and NUL. `\\` is excluded on every platform
    /// (not only Windows) so a Tree cannot mean different things depending on
    /// where it is materialized.
    pub fn new(segment: impl Into<String>) -> Result<Self, NameError> {
        let segment = segment.into();
        if segment.is_empty() {
            return Err(NameError::Empty);
        }
        if segment == "." || segment == ".." {
            return Err(NameError::Dot);
        }
        if segment.contains(['/', '\\', '\0']) {
            return Err(NameError::Separator);
        }
        Ok(Self(segment))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Immutable file bytes. The language value holds the *Blob*, not a
/// `ContentHash` and not a size: a `ContentHash` is the Blob's identity and
/// `size` is derived, and naming either in the entry would leak a storage field
/// back into the semantics.
///
/// r[impl machine.identity.tree-hash-is-not-node-hash]
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Blob(Vec<u8>);

impl Blob {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Derived, never stored beside the bytes and never hashed as its own
    /// field: a size restated beside a content hash is a storage field wearing
    /// a semantic coat.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for Blob {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// One entry of a [`Tree`].
///
/// `executable` is portable semantic *intent*, not a POSIX mode fragment: it
/// participates in identity on every platform, Unix materialization maps it to
/// a canonical mode, and Windows preserves it as metadata. mtime, uid/gid,
/// setuid/setgid/sticky, other mode bits, xattrs, resource forks, hardlink
/// identity, device/FIFO/socket nodes and ACLs are outside the model and have
/// no representation here to lose.
///
/// r[impl machine.identity.tree-canonicalization]
/// r[impl lang.tree.executable]
/// r[impl lang.tree.excluded-metadata]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeEntry {
    File { content: Blob, executable: bool },
    /// A subdirectory. An *empty* `Dir` is representable and round-trips —
    /// required to express an output that creates a directory for a later
    /// process without writing a file into it.
    Dir(Tree),
    /// A relative UTF-8 target, preserved verbatim. Dangling targets and `..`
    /// are representable; resolution is the materializer's and the mount
    /// grant's problem, not the value's.
    ///
    /// r[impl lang.tree.symlink]
    Symlink { target: String },
}

impl TreeEntry {
    /// A non-executable regular file.
    #[must_use]
    pub fn file(bytes: impl Into<Vec<u8>>) -> Self {
        Self::File {
            content: Blob::new(bytes.into()),
            executable: false,
        }
    }

    /// An executable regular file.
    #[must_use]
    pub fn executable(bytes: impl Into<Vec<u8>>) -> Self {
        Self::File {
            content: Blob::new(bytes.into()),
            executable: true,
        }
    }

    #[must_use]
    pub fn symlink(target: impl Into<String>) -> Self {
        Self::Symlink {
            target: target.into(),
        }
    }

    /// The entry-kind discriminant that enters identity. The ordinals are
    /// load-bearing epoch constants: renumbering them silently invalidates
    /// every existing `TreeHash`.
    #[must_use]
    pub fn kind_tag(&self) -> u64 {
        match self {
            Self::File { .. } => 0,
            Self::Dir(_) => 1,
            Self::Symlink { .. } => 2,
        }
    }

    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::File { .. } => "file",
            Self::Dir(_) => "directory",
            Self::Symlink { .. } => "symlink",
        }
    }
}

/// A directory: segment-keyed, recursive, ordered by [`Name`].
///
/// r[impl machine.identity.tree-model]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    entries: BTreeMap<Name, TreeEntry>,
}

/// Why a Tree operation could not be performed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeError {
    /// A path segment was not a valid [`Name`].
    Name { segment: String, error: NameError },
    /// An interior path component exists and is not a directory.
    NotADirectory { path: String },
    /// Two archive members claim the same name with different kinds, or the
    /// same name twice.
    Collision { path: String },
    /// A symlink target was absolute, or not relative UTF-8.
    AbsoluteSymlink { path: String, target: String },
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name { segment, error } => {
                let reason = match error {
                    NameError::Empty => "is empty",
                    NameError::Dot => "is `.` or `..`",
                    NameError::Separator => "contains a separator or NUL",
                };
                write!(f, "tree name `{segment}` {reason}")
            }
            Self::NotADirectory { path } => write!(f, "tree path `{path}` is not a directory"),
            Self::Collision { path } => write!(f, "tree path `{path}` is claimed twice"),
            Self::AbsoluteSymlink { path, target } => {
                write!(f, "symlink `{path}` -> `{target}` is not relative")
            }
        }
    }
}

impl Tree {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The entries in canonical (Name) order. This is the order identity is
    /// computed in and the order a listing witnesses.
    pub fn iter(&self) -> impl Iterator<Item = (&Name, &TreeEntry)> {
        self.entries.iter()
    }

    #[must_use]
    pub fn get(&self, name: &Name) -> Option<&TreeEntry> {
        self.entries.get(name)
    }

    /// One-segment lookup by string, for callers holding a raw segment.
    #[must_use]
    pub fn get_segment(&self, segment: &str) -> Option<&TreeEntry> {
        let name = Name::new(segment).ok()?;
        self.entries.get(&name)
    }

    /// Insert one entry directly into this directory, replacing any entry that
    /// shared the name.
    pub fn insert(&mut self, name: Name, entry: TreeEntry) -> Option<TreeEntry> {
        self.entries.insert(name, entry)
    }

    /// `tree / p"a/b"`: a projection through nested maps, one segment at a
    /// time. There is no whole-path key to look up — `a/b` means "the entry
    /// named `b` of the directory named `a`", and each step must find a
    /// directory or the projection is absent.
    ///
    /// r[impl machine.identity.tree-model]
    /// r[impl lang.tree.projection]
    #[must_use]
    pub fn project(&self, path: &str) -> Option<&TreeEntry> {
        let mut segments = split_path(path).peekable();
        segments.peek()?;
        let mut directory = self;
        let mut found = None;
        for segment in segments {
            let name = Name::new(segment).ok()?;
            let entry = directory.entries.get(&name)?;
            match entry {
                TreeEntry::Dir(child) => directory = child,
                _ => directory = EMPTY_TREE,
            }
            found = Some(entry);
        }
        found
    }

    /// The subdirectory at `path`, if the projection lands on a `Dir`. An empty
    /// path projects the root.
    #[must_use]
    pub fn project_dir(&self, path: &str) -> Option<&Tree> {
        if split_path(path).next().is_none() {
            return Some(self);
        }
        match self.project(path)? {
            TreeEntry::Dir(child) => Some(child),
            _ => None,
        }
    }

    /// The file bytes at `path`, if the projection lands on a `File`. Symlinks
    /// are NOT followed: a projection returns the value that is there.
    #[must_use]
    pub fn file_bytes(&self, path: &str) -> Option<&[u8]> {
        match self.project(path)? {
            TreeEntry::File { content, .. } => Some(content.as_bytes()),
            _ => None,
        }
    }

    /// Insert `entry` at a multi-segment `path`, creating intermediate
    /// directories. Refuses to descend through a non-directory and refuses to
    /// overwrite an existing entry — an archive that names the same path twice
    /// is a collision, not a last-writer-wins merge.
    ///
    /// Intermediate directories are created, never *followed*: if `a` is a
    /// symlink, `a/b` is a `NotADirectory` error rather than a write through
    /// the link. That is what makes archive extraction symlink-safe by
    /// construction.
    ///
    /// r[impl lang.tree.symlink]
    pub fn insert_path(&mut self, path: &str, entry: TreeEntry) -> Result<(), TreeError> {
        let segments = split_path(path).collect::<Vec<_>>();
        let Some((last, parents)) = segments.split_last() else {
            return Err(TreeError::Name {
                segment: path.to_owned(),
                error: NameError::Empty,
            });
        };
        let mut directory = self;
        let mut walked = String::new();
        for segment in parents {
            let name = name_of(segment)?;
            push_segment(&mut walked, segment);
            let slot = directory
                .entries
                .entry(name)
                .or_insert_with(|| TreeEntry::Dir(Tree::new()));
            match slot {
                TreeEntry::Dir(child) => directory = child,
                _ => {
                    return Err(TreeError::NotADirectory {
                        path: walked.clone(),
                    });
                }
            }
        }
        let name = name_of(last)?;
        push_segment(&mut walked, last);
        match directory.entries.entry(name) {
            Entry::Vacant(slot) => {
                slot.insert(entry);
                Ok(())
            }
            // A directory the archive already implied, now declared
            // explicitly, is not a collision: it keeps its children.
            Entry::Occupied(slot)
                if matches!(slot.get(), TreeEntry::Dir(_)) && matches!(entry, TreeEntry::Dir(_)) =>
            {
                Ok(())
            }
            Entry::Occupied(_) => Err(TreeError::Collision { path: walked }),
        }
    }

    /// Ensure a directory exists at `path`, creating intermediates. An empty
    /// path is the root and is always present.
    pub fn insert_dir(&mut self, path: &str) -> Result<(), TreeError> {
        if split_path(path).next().is_none() {
            return Ok(());
        }
        self.insert_path(path, TreeEntry::Dir(Tree::new()))
    }

    /// Every `(path, entry)` in canonical depth-first order, paths joined with
    /// `/`. Directories are yielded before their children. This is a *view* for
    /// listing and globbing — it is not the identity encoding and not a key
    /// space.
    #[must_use]
    pub fn walk(&self) -> Vec<(String, &TreeEntry)> {
        let mut out = Vec::new();
        self.walk_into(&mut String::new(), &mut out);
        out
    }

    fn walk_into<'tree>(&'tree self, prefix: &mut String, out: &mut Vec<(String, &'tree TreeEntry)>) {
        for (name, entry) in &self.entries {
            let restore = prefix.len();
            push_segment(prefix, name.as_str());
            out.push((prefix.clone(), entry));
            if let TreeEntry::Dir(child) = entry {
                child.walk_into(prefix, out);
            }
            prefix.truncate(restore);
        }
    }

    /// Every file path, in canonical depth-first order. Directories and
    /// symlinks are not files and are not yielded.
    #[must_use]
    pub fn file_paths(&self) -> Vec<String> {
        self.walk()
            .into_iter()
            .filter(|(_, entry)| matches!(entry, TreeEntry::File { .. }))
            .map(|(path, _)| path)
            .collect()
    }
}

impl<'tree> IntoIterator for &'tree Tree {
    type Item = (&'tree Name, &'tree TreeEntry);
    type IntoIter = std::collections::btree_map::Iter<'tree, Name, TreeEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

/// A shared empty tree, so [`Tree::project`] can keep walking after a
/// non-directory without allocating and without an early `return`.
static EMPTY_TREE: &Tree = &Tree {
    entries: BTreeMap::new(),
};

/// Split a `/`-joined path into its nonempty segments. `.` and `//` runs are
/// dropped here so that callers may pass archive paths verbatim; `..` is *not*
/// dropped — it is not a valid [`Name`], so it fails validation rather than
/// silently escaping.
pub fn split_path(path: &str) -> impl Iterator<Item = &str> {
    path.split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
}

fn name_of(segment: &str) -> Result<Name, TreeError> {
    Name::new(segment).map_err(|error| TreeError::Name {
        segment: segment.to_owned(),
        error,
    })
}

fn push_segment(path: &mut String, segment: &str) {
    if !path.is_empty() {
        path.push('/');
    }
    path.push_str(segment);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// r[verify machine.identity.tree-canonicalization]
    #[test]
    fn name_excludes_navigation_separators_and_nul() {
        assert_eq!(Name::new(""), Err(NameError::Empty));
        assert_eq!(Name::new("."), Err(NameError::Dot));
        assert_eq!(Name::new(".."), Err(NameError::Dot));
        assert_eq!(Name::new("a/b"), Err(NameError::Separator));
        assert_eq!(Name::new("a\\b"), Err(NameError::Separator));
        assert_eq!(Name::new("a\0b"), Err(NameError::Separator));
        assert_eq!(Name::new("...").expect("`...` is a name").as_str(), "...");
        assert_eq!(Name::new("a b").expect("spaces are names").as_str(), "a b");
    }

    /// Spelling is preserved exactly: NFC and NFD spellings of the same
    /// grapheme are different names, and case is significant on every platform.
    ///
    /// r[verify machine.identity.tree-canonicalization]
    #[test]
    fn names_are_not_normalized_and_are_case_sensitive() {
        let composed = Name::new("é").expect("NFC name");
        let decomposed = Name::new("e\u{301}").expect("NFD name");
        assert_ne!(composed, decomposed);
        assert_ne!(
            Name::new("README").expect("upper"),
            Name::new("readme").expect("lower")
        );
    }

    /// r[verify machine.identity.tree-model]
    #[test]
    fn projection_walks_one_segment_at_a_time() {
        let mut tree = Tree::new();
        tree.insert_path("a/b/c.txt", TreeEntry::file(*b"hello"))
            .expect("insert");
        assert_eq!(tree.file_bytes("a/b/c.txt"), Some(&b"hello"[..]));
        assert!(matches!(tree.project("a"), Some(TreeEntry::Dir(_))));
        assert!(matches!(tree.project("a/b"), Some(TreeEntry::Dir(_))));
        assert_eq!(tree.project("a/b/missing"), None);
        // There is no whole-path key: `a/b/c.txt` is three lookups, not one.
        assert_eq!(tree.get_segment("a/b/c.txt"), None);
        // A projection through a file is not a projection.
        assert_eq!(tree.project("a/b/c.txt/d"), None);
    }

    /// r[verify machine.identity.tree-canonicalization]
    #[test]
    fn empty_directories_round_trip() {
        let mut tree = Tree::new();
        tree.insert_dir("out/empty").expect("insert dir");
        let Some(TreeEntry::Dir(empty)) = tree.project("out/empty") else {
            panic!("empty directory is representable");
        };
        assert!(empty.is_empty());
        assert_eq!(
            tree.walk().iter().map(|(path, _)| path.as_str()).collect::<Vec<_>>(),
            ["out", "out/empty"]
        );
    }

    /// r[verify lang.tree.symlink]
    #[test]
    fn insert_path_never_descends_through_a_symlink() {
        let mut tree = Tree::new();
        tree.insert_path("link", TreeEntry::symlink("../outside"))
            .expect("insert symlink");
        assert_eq!(
            tree.insert_path("link/escaped.txt", TreeEntry::file(*b"nope")),
            Err(TreeError::NotADirectory {
                path: "link".to_owned()
            })
        );
    }

    /// r[verify machine.identity.tree-canonicalization]
    #[test]
    fn parent_segments_are_rejected_rather_than_resolved() {
        let mut tree = Tree::new();
        assert_eq!(
            tree.insert_path("../escape.txt", TreeEntry::file(*b"nope")),
            Err(TreeError::Name {
                segment: "..".to_owned(),
                error: NameError::Dot
            })
        );
    }

    #[test]
    fn declaring_an_implied_directory_keeps_its_children() {
        let mut tree = Tree::new();
        tree.insert_path("src/lib.rs", TreeEntry::file(*b"pub fn f() {}"))
            .expect("insert file");
        tree.insert_dir("src").expect("redeclare dir");
        assert_eq!(tree.file_paths(), ["src/lib.rs"]);
    }

    #[test]
    fn colliding_files_are_an_error_not_a_last_writer_wins() {
        let mut tree = Tree::new();
        tree.insert_path("a.txt", TreeEntry::file(*b"one"))
            .expect("insert");
        assert_eq!(
            tree.insert_path("a.txt", TreeEntry::file(*b"two")),
            Err(TreeError::Collision {
                path: "a.txt".to_owned()
            })
        );
    }
}
