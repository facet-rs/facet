// measure-bloat/src/runner.rs

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf}; // For temp_dir

use crate::{analysis, build, config, report, types, workspace};

/// Main orchestration function for running the entire comparison.
///
/// It sets up workspaces for HEAD and the main branch (hybrid),
/// iterates through measurement targets, performs measurements for each variant,
/// and then generates a comparison report.
pub fn run_global_comparison(
    cli_repo_path_str: &str,    // Path to the repository (HEAD checkout)
    cli_output_path_str: &str,  // Where to save the report
    cli_main_branch_name: &str, // Name of the main branch (e.g., "main")
) -> Result<()> {
    let base_repo_path = PathBuf::from(cli_repo_path_str)
        .canonicalize()
        .with_context(|| {
            format!(
                "Failed to canonicalize base repository path: {}",
                cli_repo_path_str
            )
        })?;
    let output_path = Path::new(cli_output_path_str);

    println!(
        "🚀 Starting measurement comparison for repository: {:?}",
        base_repo_path
    );
    println!("Comparing against main branch: '{}'", cli_main_branch_name);

    let measurement_targets = config::get_measurement_targets();
    if measurement_targets.is_empty() {
        println!("No measurement targets defined in config.rs. Exiting.");
        return Ok(());
    }

    let mut all_results: Vec<types::BuildResult> = Vec::new();

    // --- HEAD Variant Setup ---
    // The HEAD workspace is simply the provided base_repo_path.
    // No special copying or git operations needed for its source.
    // If HEAD needed specific .cargo/config.toml patching (e.g. for external deps), it would be done here.
    // For now, assuming HEAD's Cargo.toml setup for internal deps is correct.
    let head_workspace_path = base_repo_path.clone();
    println!("HEAD workspace path: {:?}", head_workspace_path);

    // --- Main Variant (Hybrid) Setup ---
    println!("\nSetting up 'main-facet' (hybrid) variant workspace...");
    let main_facet_source_worktree_prep = workspace::setup_variant_source_checkout(
        "main-facet-sources",
        &base_repo_path,
        cli_main_branch_name,
    )?;
    let main_facet_source_path = &main_facet_source_worktree_prep.path;

    let core_facet_crate_names = config::get_core_facet_crate_names();
    let ks_crates_config = config::get_ks_crates_config();

    let hybrid_ws_root_parent = env::temp_dir().join("facet_measure_hybrid_workspaces");
    fs::create_dir_all(&hybrid_ws_root_parent)
        .context("Failed to create parent for hybrid workspaces")?;
    let hybrid_ws_root = tempfile::Builder::new()
        .prefix("hybrid-main-ws-")
        .tempdir_in(hybrid_ws_root_parent)
        .context("Failed to create temporary directory for hybrid workspace")?
        .into_path(); // Keep the PathBuf

    workspace::create_hybrid_main_variant_workspace(
        &hybrid_ws_root,
        &base_repo_path, // Source for ks-crates and root Cargo.toml
        main_facet_source_path,
        &core_facet_crate_names,
        &ks_crates_config,
    )
    .context("Failed to create hybrid main variant workspace structure")?;

    workspace::rewrite_paths_in_hybrid_workspace_ks_crates(
        &hybrid_ws_root,
        &ks_crates_config,
        &core_facet_crate_names,
    )
    .context("Failed to rewrite paths in hybrid workspace ks-crates")?;
    println!(
        "'main-facet' (hybrid) workspace prepared at: {:?}",
        hybrid_ws_root
    );

    // --- Iterate through Measurement Targets ---
    for target_config in &measurement_targets {
        println!("\n🎯 Measuring Target: '{}'", target_config.name);

        // 1. Measure HEAD (Facet)
        println!("   Variant: head-facet");
        let head_result = measure_single_target_variant(
            target_config,
            &head_workspace_path,
            "head-facet",
            &base_repo_path, // For context, e.g. if worktrees need base repo for git ops
        )
        .with_context(|| {
            format!(
                "Failed to measure 'head-facet' for target '{}'",
                target_config.name
            )
        })?;
        all_results.push(head_result);

        // 2. Measure main (Facet - Hybrid)
        println!("   Variant: main-facet (hybrid)");
        let main_result = measure_single_target_variant(
            target_config,
            &hybrid_ws_root, // Use the prepared hybrid workspace
            "main-facet",
            &base_repo_path,
        )
        .with_context(|| {
            format!(
                "Failed to measure 'main-facet' (hybrid) for target '{}'",
                target_config.name
            )
        })?;
        all_results.push(main_result);

        // 3. Measure Serde (if applicable for this target)
        if target_config.serde_binary_name.is_some() {
            println!("   Variant: serde (from HEAD)");
            let serde_result = measure_single_target_variant(
                target_config,
                &head_workspace_path, // Serde variant is built from HEAD sources
                "serde",
                &base_repo_path,
            )
            .with_context(|| {
                format!(
                    "Failed to measure 'serde' for target '{}'",
                    target_config.name
                )
            })?;
            all_results.push(serde_result);
        }
    }

    // --- Cleanup ---
    println!("\n🧹 Cleaning up workspaces...");
    if let Err(e) = workspace::cleanup_prepared_workspace(
        main_facet_source_worktree_prep,
        Some(&base_repo_path),
    ) {
        eprintln!("Error cleaning up main facet source worktree: {:?}", e);
    }
    if let Err(e) = fs::remove_dir_all(&hybrid_ws_root) {
        eprintln!(
            "Error cleaning up hybrid workspace directory {:?}: {:?}",
            hybrid_ws_root, e
        );
    }

    // --- Generate Report ---
    if !all_results.is_empty() {
        println!("\n📊 Generating comparison report...");
        let report_content = report::generate_comparison_report(&all_results)
            .context("Failed to generate comparison report content")?;
        fs::write(output_path, report_content)
            .with_context(|| format!("Failed to write report to {:?}", output_path))?;
        println!("✅ Comparison report generated at: {:?}", output_path);
    } else {
        println!("No results collected. Report generation skipped.");
    }

    Ok(())
}

/// Measures a single target for a specific variant (e.g., "head-facet", "main-facet", "serde").
///
/// This function orchestrates the build, LLVM IR analysis, .rlib size collection,
/// and main executable size measurement for the given configuration.
fn measure_single_target_variant(
    target_config: &config::MeasurementTarget,
    active_workspace_path: &Path, // Path to the root of the workspace to build (HEAD or hybrid)
    variant_name: &str,           // "head-facet", "main-facet", or "serde"
    _base_repo_path: &Path,       // Original repo path, for context (e.g. git ops if needed)
) -> Result<types::BuildResult> {
    println!(
        "    Measuring variant: '{}' for target '{}' in workspace {:?}...",
        variant_name, target_config.name, active_workspace_path
    );

    // 1. Define isolated build artifacts directory
    let build_artifacts_basedir = env::temp_dir().join("facet_measure_artifacts");
    let sanitized_target_name = target_config
        .name
        .replace(|c: char| !c.is_alphanumeric(), "_")
        .to_lowercase();
    let variant_build_target_dir =
        build_artifacts_basedir.join(format!("{}/{}", variant_name, sanitized_target_name));

    if variant_build_target_dir.exists() {
        fs::remove_dir_all(&variant_build_target_dir).with_context(|| {
            format!(
                "Failed to remove existing artifact directory: {:?}",
                variant_build_target_dir
            )
        })?;
    }
    fs::create_dir_all(&variant_build_target_dir).with_context(|| {
        format!(
            "Failed to create artifact directory: {:?}",
            variant_build_target_dir
        )
    })?;
    println!(
        "    Artifacts will be stored in: {:?}",
        variant_build_target_dir
    );

    // Determine binary name and crates to analyze based on variant
    let (actual_binary_to_build, actual_crates_for_analysis) = if variant_name.contains("serde") {
        (
            target_config.serde_binary_name.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Serde binary name is None for target '{}' but 'serde' variant was requested.",
                    target_config.name
                )
            })?,
            &target_config.serde_crates_to_analyze,
        )
    } else {
        // "head-facet" or "main-facet"
        (
            &target_config.facet_binary_name,
            &target_config.facet_crates_to_analyze,
        )
    };
    println!("    Building binary/example: '{}'", actual_binary_to_build);

    // 2. Build the project
    let build_opts = types::BuildWithLllvmIrOpts {
        manifest_path: active_workspace_path
            .join("Cargo.toml")
            .to_string_lossy()
            .into_owned(),
        target_dir: Some(variant_build_target_dir.clone()),
        env_vars: HashMap::new(), // Customize with RUSTFLAGS or other vars if needed
    };

    let llvm_build_output = build::build_project_for_analysis(
        actual_binary_to_build,
        active_workspace_path,     // CWD for cargo build
        &variant_build_target_dir, // Output target dir for cargo
        &build_opts,
    )
    .with_context(|| {
        format!(
            "Build failed for target '{}', variant '{}'",
            target_config.name, variant_name
        )
    })?;

    // 3. Fetch LLVM lines data
    // Note: fetch_llvm_lines_data uses llvm_build_output.target_dir which is variant_build_target_dir
    let llvm_lines_summary = build::fetch_llvm_lines_data(
        &llvm_build_output.target_dir,
        actual_binary_to_build,
        actual_crates_for_analysis,
        &active_workspace_path.join("Cargo.toml"), // Manifest for llvm-lines to find the package/target
    )
    .with_context(|| {
        format!(
            "LLVM lines analysis failed for target '{}', variant '{}'",
            target_config.name, variant_name
        )
    })?;

    // 4. Analyze .rlib sizes
    let rlib_sizes = analysis::collect_rlib_sizes(
        &llvm_build_output.target_dir, // Search in the specific variant's target dir
        actual_crates_for_analysis,
    )
    .with_context(|| {
        format!(
            ".rlib size collection failed for target '{}', variant '{}'",
            target_config.name, variant_name
        )
    })?;

    // 5. Get main executable size
    let main_executable_size = analysis::get_main_executable_size(
        &llvm_build_output.target_dir, // Search in the specific variant's target dir
        actual_binary_to_build,
    )
    .with_context(|| {
        format!(
            "Main executable size check failed for target '{}', variant '{}'",
            target_config.name, variant_name
        )
    })?;

    println!(
        "    Measurement complete for variant: '{}', target '{}'",
        variant_name, target_config.name
    );

    Ok(types::BuildResult {
        target_name: target_config.name.clone(),
        variant_name: variant_name.to_string(),
        main_executable_size,
        // text_section_size: None, // No tool for this yet
        build_time_ms: llvm_build_output.timing_summary.total_duration.as_millis(),
        rlib_sizes,
        llvm_lines: Some(llvm_lines_summary),
        build_timing_summary: llvm_build_output.timing_summary,
    })
}
