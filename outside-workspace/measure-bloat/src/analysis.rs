// measure-bloat/src/analysis.rs

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

// Assuming types like CrateRlibSize are defined in crate::types
use crate::types::CrateRlibSize;

/// Collects the sizes of .rlib files for specified crates from the build artifacts directory.
///
/// # Arguments
/// * `build_artifacts_target_dir`: Path to the root of the target directory where `cargo build` placed artifacts.
///                                 Example: `/tmp/facet-measure-artifacts/head-facet/my_target/`
/// * `crates_to_analyze`: A list of crate names (as they appear in `Cargo.toml`, e.g., "my-crate")
///                        whose .rlib files should be found and measured.
///
/// # Returns
/// A `Result` containing a vector of `CrateRlibSize` structs.
pub(crate) fn collect_rlib_sizes(
    build_artifacts_target_dir: &Path,
    crates_to_analyze: &[String],
) -> Result<Vec<CrateRlibSize>> {
    let mut rlib_sizes = Vec::new();
    let deps_dir = build_artifacts_target_dir.join("release").join("deps");

    log::debug!(
        "[analysis] Collecting .rlib sizes from deps_dir: {:?}, for crates: {:?}",
        deps_dir,
        crates_to_analyze
    );

    if !deps_dir.exists() || !deps_dir.is_dir() {
        log::warn!(
            "[analysis] deps directory not found or not a directory: {:?}. No .rlib sizes will be collected.",
            deps_dir
        );
        return Ok(rlib_sizes);
    }

    for entry_res in fs::read_dir(&deps_dir)
        .with_context(|| format!("[analysis] Failed to read deps directory: {:?}", deps_dir))?
    {
        let entry = entry_res.context("[analysis] Failed to read directory entry in deps")?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "rlib") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // stem is like "libcrate_name_with_underscores-hash" or "libcrate_name_with_underscores"
                for crate_name_from_config in crates_to_analyze {
                    // Normalize the crate name from config (e.g., "my-crate" -> "my_crate")
                    // because .rlib filenames use underscores.
                    let normalized_config_crate_name = crate_name_from_config.replace('-', "_");
                    let expected_prefix = format!("lib{}", normalized_config_crate_name);

                    if stem.starts_with(&expected_prefix) {
                        // Further check: ensure it's not just a prefix of another crate,
                        // e.g., "libmy_crate" vs "libmy_crate_extra".
                        // It should be "libmy_crate.rlib" or "libmy_crate-hash.rlib".
                        if stem.len() == expected_prefix.len() || // exact match, e.g. libfoo.rlib
                            stem.chars().nth(expected_prefix.len()) == Some('-')
                        // hash follows, e.g. libfoo-abcdef.rlib
                        {
                            let metadata = fs::metadata(&path).with_context(|| {
                                format!("[analysis] Failed to get metadata for rlib: {:?}", path)
                            })?;
                            log::trace!(
                                "[analysis] Found .rlib for \'{}\': {:?} (size: {})",
                                crate_name_from_config,
                                path,
                                metadata.len()
                            );
                            rlib_sizes.push(CrateRlibSize {
                                name: crate_name_from_config.clone(), // Use original name from config for reporting
                                size: metadata.len(),
                            });
                            // Found the .rlib for this crate_name_from_config, break from inner loop
                            break;
                        }
                    }
                }
            }
        }
    }

    // Sort for consistent output and easier diffing/review
    rlib_sizes.sort_by(|a, b| a.name.cmp(&b.name));
    // Optional: Deduplicate if multiple .rlibs could match (e.g. due to hashes or slightly different names)
    // For now, the first match (by iterating through crates_to_analyze) is taken. If a crate produces multiple
    // .rlib files that match this pattern (unlikely for direct dependencies), this could be an issue.
    // However, `crates_to_analyze` should contain unique crate names.

    log::debug!("[analysis] Collected .rlib sizes: {:?}", rlib_sizes);
    Ok(rlib_sizes)
}

/// Gets the file size of the main compiled executable (binary or example).
///
/// # Arguments
/// * `build_artifacts_target_dir`: Path to the root of the target directory.
/// * `binary_name_from_config`: The name of the binary or example as specified in `MeasurementTarget`
///                              (e.g., "my_app", "example_benchmark").
///
/// # Returns
/// A `Result` containing an `Option<u64>` with the file size in bytes, or `None` if not found.
pub(crate) fn get_main_executable_size(
    build_artifacts_target_dir: &Path,
    binary_name_from_config: &str,
) -> Result<Option<u64>> {
    let release_dir = build_artifacts_target_dir.join("release");

    log::debug!(
        "[analysis] Searching for main executable \'{}\' in directory: {:?}",
        binary_name_from_config,
        release_dir
    );

    // Cargo might replace hyphens with underscores in the actual filename.
    let name_hyphens = binary_name_from_config.to_string();
    let name_underscores = binary_name_from_config.replace('-', "_");

    let mut candidates = vec![
        release_dir.join(&name_hyphens),
        release_dir.join(&name_underscores),
    ];

    // On Windows, executables have a .exe extension.
    if cfg!(windows) {
        candidates.push(release_dir.join(format!("{}.exe", name_hyphens)));
        candidates.push(release_dir.join(format!("{}.exe", name_underscores)));
    }

    for path_candidate in candidates {
        if path_candidate.exists() && path_candidate.is_file() {
            let metadata = fs::metadata(&path_candidate).with_context(|| {
                format!(
                    "[analysis] Failed to get metadata for executable: {:?}",
                    path_candidate
                )
            })?;
            log::info!(
                "[analysis] Found main executable at: {:?} (size: {})",
                path_candidate,
                metadata.len()
            );
            return Ok(Some(metadata.len()));
        }
    }

    log::warn!(
        "[analysis] Main executable \'{}\' (or variants like \'{}\') not found in {:?}. Tested candidates.",
        name_hyphens,
        name_underscores,
        release_dir
    );
    Ok(None)
}
