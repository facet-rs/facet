// measure-bloat/src/workspace.rs

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::{DocumentMut, Item, Value};

/// Represents a successfully prepared workspace for a specific variant.
///
/// The `path` is the root of this workspace.
/// If a temporary worktree or directory was created, `is_temporary` will be true,
/// indicating it needs cleanup.
#[derive(Debug)]
pub(crate) struct PreparedWorkspace {
    pub path: PathBuf,
    pub variant_name: String,
    pub is_temporary: bool,
    pub temp_worktree_path: Option<PathBuf>, // Specific to git worktree
}

/// Sets up the source code checkout/path for a given variant.
///
/// - For "head" or "serde", it uses the `base_repo_path` as these are built from HEAD.
/// - For "main-facet-sources", it creates a git worktree of the `main_branch_name`
///   from `base_repo_path` in a temporary directory. This worktree is *only* for
///   sourcing `facet-*` crates for the hybrid main build.
pub(crate) fn setup_variant_source_checkout(
    variant_name: &str, // "head", "main-facet-sources", "serde"
    base_repo_path: &Path,
    main_branch_name: &str, // e.g., "main"
) -> Result<PreparedWorkspace> {
    println!(
        "[workspace] Setting up source checkout for variant: '{}' from base: {:?}",
        variant_name, base_repo_path
    );

    match variant_name {
        "head" | "serde" => {
            if !base_repo_path.join("Cargo.toml").exists() {
                anyhow::bail!(
                    "Base repository path {:?} does not appear to be a valid Rust project (missing Cargo.toml).",
                    base_repo_path
                );
            }
            Ok(PreparedWorkspace {
                path: base_repo_path.to_path_buf(),
                variant_name: variant_name.to_string(),
                is_temporary: false,
                temp_worktree_path: None,
            })
        }
        "main-facet-sources" => {
            let timestamp = chrono::Utc::now().timestamp_millis(); // Requires chrono crate
            let worktree_dir_name =
                format!("facet-main-src-worktree-{}-{}", main_branch_name, timestamp);
            let worktree_path = std::env::temp_dir().join(worktree_dir_name);

            if worktree_path.exists() {
                fs::remove_dir_all(&worktree_path).with_context(|| {
                    format!(
                        "Failed to remove existing temporary worktree directory: {:?}",
                        worktree_path
                    )
                })?;
            }

            println!(
                "[workspace] Creating git worktree for branch '{}' at {:?}",
                main_branch_name, worktree_path
            );
            let status = Command::new("git")
                .arg("-C")
                .arg(base_repo_path) // Assumes base_repo_path is the main .git containing dir
                .arg("worktree")
                .arg("add")
                .arg("--detach") // Checkout the commit, not the branch itself
                .arg(&worktree_path)
                .arg(main_branch_name)
                .status()
                .with_context(|| {
                    format!(
                        "Failed to execute git worktree add for branch '{}' at {:?}",
                        main_branch_name, worktree_path
                    )
                })?;

            if !status.success() {
                anyhow::bail!(
                    "git worktree add command failed for branch '{}' (status: {}). Worktree path: {:?}. Ensure base repo {:?} is not shallow and branch exists.",
                    main_branch_name,
                    status,
                    worktree_path,
                    base_repo_path
                );
            }

            println!(
                "[workspace] Successfully created git worktree for main branch facet sources at: {:?}",
                worktree_path
            );
            Ok(PreparedWorkspace {
                path: worktree_path.clone(),
                variant_name: variant_name.to_string(),
                is_temporary: true,
                temp_worktree_path: Some(worktree_path),
            })
        }
        _ => anyhow::bail!(
            "Unsupported variant name for source checkout: {}",
            variant_name
        ),
    }
}

/// Creates the hybrid workspace for the "main-facet-hybrid" variant.
/// This involves copying facet-* crates from `main_facet_source_path` and
/// ks-* crates + root files from `head_repo_path` into `hybrid_ws_root`.
pub(crate) fn create_hybrid_main_variant_workspace(
    hybrid_ws_root: &Path,             // Root for the new synthetic workspace
    head_repo_path: &Path,             // Path to the HEAD checkout
    main_facet_source_path: &Path,     // Path to the worktree with main's facet-* sources
    core_facet_crate_names: &[String], // Names of facet-* directories, e.g., "facet-core"
    head_specific_crates_config: &[(String, String)], // (name, path_str relative to head_repo_path)
) -> Result<()> {
    println!(
        "[workspace] Creating hybrid main variant workspace at: {:?}",
        hybrid_ws_root
    );
    // Caller should ensure hybrid_ws_root is clean or newly created.
    // fs::create_dir_all(hybrid_ws_root).context...

    // 1. Copy root Cargo.toml, Cargo.lock, .cargo/ from HEAD
    for item_name in &["Cargo.toml", "Cargo.lock"] {
        let src_item = head_repo_path.join(item_name);
        if src_item.exists() {
            fs::copy(&src_item, hybrid_ws_root.join(item_name)).with_context(|| {
                format!(
                    "Failed to copy '{}' from HEAD {:?} to hybrid workspace {:?}",
                    item_name, src_item, hybrid_ws_root
                )
            })?;
        }
    }
    let head_dot_cargo_dir = head_repo_path.join(".cargo");
    if head_dot_cargo_dir.is_dir() {
        copy_dir_recursive(&head_dot_cargo_dir, &hybrid_ws_root.join(".cargo"))
            .context("Failed to copy .cargo directory to hybrid workspace")?;
    }

    // 2. Copy core_facet_crate_names from main_facet_source_path (the main worktree)
    // These are assumed to be top-level directories in the source worktree and hybrid workspace.
    for facet_crate_dir_name in core_facet_crate_names {
        let src_facet_crate_path = main_facet_source_path.join(facet_crate_dir_name);
        let dest_facet_crate_path = hybrid_ws_root.join(facet_crate_dir_name);
        if src_facet_crate_path.is_dir() {
            copy_dir_recursive(&src_facet_crate_path, &dest_facet_crate_path).with_context(
                || {
                    format!(
                        "Failed to copy core facet crate '{}' from {:?} to {:?}",
                        facet_crate_dir_name, src_facet_crate_path, dest_facet_crate_path
                    )
                },
            )?;
        } else {
            eprintln!(
                "Warning: Core Facet crate source directory not found in main worktree: {:?}. This crate will be missing in the hybrid build.",
                src_facet_crate_path
            );
        }
    }

    // 3. Copy head_specific_crates (e.g., ks-* crates) from head_repo_path
    // These maintain their relative path structure (e.g., "outside-workspace/ks-types").
    for (_crate_name, ks_crate_rel_path_str) in head_specific_crates_config {
        let ks_crate_rel_path = PathBuf::from(ks_crate_rel_path_str);
        let src_ks_crate_path = head_repo_path.join(&ks_crate_rel_path);
        let dest_ks_crate_path = hybrid_ws_root.join(&ks_crate_rel_path);

        if src_ks_crate_path.is_dir() {
            if let Some(parent) = dest_ks_crate_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "Failed to create parent directory {:?} for ks-crate copy",
                        parent
                    )
                })?;
            }
            copy_dir_recursive(&src_ks_crate_path, &dest_ks_crate_path).with_context(|| {
                format!(
                    "Failed to copy head-specific crate from {:?} to {:?}",
                    src_ks_crate_path, dest_ks_crate_path
                )
            })?;
        } else {
            eprintln!(
                "Warning: Head-specific crate source directory not found in HEAD: {:?}. This crate will be missing in the hybrid build.",
                src_ks_crate_path
            );
        }
    }
    Ok(())
}

/// Rewrites path dependencies in Cargo.toml files of head-specific (e.g., ks-*) crates
/// within the hybrid workspace. Paths to core_facet_crate dependencies are adjusted
/// to point to their new locations (now peers) within the hybrid workspace.
pub(crate) fn rewrite_paths_in_hybrid_workspace_ks_crates(
    hybrid_ws_root: &Path, // Root of the synthetic workspace
    head_specific_crates_config: &[(String, String)], // (crate_name, path_str relative to hybrid_ws_root)
    core_facet_crate_names: &[String], // Names of facet-* directories, now at hybrid_ws_root
) -> Result<()> {
    println!("[workspace] Rewriting paths in hybrid workspace for head-specific crates...");

    for (_ks_crate_name, ks_crate_rel_path_str) in head_specific_crates_config {
        let ks_crate_dir_in_hybrid = hybrid_ws_root.join(ks_crate_rel_path_str);
        let ks_cargo_toml_path = ks_crate_dir_in_hybrid.join("Cargo.toml");

        if !ks_cargo_toml_path.exists() {
            eprintln!(
                "Warning: Cargo.toml not found for head-specific crate at: {:?}. Skipping path rewrite.",
                ks_cargo_toml_path
            );
            continue;
        }

        let toml_content = fs::read_to_string(&ks_cargo_toml_path)
            .with_context(|| format!("Failed to read Cargo.toml at {:?}", ks_cargo_toml_path))?;
        let mut doc = toml_content
            .parse::<DocumentMut>()
            .with_context(|| format!("Failed to parse Cargo.toml at {:?}", ks_cargo_toml_path))?;

        let mut modified_toml = false;

        for section_key in &["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(deps) = doc
                .get_mut(section_key)
                .and_then(|item| item.as_table_mut())
            {
                for (dep_key, dep_value_item) in deps.iter_mut() {
                    let dep_name_str = dep_key.get();
                    if core_facet_crate_names
                        .iter()
                        .any(|f_name| f_name == dep_name_str)
                    {
                        // This dependency is a core facet crate.
                        // Assume core_facet_crate is now a sibling directory at hybrid_ws_root.
                        let target_facet_crate_path_in_hybrid = hybrid_ws_root.join(dep_name_str);

                        // Calculate the new relative path from ks_crate_dir_in_hybrid
                        // to target_facet_crate_path_in_hybrid.
                        let new_relative_path = pathdiff::diff_paths(
                            &target_facet_crate_path_in_hybrid,
                            &ks_crate_dir_in_hybrid,
                        )
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Could not calculate relative path from {:?} to {:?} for dependency {}",
                                ks_crate_dir_in_hybrid, target_facet_crate_path_in_hybrid, dep_name_str
                            )
                        })?;

                        let new_path_val_str = new_relative_path.to_string_lossy().into_owned();

                        if let Some(dep_table) = dep_value_item.as_inline_table_mut() {
                            if dep_table.contains_key("path") {
                                dep_table.insert("path", Value::from(new_path_val_str.clone()));
                                modified_toml = true;
                            }
                        } else if let Some(dep_table) = dep_value_item.as_table_mut() {
                            if dep_table.contains_key("path") {
                                dep_table["path"] =
                                    Item::Value(Value::from(new_path_val_str.clone()));
                                modified_toml = true;
                            }
                        }
                        // Note: This doesn't handle cases where `path` might be part of a target-specific dependency table.
                        // e.g. [target.'cfg(windows)'.dependencies.my-facet-crate]
                        // For simpler workspace structures, this should be sufficient.

                        if modified_toml {
                            println!(
                                "  Rewrote path for dep '{}' in {:?} to: {}",
                                dep_name_str, ks_cargo_toml_path, new_path_val_str
                            );
                        }
                    }
                }
            }
        }

        if modified_toml {
            fs::write(&ks_cargo_toml_path, doc.to_string()).with_context(|| {
                format!(
                    "Failed to write modified Cargo.toml to {:?}",
                    ks_cargo_toml_path
                )
            })?;
        }
    }
    Ok(())
}

/// Cleans up a prepared workspace.
/// If it was a temporary git worktree (e.g., `main-facet-sources`), it removes the worktree.
/// If it was a synthetic hybrid workspace, the caller is responsible for removing its root directory.
pub(crate) fn cleanup_prepared_workspace(
    prepared_ws: PreparedWorkspace,
    base_repo_path_for_worktree_ops: Option<&Path>, // Required if it was a git worktree
) -> Result<()> {
    println!(
        "[workspace] Cleaning up workspace for variant: '{}' at path: {:?}",
        prepared_ws.variant_name, prepared_ws.path
    );

    if prepared_ws.is_temporary {
        if let Some(worktree_path) = prepared_ws.temp_worktree_path {
            // This was a git worktree (e.g., for main-facet-sources)
            let base_path = base_repo_path_for_worktree_ops.ok_or_else(|| {
                anyhow::anyhow!(
                    "base_repo_path_for_worktree_ops is required to remove git worktree at {:?}",
                    worktree_path
                )
            })?;
            println!("[workspace] Removing git worktree at: {:?}", worktree_path);
            let status = Command::new("git")
                .arg("-C")
                .arg(base_path)
                .arg("worktree")
                .arg("remove")
                .arg("--force") // Use --force to remove even if there are uncommitted changes or untracked files
                .arg(&worktree_path)
                .status()
                .with_context(|| {
                    format!(
                        "Failed to execute git worktree remove for {:?}",
                        worktree_path
                    )
                })?;

            if !status.success() {
                // Log error but proceed. The directory might still be there.
                eprintln!(
                    "Warning: git worktree remove command failed (status: {}) for {:?}. The directory might still exist.",
                    status, worktree_path
                );
            }
            // Attempt to remove the directory even if `git worktree remove` had issues or left it.
            if worktree_path.exists() {
                fs::remove_dir_all(&worktree_path).with_context(|| {
                    format!(
                        "Failed to remove worktree directory after git command: {:?}",
                        worktree_path
                    )
                })?;
            }
        } else if prepared_ws.variant_name == "main-facet-hybrid"
            || prepared_ws.path.starts_with(std::env::temp_dir())
        {
            // This logic implies the caller created a temp dir for the hybrid workspace
            // and passed its path here. The caller is usually responsible for removing
            // dirs created by tempfile::tempdir().
            // If `prepared_ws.path` itself is a temporary dir for hybrid ws, then:
            println!(
                "[workspace] Note: Cleanup of hybrid workspace directory {:?} is typically handled by the caller if created with tempfile::tempdir().",
                prepared_ws.path
            );
            // fs::remove_dir_all(&prepared_ws.path).with_context(|| {
            //     format!(
            //         "Failed to remove temporary hybrid workspace directory: {:?}",
            //         prepared_ws.path
            //     )
            // })?;
        }
        // Any other temporary files created by this module (e.g. .cargo/config.toml backups) would be cleaned here.
    }
    Ok(())
}

/// Basic recursive directory copy function.
/// Skips `.git` and `target` directories.
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| {
        format!(
            "[copy_dir_recursive] Failed to create destination directory: {:?}",
            dst
        )
    })?;
    for entry_res in fs::read_dir(src).with_context(|| {
        format!(
            "[copy_dir_recursive] Failed to read source directory: {:?}",
            src
        )
    })? {
        let entry = entry_res?;
        let ty = entry.file_type()?;
        let src_path = entry.path();

        if let Some(file_name) = src_path.file_name() {
            if file_name == ".git" || file_name == "target" {
                continue; // Skip .git and target directories
            }
        }

        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "[copy_dir_recursive] Failed to copy file from {:?} to {:?}",
                    src_path, dst_path
                )
            })?;
        }
    }
    Ok(())
}
