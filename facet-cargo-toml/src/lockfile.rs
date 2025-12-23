//! Cargo.lock parsing and dependency reachability analysis.
//!
//! This module parses Cargo.lock files (v3 and v4 formats) and computes
//! the set of reachable registry packages from the root crate.
//!
//! ## Lockfile Format
//!
//! Cargo.lock contains:
//! - `version`: lockfile format version (3 or 4)
//! - `[[package]]`: array of package entries with name, version, source, checksum, dependencies
//!
//! ## Reachability
//!
//! Starting from the root package, we BFS through dependencies to find all
//! reachable packages. Only these packages need to be materialized and built.

use std::collections::{HashMap, HashSet, VecDeque};

use camino::{Utf8Path, Utf8PathBuf};
use facet::Facet;
use thiserror::Error;

/// The crates.io registry source string in Cargo.lock
pub const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// Errors that can occur during lockfile parsing
#[derive(Debug, Error)]
pub enum LockfileError {
    #[error("failed to read {path}: {source}")]
    ReadError {
        path: Utf8PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse Cargo.lock: {0}")]
    ParseError(String),

    #[error("unsupported lockfile version: {0} (supported: 3, 4)")]
    UnsupportedVersion(u32),

    #[error("missing lockfile version field")]
    MissingVersion,

    #[error("package '{name}' has unsupported source: {source_url}")]
    UnsupportedSource { name: String, source_url: String },

    #[error("registry package '{name} {version}' is missing checksum")]
    MissingChecksum { name: String, version: String },

    #[error("root package '{name}' not found in lockfile")]
    RootNotFound { name: String },

    #[error("dependency '{dep}' of package '{package}' not found in lockfile")]
    DependencyNotFound { package: String, dep: String },
}

/// A package entry from Cargo.lock
#[derive(Debug, Clone)]
pub struct LockPackage {
    /// Package name
    pub name: String,
    /// Version string
    pub version: String,
    /// Source (None for path deps, Some for registry/git)
    pub source: Option<String>,
    /// SHA256 checksum (required for registry packages)
    pub checksum: Option<String>,
    /// Dependencies as "name" or "name version" or "name version (source)"
    pub dependencies: Vec<String>,
}

impl LockPackage {
    /// Returns true if this is a registry package (from crates.io)
    pub fn is_registry(&self) -> bool {
        self.source.as_deref() == Some(CRATES_IO_SOURCE)
    }

    /// Returns true if this is a path dependency (no source)
    pub fn is_path(&self) -> bool {
        self.source.is_none()
    }

    /// Returns a unique key for this package: "name version"
    pub fn key(&self) -> String {
        format!("{} {}", self.name, self.version)
    }
}

/// Parsed Cargo.lock file
#[derive(Debug)]
pub struct Lockfile {
    /// Lockfile format version (3 or 4)
    pub version: u32,
    /// All packages in the lockfile
    pub packages: Vec<LockPackage>,
}

/// Raw TOML structure for parsing
#[derive(Facet, Debug)]
struct RawLockfile {
    version: Option<u32>,
    package: Option<Vec<RawPackage>>,
}

#[derive(Facet, Debug)]
struct RawPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    dependencies: Option<Vec<String>>,
}

impl Lockfile {
    /// Parse a Cargo.lock file from disk
    pub fn from_path(path: &Utf8Path) -> Result<Self, LockfileError> {
        let contents = std::fs::read_to_string(path).map_err(|e| LockfileError::ReadError {
            path: path.to_owned(),
            source: e,
        })?;
        Self::parse(&contents)
    }

    /// Parse Cargo.lock content
    pub fn parse(contents: &str) -> Result<Self, LockfileError> {
        let raw: RawLockfile =
            facet_toml::from_str(contents).map_err(|e| LockfileError::ParseError(e.to_string()))?;

        let version = raw.version.ok_or(LockfileError::MissingVersion)?;

        // We support v3 and v4
        if version != 3 && version != 4 {
            return Err(LockfileError::UnsupportedVersion(version));
        }

        let packages = raw
            .package
            .unwrap_or_default()
            .into_iter()
            .map(|p| LockPackage {
                name: p.name,
                version: p.version,
                source: p.source,
                checksum: p.checksum,
                dependencies: p.dependencies.unwrap_or_default(),
            })
            .collect();

        Ok(Lockfile { version, packages })
    }

    /// Find a package by name (for path deps with unique names)
    pub fn find_by_name(&self, name: &str) -> Option<&LockPackage> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Find a path package by name (source is None).
    /// Use this for root package lookup to avoid matching registry crates.
    pub fn find_path_by_name(&self, name: &str) -> Option<&LockPackage> {
        self.packages
            .iter()
            .find(|p| p.name == name && p.source.is_none())
    }

    /// Find a package by name and version
    pub fn find_by_name_version(&self, name: &str, version: &str) -> Option<&LockPackage> {
        self.packages
            .iter()
            .find(|p| p.name == name && p.version == version)
    }

    /// Build an index for efficient lookups
    pub fn build_index(&self) -> LockfileIndex<'_> {
        LockfileIndex::new(self)
    }
}

/// Index for efficient package lookups
pub struct LockfileIndex<'a> {
    /// Map from "name version" to package
    by_key: HashMap<String, &'a LockPackage>,
    /// Map from name to list of packages (for ambiguous lookups)
    by_name: HashMap<&'a str, Vec<&'a LockPackage>>,
}

impl<'a> LockfileIndex<'a> {
    fn new(lockfile: &'a Lockfile) -> Self {
        let mut by_key = HashMap::new();
        let mut by_name: HashMap<&str, Vec<&LockPackage>> = HashMap::new();

        for pkg in &lockfile.packages {
            by_key.insert(pkg.key(), pkg);
            by_name.entry(&pkg.name).or_default().push(pkg);
        }

        LockfileIndex { by_key, by_name }
    }

    /// Resolve a dependency string to a package.
    ///
    /// Dependency strings can be:
    /// - "name" (unambiguous, single version in lockfile)
    /// - "name version"
    /// - "name version (source)" (v4 format for disambiguation)
    pub fn resolve_dep(&self, dep: &str) -> Option<&'a LockPackage> {
        // Try exact match first (handles "name version" and "name version (source)")
        // Strip the source part if present
        let dep_key = if let Some(paren_idx) = dep.find(" (") {
            &dep[..paren_idx]
        } else {
            dep
        };

        if let Some(pkg) = self.by_key.get(dep_key) {
            return Some(pkg);
        }

        // Try name-only lookup (for unique deps)
        if !dep.contains(' ')
            && let Some(pkgs) = self.by_name.get(dep)
            && pkgs.len() == 1
        {
            return Some(pkgs[0]);
        }

        None
    }
}

/// Result of reachability analysis
#[derive(Debug)]
pub struct ReachablePackages {
    /// All reachable packages (for lookups)
    packages: Vec<LockPackage>,
    /// Index by "name version" for fast lookups
    by_key: HashMap<String, usize>,
    /// Index by name for prefix lookups
    by_name: HashMap<String, Vec<usize>>,
}

impl ReachablePackages {
    /// Create from a list of packages
    fn new(packages: Vec<LockPackage>) -> Self {
        let mut by_key = HashMap::new();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();

        for (idx, pkg) in packages.iter().enumerate() {
            by_key.insert(pkg.key(), idx);
            by_name.entry(pkg.name.clone()).or_default().push(idx);
        }

        ReachablePackages {
            packages,
            by_key,
            by_name,
        }
    }

    /// Iterate over registry packages only
    pub fn registry_packages(&self) -> impl Iterator<Item = &LockPackage> {
        self.packages.iter().filter(|p| p.is_registry())
    }

    /// Iterate over path packages only
    pub fn path_packages(&self) -> impl Iterator<Item = &LockPackage> {
        self.packages.iter().filter(|p| p.is_path())
    }

    /// Get a package by name and version
    pub fn get_package(&self, name: &str, version: &str) -> Option<&LockPackage> {
        let key = format!("{} {}", name, version);
        self.by_key.get(&key).map(|&idx| &self.packages[idx])
    }

    /// Find a package by name prefix (for resolving dependencies)
    pub fn find_by_name_prefix(&self, name: &str) -> Option<&LockPackage> {
        self.by_name
            .get(name)
            .and_then(|indices| indices.first())
            .map(|&idx| &self.packages[idx])
    }

    /// Find a path package by name (source.is_none()).
    /// Returns None if no path package with that name exists.
    /// Use this for resolving path crate entries in the lockfile.
    pub fn find_path_package(&self, name: &str) -> Option<&LockPackage> {
        self.by_name.get(name).and_then(|indices| {
            indices
                .iter()
                .map(|&idx| &self.packages[idx])
                .find(|pkg| pkg.source.is_none())
        })
    }

    /// Resolve a dependency string to a package.
    ///
    /// Handles formats: "name", "name version", "name version (source)"
    pub fn find_dependency(&self, dep_str: &str) -> Option<&LockPackage> {
        // Strip source part if present: "name version (source)" -> "name version"
        let dep_key = if let Some(paren_idx) = dep_str.find(" (") {
            &dep_str[..paren_idx]
        } else {
            dep_str
        };

        // Try exact "name version" match
        if let Some(&idx) = self.by_key.get(dep_key) {
            return Some(&self.packages[idx]);
        }

        // Try name-only lookup (for unique deps)
        if !dep_key.contains(' ')
            && let Some(indices) = self.by_name.get(dep_key)
            && indices.len() == 1
        {
            return Some(&self.packages[indices[0]]);
        }

        None
    }

    /// Check if a package is in the reachable set
    pub fn contains(&self, name: &str, version: &str) -> bool {
        let key = format!("{} {}", name, version);
        self.by_key.contains_key(&key)
    }
}

impl Lockfile {
    /// Compute reachable packages starting from the root package name.
    ///
    /// This performs BFS through the dependency graph and returns all
    /// reachable registry and path packages.
    pub fn compute_reachable(&self, root_name: &str) -> Result<ReachablePackages, LockfileError> {
        let index = self.build_index();

        // Find root package - must be a path package (source = None).
        // This prevents matching a registry crate with the same name.
        let root =
            self.find_path_by_name(root_name)
                .ok_or_else(|| LockfileError::RootNotFound {
                    name: root_name.to_string(),
                })?;

        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<&LockPackage> = VecDeque::new();
        let mut reachable_packages: Vec<LockPackage> = Vec::new();

        queue.push_back(root);
        visited.insert(root.key());

        while let Some(pkg) = queue.pop_front() {
            // Validate the package source
            if pkg.is_registry() {
                // Validate checksum exists for registry packages
                if pkg.checksum.is_none() {
                    return Err(LockfileError::MissingChecksum {
                        name: pkg.name.clone(),
                        version: pkg.version.clone(),
                    });
                }
            } else if !pkg.is_path() {
                // Unsupported source (git, other registry, etc.)
                return Err(LockfileError::UnsupportedSource {
                    name: pkg.name.clone(),
                    source_url: pkg.source.clone().unwrap_or_default(),
                });
            }

            // Clone the package into our result set
            reachable_packages.push(pkg.clone());

            // Enqueue dependencies
            for dep_str in &pkg.dependencies {
                let dep_pkg = index.resolve_dep(dep_str).ok_or_else(|| {
                    LockfileError::DependencyNotFound {
                        package: pkg.key(),
                        dep: dep_str.clone(),
                    }
                })?;

                let dep_key = dep_pkg.key();
                if !visited.contains(&dep_key) {
                    visited.insert(dep_key);
                    queue.push_back(dep_pkg);
                }
            }
        }

        Ok(ReachablePackages::new(reachable_packages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_v4_lockfile() {
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "serde",
]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c873e1b9b056a4dc4c0c198b24c3ffa059243875552b2bd0933b1aee4ce2"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        assert_eq!(lockfile.version, 4);
        assert_eq!(lockfile.packages.len(), 2);

        let serde = lockfile.find_by_name("serde").unwrap();
        assert!(serde.is_registry());
        assert_eq!(
            serde.checksum.as_deref(),
            Some("3fb1c873e1b9b056a4dc4c0c198b24c3ffa059243875552b2bd0933b1aee4ce2")
        );
    }

    #[test]
    fn parse_v3_lockfile() {
        let contents = r#"
version = 3

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "serde 1.0.197",
]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c873e1b9b056a4dc4c0c198b24c3ffa059243875552b2bd0933b1aee4ce2"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        assert_eq!(lockfile.version, 3);
    }

    #[test]
    fn reject_unsupported_version() {
        let contents = r#"
version = 2

[[package]]
name = "myapp"
version = "0.1.0"
"#;
        let err = Lockfile::parse(contents).unwrap_err();
        assert!(matches!(err, LockfileError::UnsupportedVersion(2)));
    }

    #[test]
    fn reject_missing_version() {
        let contents = r#"
[[package]]
name = "myapp"
version = "0.1.0"
"#;
        let err = Lockfile::parse(contents).unwrap_err();
        assert!(matches!(err, LockfileError::MissingVersion));
    }

    #[test]
    fn compute_reachable_simple() {
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "serde",
]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "3fb1c873e1b9b056a4dc4c0c198b24c3ffa059243875552b2bd0933b1aee4ce2"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        let reachable = lockfile.compute_reachable("myapp").unwrap();

        let path_pkgs: Vec<_> = reachable.path_packages().collect();
        assert_eq!(path_pkgs.len(), 1);
        assert_eq!(path_pkgs[0].name, "myapp");

        let registry_pkgs: Vec<_> = reachable.registry_packages().collect();
        assert_eq!(registry_pkgs.len(), 1);
        assert_eq!(registry_pkgs[0].name, "serde");
        assert_eq!(registry_pkgs[0].version, "1.0.197");
    }

    #[test]
    fn compute_reachable_transitive() {
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "serde",
]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaa"
dependencies = [
    "serde_derive",
]

[[package]]
name = "serde_derive"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bbbb"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        let reachable = lockfile.compute_reachable("myapp").unwrap();

        let registry_pkgs: Vec<_> = reachable.registry_packages().collect();
        assert_eq!(registry_pkgs.len(), 2);
        let names: Vec<_> = registry_pkgs.iter().map(|p| &p.name).collect();
        assert!(names.contains(&&"serde".to_string()));
        assert!(names.contains(&&"serde_derive".to_string()));
    }

    #[test]
    fn compute_reachable_skips_unreachable() {
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "used_crate",
]

[[package]]
name = "used_crate"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaa"

[[package]]
name = "unused_crate"
version = "2.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bbbb"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        let reachable = lockfile.compute_reachable("myapp").unwrap();

        let registry_pkgs: Vec<_> = reachable.registry_packages().collect();
        assert_eq!(registry_pkgs.len(), 1);
        assert_eq!(registry_pkgs[0].name, "used_crate");
    }

    #[test]
    fn reject_git_dependency() {
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "git_dep",
]

[[package]]
name = "git_dep"
version = "0.1.0"
source = "git+https://github.com/example/repo#abcd1234"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        let err = lockfile.compute_reachable("myapp").unwrap_err();
        assert!(matches!(err, LockfileError::UnsupportedSource { name, .. } if name == "git_dep"));
    }

    #[test]
    fn reject_missing_checksum() {
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "bad_crate",
]

[[package]]
name = "bad_crate"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        let err = lockfile.compute_reachable("myapp").unwrap_err();
        assert!(matches!(err, LockfileError::MissingChecksum { name, .. } if name == "bad_crate"));
    }

    #[test]
    fn resolve_versioned_dep_string() {
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "foo 1.0.0",
    "foo 2.0.0",
]

[[package]]
name = "foo"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaa"

[[package]]
name = "foo"
version = "2.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bbbb"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        let reachable = lockfile.compute_reachable("myapp").unwrap();

        let registry_pkgs: Vec<_> = reachable.registry_packages().collect();
        assert_eq!(registry_pkgs.len(), 2);
        let versions: Vec<_> = registry_pkgs.iter().map(|p| &p.version).collect();
        assert!(versions.contains(&&"1.0.0".to_string()));
        assert!(versions.contains(&&"2.0.0".to_string()));
    }

    #[test]
    fn resolve_dep_with_source_suffix() {
        // v4 format can have "name version (source)" for disambiguation
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "foo 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)",
]

[[package]]
name = "foo"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaa"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        let reachable = lockfile.compute_reachable("myapp").unwrap();

        let registry_pkgs: Vec<_> = reachable.registry_packages().collect();
        assert_eq!(registry_pkgs.len(), 1);
        assert_eq!(registry_pkgs[0].name, "foo");
    }

    #[test]
    fn path_deps_with_path_deps() {
        let contents = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "mylib",
]

[[package]]
name = "mylib"
version = "0.1.0"
dependencies = [
    "serde",
]

[[package]]
name = "serde"
version = "1.0.197"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaa"
"#;
        let lockfile = Lockfile::parse(contents).unwrap();
        let reachable = lockfile.compute_reachable("myapp").unwrap();

        // Both myapp and mylib are path deps
        let path_pkgs: Vec<_> = reachable.path_packages().collect();
        assert_eq!(path_pkgs.len(), 2);
        let path_names: Vec<_> = path_pkgs.iter().map(|p| &p.name).collect();
        assert!(path_names.contains(&&"myapp".to_string()));
        assert!(path_names.contains(&&"mylib".to_string()));

        // serde is a registry dep
        let registry_pkgs: Vec<_> = reachable.registry_packages().collect();
        assert_eq!(registry_pkgs.len(), 1);
        assert_eq!(registry_pkgs[0].name, "serde");
    }
}
