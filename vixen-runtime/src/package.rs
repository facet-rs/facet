//! Running a package's manifest-named commands — the MVP slice of
//! `r[vixen.package.run-mvp]` (`/spec/vixen/packages`).
//!
//! `prepare_command(dir, name)` is resolution rung 1 in its smallest honest
//! form: read `<dir>/package.styx` through the `vix-package-schema` document
//! types (the embedder's window onto the manifest — the same Shape the styx
//! tooling and the vix rail read), resolve the command in the manifest's
//! command map, follow the fn ref into the directory's `.vix` files (the
//! named module is the root file, its siblings are the module set — exactly
//! what [`crate::module_graph`] already gives a directory), and select the
//! named root. Everything the MVP defers is a typed [`PackageError`] naming
//! the deferral, never a silent gap.
//!
//! Deliberately absent, per the spec page: input resolution (capabilities
//! come from the machine manifest alone), the registry rung, artifacts,
//! program exposure, and argument tails.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use vix_package_schema::PackageManifest;

use crate::module_graph::{ModuleGraph, ModuleGraphError};
use crate::ratchet::{PreparedRun, RunError, prepare_source_with_modules};

/// The manifest's one on-disk name, at the package root
/// (`vixen.package.manifest-is-data`).
pub const MANIFEST_FILE: &str = "package.styx";

/// A resolved, compiled, root-selected command, ready to execute. The
/// [`PreparedRun`] inside is the ordinary production path — nothing about a
/// command run is special beyond how its root was chosen.
pub struct PreparedCommand {
    /// The package's own name, from its manifest header.
    pub package: String,
    /// The command name as invoked.
    pub command: String,
    /// The `module::fn` ref the manifest bound the command to.
    pub fn_ref: String,
    /// The prepared run with exactly the command's root selected.
    pub run: PreparedRun,
}

/// Why a command could not be prepared. Every arm names what was looked for
/// and what exists instead — a refusal that cannot say both sides is a gap,
/// not a refusal.
#[derive(Debug)]
pub enum PackageError {
    /// `<dir>/package.styx` does not exist: the directory is not a package.
    ManifestMissing { path: PathBuf },
    /// The manifest exists and could not be read.
    ManifestRead { path: PathBuf, source: io::Error },
    /// The manifest exists and is not a well-formed package document.
    ManifestDecode { path: PathBuf, detail: String },
    /// The named command is not in the manifest's command map; `declared`
    /// lists what is (the error-with-listing shape).
    UnknownCommand {
        package: String,
        command: String,
        declared: Vec<String>,
    },
    /// A command's fn ref is not the `module::fn` shape.
    MalformedFnRef { command: String, fn_ref: String },
    /// The fn ref names a module with no `.vix` file in the package.
    ModuleMissing {
        command: String,
        module: String,
        directory: PathBuf,
    },
    /// The package directory's module set could not be loaded.
    Modules(ModuleGraphError),
    /// The fn exists without check-stream root standing — the MVP's stated
    /// restriction (`vixen.package.run-mvp`): value-returning command fns
    /// are the named next rung, not a behavior. `declared_roots` lists the
    /// fns that could run.
    MissingRootStanding {
        command: String,
        fn_ref: String,
        declared_roots: Vec<String>,
    },
    /// The package's own code failed to compile or prepare.
    Prepare(RunError),
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageError::ManifestMissing { path } => {
                write!(f, "no package manifest at {}", path.display())
            }
            PackageError::ManifestRead { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            PackageError::ManifestDecode { path, detail } => {
                write!(f, "{} is not a package manifest: {detail}", path.display())
            }
            PackageError::UnknownCommand {
                package,
                command,
                declared,
            } => {
                write!(f, "no command `{command}` in package `{package}`")?;
                if declared.is_empty() {
                    write!(f, "\n  package.styx declares no commands")
                } else {
                    write!(f, "\n  package.styx declares: {}", declared.join(", "))
                }
            }
            PackageError::MalformedFnRef { command, fn_ref } => write!(
                f,
                "command `{command}` names `{fn_ref}`, which is not a `module::fn` path"
            ),
            PackageError::ModuleMissing {
                command,
                module,
                directory,
            } => write!(
                f,
                "command `{command}` names module `{module}`, but {} has no {module}.vix",
                directory.display()
            ),
            PackageError::Modules(error) => write!(f, "{error}"),
            PackageError::MissingRootStanding {
                command,
                fn_ref,
                declared_roots,
            } => {
                write!(
                    f,
                    "command `{command}` names `{fn_ref}`, which has no check-stream root \
                     standing — root standing for value-returning command fns is the named \
                     next rung (vixen.package.run-mvp), not a behavior yet"
                )?;
                if !declared_roots.is_empty() {
                    write!(f, "\n  fns with standing: {}", declared_roots.join(", "))?;
                }
                Ok(())
            }
            PackageError::Prepare(error) => write!(f, "the package's code failed to prepare: {error:#?}"),
        }
    }
}

impl std::error::Error for PackageError {}

/// Resolve and prepare `<directory>`'s `<command>`: manifest → command map →
/// fn ref → root file + module set → compile → select the one named root.
/// The caller executes the returned run with whatever authorities it grants
/// (the CLI's decision, exactly as for a file run).
pub fn prepare_command(
    directory: &Path,
    command: &str,
) -> Result<PreparedCommand, PackageError> {
    let manifest_path = directory.join(MANIFEST_FILE);
    let text = match fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(PackageError::ManifestMissing {
                path: manifest_path,
            });
        }
        Err(source) => {
            return Err(PackageError::ManifestRead {
                path: manifest_path,
                source,
            });
        }
    };
    let manifest: PackageManifest =
        facet_styx::from_str(&text).map_err(|error| PackageError::ManifestDecode {
            path: manifest_path.clone(),
            detail: error.to_string(),
        })?;

    let Some(entry) = manifest.command.get(command) else {
        return Err(PackageError::UnknownCommand {
            package: manifest.package.name,
            command: command.to_owned(),
            declared: manifest.command.keys().cloned().collect(),
        });
    };
    let fn_ref = entry.r#fn.clone();
    let Some((module, fn_name)) = fn_ref.split_once("::") else {
        return Err(PackageError::MalformedFnRef {
            command: command.to_owned(),
            fn_ref,
        });
    };
    if module.is_empty() || fn_name.is_empty() || fn_name.contains("::") {
        return Err(PackageError::MalformedFnRef {
            command: command.to_owned(),
            fn_ref,
        });
    }

    let graph = ModuleGraph::from_dir(directory).map_err(PackageError::Modules)?;
    let Some(root) = graph.root(module) else {
        return Err(PackageError::ModuleMissing {
            command: command.to_owned(),
            module: module.to_owned(),
            directory: directory.to_path_buf(),
        });
    };

    let run = prepare_source_with_modules(root.source(), root.modules())
        .map_err(PackageError::Prepare)?;
    let declared_roots = run.declared_roots();
    if !declared_roots.iter().any(|name| name == fn_name) {
        return Err(PackageError::MissingRootStanding {
            command: command.to_owned(),
            fn_ref,
            declared_roots,
        });
    }

    let run = run.select_roots(&[fn_name]);
    Ok(PreparedCommand {
        package: manifest.package.name,
        command: command.to_owned(),
        fn_ref,
        run,
    })
}
