//! Cargo.lock types.

use camino::Utf8Path;
use facet::Facet;

/// The crates.io registry source string in Cargo.lock.
pub const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// A package entry from a `Cargo.lock` file.
#[derive(Debug, Clone)]
pub struct LockPackage {
    /// Package name.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Source URL (None for path dependencies).
    pub source: Option<String>,
    /// SHA256 checksum (for registry packages).
    pub checksum: Option<String>,
    /// Dependencies as "name" or "name version" or "name version (source)".
    pub dependencies: Vec<String>,
}

impl LockPackage {
    /// Returns true if this is a crates.io registry package.
    pub fn is_registry(&self) -> bool {
        self.source.as_deref() == Some(CRATES_IO_SOURCE)
    }

    /// Returns true if this is a path dependency.
    pub fn is_path(&self) -> bool {
        self.source.is_none()
    }
}

/// A parsed `Cargo.lock` file.
///
/// Supports lockfile format versions 3 and 4.
#[derive(Debug)]
pub struct CargoLock {
    /// Lockfile format version (3 or 4).
    pub version: u32,
    /// All packages in the lockfile.
    pub packages: Vec<LockPackage>,
}

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

impl CargoLock {
    /// Parse a `Cargo.lock` file from disk.
    pub fn from_path(path: impl AsRef<Utf8Path>) -> Result<Self, crate::Error> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| crate::Error::Io {
            path: path.to_owned(),
            source: crate::IoError::from(source),
        })?;
        Self::parse(&contents)
    }

    /// Parse `Cargo.lock` content from a string.
    pub fn parse(contents: &str) -> Result<Self, crate::Error> {
        let raw: RawLockfile = facet_toml::from_str(contents).map_err(|e| crate::Error::Parse {
            message: e.to_string(),
        })?;

        let version = raw.version.unwrap_or(3);

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

        Ok(CargoLock { version, packages })
    }

    /// Find a package by name.
    pub fn find_by_name(&self, name: &str) -> Option<&LockPackage> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Find a package by name and version.
    pub fn find_by_name_version(&self, name: &str, version: &str) -> Option<&LockPackage> {
        self.packages
            .iter()
            .find(|p| p.name == name && p.version == version)
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
    "aho-corasick",
]

[[package]]
name = "aho-corasick"
version = "1.1.2"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "b2969dcb958b36655471fc61f7e416fa76033bdd4bfed0678d8fee1e2d07a1f0"
"#;
        let lockfile = CargoLock::parse(contents).unwrap();
        assert_eq!(lockfile.version, 4);
        assert_eq!(
            lockfile.packages.len(),
            2,
            "Expected 2 packages but got {}: {:?}",
            lockfile.packages.len(),
            lockfile
                .packages
                .iter()
                .map(|p| &p.name)
                .collect::<Vec<_>>()
        );

        let ac = lockfile.find_by_name("aho-corasick").unwrap();
        assert!(ac.is_registry());
        assert!(ac.checksum.is_some());
    }

    #[test]
    fn parse_v3_lockfile() {
        let contents = r#"
version = 3

[[package]]
name = "myapp"
version = "0.1.0"
dependencies = [
    "aho-corasick 1.1.2",
]

[[package]]
name = "aho-corasick"
version = "1.1.2"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "b2969dcb958b36655471fc61f7e416fa76033bdd4bfed0678d8fee1e2d07a1f0"
"#;
        let lockfile = CargoLock::parse(contents).unwrap();
        assert_eq!(lockfile.version, 3);
    }
}
