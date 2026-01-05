//! Crate name detection for proc-macros.
//!
//! This module provides functionality similar to `proc-macro-crate`, but uses
//! `facet-cargo-toml` for TOML parsing instead of `toml_edit`.

use std::{
    collections::BTreeMap,
    env, fmt,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::SystemTime,
};

use facet_cargo_toml::{CargoToml, Dependency};

/// Error type for crate name detection.
pub enum Error {
    NotFound(PathBuf),
    CargoManifestDirNotSet,
    FailedGettingWorkspaceManifestPath,
    CouldNotRead { path: PathBuf, message: String },
    CrateNotFound { crate_name: String, path: PathBuf },
}

impl std::error::Error for Error {}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::NotFound(path) => {
                write!(
                    f,
                    "Could not find `Cargo.toml` in manifest dir: `{}`.",
                    path.display()
                )
            }
            Error::CargoManifestDirNotSet => {
                f.write_str("`CARGO_MANIFEST_DIR` env variable not set.")
            }
            Error::CouldNotRead { path, message } => {
                write!(f, "Could not read `{}`: {}", path.display(), message)
            }
            Error::CrateNotFound { crate_name, path } => write!(
                f,
                "Could not find `{}` in `dependencies` or `dev-dependencies` in `{}`!",
                crate_name,
                path.display(),
            ),
            Error::FailedGettingWorkspaceManifestPath => {
                f.write_str("Failed to get the path of the workspace manifest path.")
            }
        }
    }
}

/// The crate as found by [`crate_name`].
#[derive(Debug, PartialEq, Clone, Eq)]
pub enum FoundCrate {
    /// The searched crate is this crate itself.
    Itself,
    /// The searched crate was found with this name.
    Name(String),
}

// --- Caching infrastructure ---

type Cache = BTreeMap<String, CacheEntry>;

struct CacheEntry {
    manifest_ts: SystemTime,
    workspace_manifest_ts: SystemTime,
    workspace_manifest_path: PathBuf,
    crate_names: CrateNames,
}

type CrateNames = BTreeMap<String, FoundCrate>;

/// Find the crate name for the given `orig_name` in the current `Cargo.toml`.
///
/// `orig_name` should be the original name of the searched crate (e.g., `"rapace"`).
///
/// The current `Cargo.toml` is determined by taking `CARGO_MANIFEST_DIR/Cargo.toml`.
///
/// # Returns
///
/// - `Ok(FoundCrate::Itself)` - the searched crate is the current crate being compiled.
/// - `Ok(FoundCrate::Name(new_name))` - the searched crate was found with the given name.
/// - `Err` if an error occurred.
pub fn crate_name(orig_name: &str) -> Result<FoundCrate, Error> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|_| Error::CargoManifestDirNotSet)?;
    let manifest_path = Path::new(&manifest_dir).join("Cargo.toml");

    let manifest_ts = cargo_toml_timestamp(&manifest_path)?;

    static CACHE: Mutex<Cache> = Mutex::new(BTreeMap::new());
    let mut cache = CACHE.lock().unwrap();

    let crate_names = match cache.entry(manifest_dir) {
        std::collections::btree_map::Entry::Occupied(entry) => {
            let cache_entry = entry.into_mut();
            let workspace_manifest_path = cache_entry.workspace_manifest_path.as_path();
            let workspace_manifest_ts = cargo_toml_timestamp(workspace_manifest_path)?;

            // Timestamp changed, rebuild this cache entry.
            if manifest_ts != cache_entry.manifest_ts
                || workspace_manifest_ts != cache_entry.workspace_manifest_ts
            {
                *cache_entry = read_cargo_toml(
                    &manifest_path,
                    workspace_manifest_path,
                    manifest_ts,
                    workspace_manifest_ts,
                )?;
            }

            &cache_entry.crate_names
        }
        std::collections::btree_map::Entry::Vacant(entry) => {
            let workspace_manifest_path =
                workspace_manifest_path(&manifest_path)?.unwrap_or_else(|| manifest_path.clone());
            let workspace_manifest_ts = cargo_toml_timestamp(&workspace_manifest_path)?;

            let cache_entry = entry.insert(read_cargo_toml(
                &manifest_path,
                &workspace_manifest_path,
                manifest_ts,
                workspace_manifest_ts,
            )?);
            &cache_entry.crate_names
        }
    };

    Ok(crate_names
        .get(orig_name)
        .ok_or_else(|| Error::CrateNotFound {
            crate_name: orig_name.to_owned(),
            path: manifest_path,
        })?
        .clone())
}

fn workspace_manifest_path(cargo_toml_manifest: &Path) -> Result<Option<PathBuf>, Error> {
    let Ok(cargo) = env::var("CARGO") else {
        return Ok(None);
    };

    let output = Command::new(cargo)
        .arg("locate-project")
        .args(["--workspace", "--message-format=plain"])
        .arg(format!("--manifest-path={}", cargo_toml_manifest.display()))
        .output()
        .map_err(|_| Error::FailedGettingWorkspaceManifestPath)?;

    String::from_utf8(output.stdout)
        .map_err(|_| Error::FailedGettingWorkspaceManifestPath)
        .map(|s| {
            let path = s.trim();
            if path.is_empty() {
                None
            } else {
                Some(path.into())
            }
        })
}

fn cargo_toml_timestamp(manifest_path: &Path) -> Result<SystemTime, Error> {
    std::fs::metadata(manifest_path)
        .and_then(|meta| meta.modified())
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound(manifest_path.to_owned())
            } else {
                Error::CouldNotRead {
                    path: manifest_path.to_owned(),
                    message: source.to_string(),
                }
            }
        })
}

fn read_cargo_toml(
    manifest_path: &Path,
    workspace_manifest_path: &Path,
    manifest_ts: SystemTime,
    workspace_manifest_ts: SystemTime,
) -> Result<CacheEntry, Error> {
    let manifest = open_cargo_toml(manifest_path)?;

    let workspace_dependencies = if manifest_path != workspace_manifest_path {
        let workspace_manifest = open_cargo_toml(workspace_manifest_path)?;
        extract_workspace_dependencies(&workspace_manifest)
    } else {
        extract_workspace_dependencies(&manifest)
    };

    let crate_names = extract_crate_names(&manifest, workspace_dependencies);

    Ok(CacheEntry {
        manifest_ts,
        workspace_manifest_ts,
        crate_names,
        workspace_manifest_path: workspace_manifest_path.to_path_buf(),
    })
}

fn open_cargo_toml(path: &Path) -> Result<CargoToml, Error> {
    // Convert std::path::Path to camino::Utf8Path
    let utf8_path = path.to_str().ok_or_else(|| Error::CouldNotRead {
        path: path.into(),
        message: "path is not valid UTF-8".to_owned(),
    })?;

    CargoToml::from_path(utf8_path).map_err(|e| Error::CouldNotRead {
        path: path.into(),
        message: e.to_string(),
    })
}

/// Extract workspace dependencies mapping dep_name -> package_name
fn extract_workspace_dependencies(workspace_toml: &CargoToml) -> BTreeMap<String, String> {
    let Some(workspace) = &workspace_toml.workspace else {
        return BTreeMap::new();
    };
    let Some(deps) = &workspace.dependencies else {
        return BTreeMap::new();
    };

    deps.iter()
        .map(|(dep_name, dep)| {
            let pkg_name = get_package_name(dep).unwrap_or(dep_name.as_str());
            (dep_name.clone(), pkg_name.to_owned())
        })
        .collect()
}

/// Get the actual package name from a dependency (handling `package = "..."` renames)
fn get_package_name(dep: &Dependency) -> Option<&str> {
    match dep {
        Dependency::Version(_) => None,
        Dependency::Workspace(_) => None,
        Dependency::Detailed(detail) => detail.package.as_ref().map(|s| s.value.as_str()),
    }
}

/// Check if this is a workspace dependency
fn is_workspace_dep(dep: &Dependency) -> bool {
    matches!(dep, Dependency::Workspace(_))
}

/// Make sure that the given crate name is a valid rust identifier.
fn sanitize_crate_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Extract all crate names from dependencies
fn extract_crate_names(
    cargo_toml: &CargoToml,
    workspace_dependencies: BTreeMap<String, String>,
) -> CrateNames {
    let package_name = cargo_toml
        .package
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str());

    // Check if we're building the crate itself
    let root_pkg = package_name.map(|name| {
        let cr = match env::var_os("CARGO_TARGET_TMPDIR") {
            // We're running for a library/binary crate
            None => FoundCrate::Itself,
            // We're running for an integration test
            Some(_) => FoundCrate::Name(sanitize_crate_name(name)),
        };
        (name.to_owned(), cr)
    });

    // Collect all dependency tables
    let mut all_deps: Vec<(&String, &Dependency)> = Vec::new();

    if let Some(deps) = &cargo_toml.dependencies {
        all_deps.extend(deps.iter());
    }
    if let Some(deps) = &cargo_toml.dev_dependencies {
        all_deps.extend(deps.iter());
    }
    if let Some(deps) = &cargo_toml.build_dependencies {
        all_deps.extend(deps.iter());
    }
    // Target-specific dependencies
    if let Some(targets) = &cargo_toml.target {
        for target_deps in targets.values() {
            if let Some(deps) = &target_deps.dependencies {
                all_deps.extend(deps.iter());
            }
            if let Some(deps) = &target_deps.dev_dependencies {
                all_deps.extend(deps.iter());
            }
            if let Some(deps) = &target_deps.build_dependencies {
                all_deps.extend(deps.iter());
            }
        }
    }

    let dep_pkgs = all_deps.into_iter().filter_map(|(dep_name, dep)| {
        let pkg_name = get_package_name(dep).unwrap_or(dep_name.as_str());

        // Skip if this is the root package (handled above)
        if package_name.is_some_and(|n| n == pkg_name) {
            return None;
        }

        // Resolve workspace dependencies
        let pkg_name = if is_workspace_dep(dep) {
            workspace_dependencies
                .get(pkg_name)
                .map(|p| p.as_str())
                .unwrap_or(pkg_name)
        } else {
            pkg_name
        };

        let cr = FoundCrate::Name(sanitize_crate_name(dep_name));
        Some((pkg_name.to_owned(), cr))
    });

    root_pkg.into_iter().chain(dep_pkgs).collect()
}
