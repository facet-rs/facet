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

In addition to comparing `HEAD` and `main` versions of Facet-based targets, the utility also supports comparing against a Serde-based implementation for specific targets.

1.  **`HEAD` Variant (Facet)**:
    *   **Source**: Uses the current repository checkout (where the `measure-bloat` tool is run).
    *   **Workspace**: Assumes the `Cargo.toml` files in the `HEAD` checkout have correct relative `path` dependencies for all workspace members (both `ks-*` and `facet-*` crates involved in the Facet implementation).
    *   **Patching**: No specific patching of internal `path` dependencies is typically needed for the Facet build.
    *   **Build Artifacts**: Stored in an isolated temporary directory (e.g., `/tmp/facet-build-artifacts/head-facet/...`).

2.  **`main` Variant (Facet - Hybrid Build)**:
    *   **Source `facet-*` crates**: A temporary `git worktree` is created from the specified `main` branch to get the source code for `facet-*` core crates.
    *   **Source `ks-*` crates & Root Workspace**: Copied from the `HEAD` repository checkout. This includes the root `Cargo.toml`, `Cargo.lock`, and directories like `outside-workspace/`.
    *   **Synthetic Workspace**: A new temporary directory is created (e.g., `/tmp/hybrid-main-build-XYZ/`).
        *   The `facet-*` crate sources (from the `main` worktree) are copied into this synthetic workspace.
        *   The `ks-*` crate sources and the root `Cargo.toml`/`Cargo.lock` (from `HEAD`) are copied into this synthetic workspace.
    *   **Path Rewriting**: The `Cargo.toml` files for the `ks-*` crates (now in the synthetic workspace) have their `path` dependencies (which originally pointed to `facet-*` crates relative to the `HEAD` checkout) rewritten. These paths are adjusted to point correctly to the `facet-*` crates now also located within the synthetic workspace. This is done using `toml_edit`.
    *   **Build Artifacts**: Stored in an isolated temporary directory (e.g., `/tmp/facet-build-artifacts/main-facet/...`).

3.  **`serde` Variant**:
    *   **Purpose**: To provide a performance and size baseline using `serde` for equivalent functionality defined in a measurement target.
    *   **Source**: Uses the current `HEAD` repository checkout. The `ks-*` crates (e.g., `ks-serde`, `ks-mock` using serde) are built as they exist in the `HEAD`.
    *   **Workspace**: Assumes the `Cargo.toml` files in the `HEAD` checkout are correctly configured for the Serde implementation (e.g., `ks-serde` might depend on `serde`, `serde_json`, etc., and `ks-mock` would use these).
    *   **Patching**: No special patching of internal `path` dependencies related to Facet core is relevant here. Standard Cargo dependency resolution applies.
    *   **Build Artifacts**: Stored in an isolated temporary directory (e.g., `/tmp/facet-build-artifacts/serde/...`).

## Module Structure (`measure-bloat/src/`)

### 1. `main.rs`
*   **Responsibility**: Application entry point, top-level orchestration of the comparison process.
*   **Contents**:
    *   `main()` function.
    *   `run_comparison()`: Manages the overall workflow.
        *   For each `MeasurementTarget` defined in `config` module:
            *   **HEAD (Facet) Variant**:
                *   Sets up the HEAD workspace (typically using the current directory).
                *   Calls `measure_single_target_variant()` for the "head-facet" variant, using `facet_binary_name` and `facet_crates_to_analyze`.
            *   **main (Facet) Variant (Hybrid)**:
                *   Sets up the hybrid main workspace (main `facet-*` sources, HEAD `ks-*` sources).
                *   Calls `measure_single_target_variant()` for the "main-facet" variant, using `facet_binary_name` and `facet_crates_to_analyze`.
            *   **serde Variant (if `serde_binary_name` is Some)**:
                *   Uses the HEAD workspace (as `ks-serde` etc. are from HEAD).
                *   Calls `measure_single_target_variant()` for the "serde" variant, using `serde_binary_name` and `serde_crates_to_analyze`.
        *   Aggregates all `BuildResult`s (`HEAD` Facet, `main` Facet, `serde`).
        *   Calls `report::generate_comparison_report()` which will now include Serde results in comparisons.
        *   Handles high-level error reporting and progress indication.
*   **Dependencies**: `cli`, `config`, `workspace`, `build`, `analysis`, `report`, `types`.

### 2. `cli.rs`
*   **Responsibility**: Command-line argument parsing.
*   **Contents**: `Cli`, `Commands` structs (using `clap`).
*   **Dependencies**: `clap`.

### 3. `config.rs`
*   **Responsibility**: Definition and loading of measurement configurations.
*   **Contents**:
    *   `MeasurementTarget` struct: Defines what to measure. Key fields include:
        *   `name`: User-friendly name for the measurement target (e.g., "json-serialization-test").
        *   `facet_binary_name`: The binary or example name for the Facet implementation (e.g., "test-json-facet").
        *   `serde_binary_name`: Optional: The binary or example name for the Serde implementation (e.g., "test-json-serde"). If `None`, this target might not have a Serde comparison.
        *   `facet_crates_to_analyze`: List of Facet-related crates (e.g., "facet-core", "ks-facet") for specific analysis like LLVM lines or .rlib sizes.
        *   `serde_crates_to_analyze`: List of Serde-related crates (e.g., "serde", "serde_json", "ks-serde") for analysis when measuring the Serde variant.
    *   `get_measurement_targets()`: Provides the list of `MeasurementTarget`s.
    *   Lists/definitions of which crates are considered "core facet" (for hybrid main build) vs. "ks/other".
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
        "facet-build-artifacts/{}/{}", // e.g., .../head-facet/json-test
        variant_name,                 // variant_name now includes "facet" or "serde"
        target_config.name // Sanitize name for path
    ));
    // Ensure this directory is clean/created

    // Determine binary name and crates to analyze based on variant
    let (actual_binary_to_build, actual_crates_for_analysis) =
        if variant_name == "serde" { // Example: variant_name could be "head-facet", "main-facet", "serde"
            (
                target_config.serde_binary_name.as_ref().expect("Serde binary name missing for serde variant"),
                &target_config.serde_crates_to_analyze
            )
        } else { // "head-facet" or "main-facet"
            (
                &target_config.facet_binary_name,
                &target_config.facet_crates_to_analyze
            )
        };

    // 2. Build the project, get target_dir (which is build_artifacts_target_dir) and timing_summary
    let build_opts = types::BuildWithLllvmIrOpts {
        manifest_path: active_workspace_path.join("Cargo.toml").to_string_lossy().into_owned(),
        target_dir: Some(build_artifacts_target_dir.clone()), // Crucial: use isolated dir
        env_vars: HashMap::new(), // Customize as needed
    };
    let llvm_build_output = build::build_project_for_analysis(
        actual_binary_to_build, // Pass the correct binary name
        active_workspace_path, // CWD for cargo build
        &build_artifacts_target_dir, // Output target dir
        &build_opts,
    )?;

    // 3. Fetch LLVM lines data using artifacts from build_artifacts_target_dir
    let llvm_lines_summary = build::fetch_llvm_lines_data(
        &llvm_build_output.target_dir, // This is build_artifacts_target_dir
        actual_binary_to_build,
        actual_crates_for_analysis,
        &active_workspace_path.join("Cargo.toml"), // Manifest for llvm-lines
    )?;

    // 4. Analyze .rlib sizes from build_artifacts_target_dir
    let rlib_sizes = analysis::collect_rlib_sizes(
        &llvm_build_output.target_dir, // This is build_artifacts_target_dir
        actual_crates_for_analysis,
    )?;

    // 5. Get main executable size from build_artifacts_target_dir
    let executable_size = analysis::get_main_executable_size(
        &llvm_build_output.target_dir, // This is build_artifacts_target_dir
        actual_binary_to_build,
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
