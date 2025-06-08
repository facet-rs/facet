# Measure Bloat Utility: Design and Structure

This document outlines the design and module structure for the `measure-bloat`
utility.

The primary goal of this utility is to compare build times, binary sizes, and
LLVM IR line counts between the current `HEAD` (e.g., a feature branch or PR)
and the `main` branch of the Facet project.

A key aspect of the "main" branch comparison is a hybrid build strategy:
*   `ks-*` crates (typically found in `outside-workspace/`) are always built from the `HEAD` checkout.
*   `facet-*` core crates are built from the specified `main` branch.
*   These are combined into a temporary, synthetic workspace for the "main" variant measurement.

## Core Principles

*   **Modularity**: Functionality is split into distinct Rust modules, each with a clear responsibility.
*   **Reproducibility**: Builds, especially for the `main` variant, should be as hermetic as possible.
*   **Testability**: Smaller modules allow for more focused unit and integration testing.
*   **Clarity**: The build and comparison process, particularly the hybrid strategy, should be well-documented.

## Build Variant Definitions

1.  **`HEAD` Variant**:
    *   **Source**: Uses the current repository checkout (where the `measure-bloat` tool is run).
    *   **Workspace**: Assumes the `Cargo.toml` files in the `HEAD` checkout have correct relative `path` dependencies for all workspace members (both `ks-*` and `facet-*` crates).
    *   **Patching**: No specific patching of internal `path` dependencies is typically needed.
    *   **Build Artifacts**: Stored in an isolated temporary directory (e.g., `/tmp/facet-build-artifacts/head/...`).

2.  **`main` Variant (Hybrid Build)**:
    *   **Source `facet-*` crates**: A temporary `git worktree` is created from the specified `main` branch to get the source code for `facet-*` core crates.
    *   **Source `ks-*` crates & Root Workspace**: Copied from the `HEAD` repository checkout. This includes the root `Cargo.toml`, `Cargo.lock`, and directories like `outside-workspace/`.
    *   **Synthetic Workspace**: A new temporary directory is created (e.g., `/tmp/hybrid-main-build-XYZ/`).
        *   The `facet-*` crate sources (from the `main` worktree) are copied into this synthetic workspace.
        *   The `ks-*` crate sources and the root `Cargo.toml`/`Cargo.lock` (from `HEAD`) are copied into this synthetic workspace.
    *   **Path Rewriting**: The `Cargo.toml` files for the `ks-*` crates (now in the synthetic workspace) have their `path` dependencies (which originally pointed to `facet-*` crates relative to the `HEAD` checkout) rewritten. These paths are adjusted to point correctly to the `facet-*` crates now also located within the synthetic workspace. This is done using `toml_edit`.
    *   **Build Artifacts**: Stored in an isolated temporary directory (e.g., `/tmp/facet-build-artifacts/main/...`).

## Module Structure (`measure-bloat/src/`)

### 1. `main.rs`
*   **Responsibility**: Application entry point, top-level orchestration of the comparison process.
*   **Contents**:
    *   `main()` function.
    *   `run_comparison()`: Manages the overall workflow.
        *   Sets up checkouts/workspaces for `HEAD` and `main` variants using the `workspace` module.
        *   Loops through `MeasurementTarget`s defined in `config` module.
        *   Calls `measure_single_target_variant()` for each target and variant.
        *   Aggregates `BuildResult`s.
        *   Calls `report::generate_comparison_report()`.
        *   Handles high-level error reporting and progress indication.
*   **Dependencies**: `cli`, `config`, `workspace`, `build`, `analysis`, `report`, `types`.

### 2. `cli.rs`
*   **Responsibility**: Command-line argument parsing.
*   **Contents**: `Cli`, `Commands` structs (using `clap`).
*   **Dependencies**: `clap`.

### 3. `config.rs`
*   **Responsibility**: Definition and loading of measurement configurations.
*   **Contents**:
    *   `MeasurementTarget` struct: Defines what to measure (binary name, relevant facet/serde crates).
    *   `get_measurement_targets()`: Provides the list of targets.
    *   Lists/definitions of which crates are considered "core facet" vs. "ks/other" for the hybrid build.
*   **Dependencies**: `serde`, `types`.

### 4. `types.rs`
*   **Responsibility**: Central repository for all shared data structures.
*   **Contents**:
    *   `BuildResult`: Consolidated result for a single measurement.
    *   `CrateRlibSize`: For `.rlib` file sizes.
    *   `LlvmBuildOutput`, `BuildWithLllvmIrOpts`: Options/output for the build step.
    *   `BuildTimingSummary`, `CrateTiming`, `CargoTimingEntry`, `CargoTimingTarget`: For build timing.
    *   `LlvmLinesSummary`, `CrateLlvmLines`, `LlvmFunction`: For LLVM lines analysis.
    *   `CrateSizeChange`: For diff reporting.
*   **Dependencies**: `serde`.

### 5. `workspace.rs`
*   **Responsibility**: Managing file system environments for measurements, including Git operations, source code aggregation for the hybrid `main` variant, and `Cargo.toml` path rewriting.
*   **Contents**:
    *   `setup_main_facet_source_worktree()`: Creates a git worktree for `facet-*` crates from the `main` branch.
    *   `create_hybrid_main_variant_workspace()`: Assembles the synthetic workspace for the `main` variant by copying `facet-*` sources from the main worktree and `ks-*` (and root) sources from `HEAD`.
    *   `rewrite_paths_in_hybrid_workspace_ks_crates()`: Modifies `path` dependencies in the `Cargo.toml` files of `ks-*` crates within the hybrid workspace to point to the `facet-*` crates (also in the hybrid workspace). Uses `toml_edit`.
    *   `cleanup_main_facet_source_worktree()`: Removes the git worktree for `facet-*` sources.
    *   `cleanup_hybrid_workspace()`: Removes the temporary synthetic workspace directory.
    *   General file/directory utilities (e.g., `copy_dir_recursive`).
*   **Dependencies**: `std::fs`, `std::process`, `anyhow`, `toml_edit`, `pathdiff`.

### 6. `build.rs`
*   **Responsibility**: Interacting directly with `cargo` to build the project and run build-time analyses.
*   **Contents**:
    *   `build_project_for_analysis()`:
        *   Runs `cargo build --release --timings=json ...` with appropriate `RUSTFLAGS` (like `--emit=llvm-ir`).
        *   Operates within the `active_workspace_path` (either HEAD or the hybrid main workspace).
        *   Directs build artifacts to an isolated `build_artifacts_target_dir`.
        *   Parses `cargo-timings.json` to create `BuildTimingSummary`.
        *   Returns `LlvmBuildOutput { target_dir: PathBuf /* the isolated build_artifacts_target_dir */, timing_summary: BuildTimingSummary }`.
    *   `fetch_llvm_lines_data()`:
        *   Runs `cargo llvm-lines ... --target-dir <build_artifacts_target_dir>` for artifacts already built.
        *   Parses the raw output from `cargo llvm-lines`.
        *   Aggregates results into `LlvmLinesSummary`.
*   **Dependencies**: `std::process`, `serde_json`, `types`, `anyhow`.

### 7. `analysis.rs`
*   **Responsibility**: Analyzing file system artifacts produced by the `build` module.
*   **Contents**:
    *   `collect_rlib_sizes()`: Scans `<build_artifacts_target_dir>/release/deps/` for `.rlib` files and records their sizes.
    *   `get_main_executable_size()`: Finds the compiled executable in `<build_artifacts_target_dir>/release/` and returns its size.
*   **Dependencies**: `std::fs`, `types`, `anyhow`.

### 8. `report.rs`
*   **Responsibility**: Generating the final Markdown comparison report.
*   **Contents**:
    *   `generate_comparison_report()`: Takes main and HEAD `BuildResult`s.
    *   `generate_highlights()`: Summarizes key changes.
    *   `generate_size_diff_table()`: Generic function for markdown tables of size differences.
    *   Formatting utilities (`format_bytes`, `format_number`, `format_signed_bytes`).
*   **Dependencies**: `types`, `anyhow`.

## Orchestration Flow (`measure_single_target_variant`)

This function (likely in `main.rs` or a `runner.rs` module) coordinates the measurement for a single target and variant:

```rust
// Simplified pseudo-code
fn measure_single_target_variant(
    target_config: &config::MeasurementTarget,
    active_workspace_path: &Path, // For HEAD, this is base_repo_path. For main, this is hybrid_ws_root.
    variant_name: &str,
    // Potentially other paths like base_repo_path if needed for context
) -> Result<types::BuildResult> {

    // 1. Define isolated build artifacts directory
    let build_artifacts_target_dir = temp_dir().join(format!(
        "facet-build-artifacts/{}/{}",
        variant_name,
        target_config.name // Sanitize name for path
    ));
    // Ensure this directory is clean/created

    // 2. Build the project, get target_dir (which is build_artifacts_target_dir) and timing_summary
    let build_opts = types::BuildWithLllvmIrOpts {
        manifest_path: active_workspace_path.join("Cargo.toml").to_string_lossy().into_owned(),
        target_dir: Some(build_artifacts_target_dir.clone()), // Crucial: use isolated dir
        env_vars: HashMap::new(), // Customize as needed
    };
    let llvm_build_output = build::build_project_for_analysis(
        target_config,
        active_workspace_path, // CWD for cargo build
        &build_artifacts_target_dir, // Output target dir
        &build_opts,
    )?;

    // 3. Fetch LLVM lines data using artifacts from build_artifacts_target_dir
    let llvm_lines_summary = build::fetch_llvm_lines_data(
        &llvm_build_output.target_dir, // This is build_artifacts_target_dir
        target_config,
        &active_workspace_path.join("Cargo.toml"), // Manifest for llvm-lines
    )?;

    // 4. Analyze .rlib sizes from build_artifacts_target_dir
    let rlib_sizes = analysis::collect_rlib_sizes(
        &llvm_build_output.target_dir, // This is build_artifacts_target_dir
        target_config,
    )?;

    // 5. Get main executable size from build_artifacts_target_dir
    let executable_size = analysis::get_main_executable_size(
        &llvm_build_output.target_dir, // This is build_artifacts_target_dir
        target_config,
    )?;

    Ok(types::BuildResult {
        target: target_config.name.clone(),
        variant: variant_name.to_string(),
        file_size: executable_size,
        text_section_size: None, // No tool for this yet
        build_time_ms: llvm_build_output.timing_summary.total_duration.as_millis(),
        rlib_sizes,
        llvm_lines: Some(llvm_lines_summary),
    })
}
```

This detailed plan provides a solid foundation for refactoring the `measure-bloat` utility.
