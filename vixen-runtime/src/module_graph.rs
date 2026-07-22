//! Filesystem-backed Vix module discovery.
//!
//! Every direct child named `*.vix` becomes one module whose name is its file
//! stem. Sources are owned by [`ModuleGraph`], which can then lend
//! [`ModuleSource`] values to the existing compiler
//! and ratchet entry points.

use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use vix::modules::ModuleSource;

/// Conventional root module selected by [`ModuleGraph::default_root`].
pub const DEFAULT_ROOT_MODULE: &str = "main";

/// One owned `.vix` file discovered in a module directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleFile {
    name: String,
    path: PathBuf,
    source: String,
}

impl ModuleFile {
    /// The module name derived from the file stem.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The source file's full path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The UTF-8 Vix source text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    fn as_source(&self) -> ModuleSource<'_> {
        ModuleSource {
            name: &self.name,
            source: &self.source,
        }
    }
}

/// An owned, deterministically ordered set of modules loaded from a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleGraph {
    directory: PathBuf,
    modules: BTreeMap<String, ModuleFile>,
}

impl ModuleGraph {
    /// Load every direct `*.vix` child of `directory` as a module.
    ///
    /// Non-Vix files and subdirectories are ignored. Module names must be valid
    /// Vix identifiers and are derived directly from file stems (`math.vix`
    /// becomes `math`).
    pub fn from_dir(directory: impl AsRef<Path>) -> Result<Self, ModuleGraphError> {
        let directory = directory.as_ref().to_path_buf();
        let entries =
            fs::read_dir(&directory).map_err(|source| ModuleGraphError::ReadDirectory {
                path: directory.clone(),
                source,
            })?;
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| ModuleGraphError::ReadDirectoryEntry {
                path: directory.clone(),
                source,
            })?;
            let file_type = entry
                .file_type()
                .map_err(|source| ModuleGraphError::ReadFileType {
                    path: entry.path(),
                    source,
                })?;
            let path = entry.path();
            if file_type.is_file() && path.extension() == Some(OsStr::new("vix")) {
                paths.push(path);
            }
        }
        paths.sort();

        let mut modules: BTreeMap<String, ModuleFile> = BTreeMap::new();
        for path in paths {
            let Some(name) = path.file_stem().and_then(OsStr::to_str) else {
                return Err(ModuleGraphError::InvalidModuleName { path });
            };
            if !is_vix_identifier(name) {
                return Err(ModuleGraphError::InvalidModuleName { path });
            }
            let source =
                fs::read_to_string(&path).map_err(|source| ModuleGraphError::ReadModule {
                    path: path.clone(),
                    source,
                })?;
            let name = name.to_owned();
            if let Some(first) = modules.get(&name) {
                return Err(ModuleGraphError::DuplicateModule {
                    name,
                    first: first.path.clone(),
                    second: path,
                });
            }
            modules.insert(name.clone(), ModuleFile { name, path, source });
        }

        Ok(Self { directory, modules })
    }

    /// Directory from which this graph was loaded.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Iterate modules in stable name order.
    pub fn iter(&self) -> impl Iterator<Item = &ModuleFile> {
        self.modules.values()
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ModuleFile> {
        self.modules.get(name)
    }

    /// Borrow every file as an input for `Compiler::compile_with_modules` or
    /// `ratchet::run_source_with_modules`.
    #[must_use]
    pub fn module_sources(&self) -> Vec<ModuleSource<'_>> {
        self.modules.values().map(ModuleFile::as_source).collect()
    }

    /// Select one file as the root program and lend every other file as a
    /// library module.
    #[must_use]
    pub fn root(&self, name: &str) -> Option<ModuleRoot<'_>> {
        let root = self.modules.get(name)?;
        let modules = self
            .modules
            .values()
            .filter(|module| module.name != name)
            .map(ModuleFile::as_source)
            .collect();
        Some(ModuleRoot { root, modules })
    }

    /// Select `main.vix` as the root and lend every other file as a module.
    #[must_use]
    pub fn default_root(&self) -> Option<ModuleRoot<'_>> {
        self.root(DEFAULT_ROOT_MODULE)
    }
}

/// A borrowed root source plus all other files in its directory as modules.
#[derive(Clone, Debug)]
pub struct ModuleRoot<'a> {
    root: &'a ModuleFile,
    modules: Vec<ModuleSource<'a>>,
}

impl<'a> ModuleRoot<'a> {
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.root.name()
    }

    #[must_use]
    pub fn path(&self) -> &'a Path {
        self.root.path()
    }

    #[must_use]
    pub fn source(&self) -> &'a str {
        self.root.source()
    }

    #[must_use]
    pub fn modules(&self) -> &[ModuleSource<'a>] {
        &self.modules
    }
}

/// Load a directory into an owned module graph.
pub fn load_module_graph(directory: impl AsRef<Path>) -> Result<ModuleGraph, ModuleGraphError> {
    ModuleGraph::from_dir(directory)
}

#[derive(Debug)]
pub enum ModuleGraphError {
    ReadDirectory {
        path: PathBuf,
        source: io::Error,
    },
    ReadDirectoryEntry {
        path: PathBuf,
        source: io::Error,
    },
    ReadFileType {
        path: PathBuf,
        source: io::Error,
    },
    ReadModule {
        path: PathBuf,
        source: io::Error,
    },
    InvalidModuleName {
        path: PathBuf,
    },
    DuplicateModule {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },
}

impl fmt::Display for ModuleGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadDirectory { path, source } => {
                write!(
                    formatter,
                    "cannot read module directory {}: {source}",
                    path.display()
                )
            }
            Self::ReadDirectoryEntry { path, source } => write!(
                formatter,
                "cannot read an entry in module directory {}: {source}",
                path.display()
            ),
            Self::ReadFileType { path, source } => {
                write!(
                    formatter,
                    "cannot inspect module file {}: {source}",
                    path.display()
                )
            }
            Self::ReadModule { path, source } => {
                write!(
                    formatter,
                    "cannot read module source {}: {source}",
                    path.display()
                )
            }
            Self::InvalidModuleName { path } => write!(
                formatter,
                "module file {} does not have a valid Vix identifier as its stem",
                path.display()
            ),
            Self::DuplicateModule {
                name,
                first,
                second,
            } => write!(
                formatter,
                "module {name:?} is provided by both {} and {}",
                first.display(),
                second.display()
            ),
        }
    }
}

impl Error for ModuleGraphError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReadDirectory { source, .. }
            | Self::ReadDirectoryEntry { source, .. }
            | Self::ReadFileType { source, .. }
            | Self::ReadModule { source, .. } => Some(source),
            Self::InvalidModuleName { .. } | Self::DuplicateModule { .. } => None,
        }
    }
}

fn is_vix_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
