// measure-bloat/src/config.rs
//! Configuration structures and accessors for the measurement utility.

use serde::{Deserialize, Serialize};
// Ensure types from types.rs are not re-declared here if MeasurementTarget were to be moved.
// For now, MeasurementTarget is defined here.

/// Defines a specific target for performance and size measurement.
///
/// A measurement target usually represents a piece of functionality or a specific binary
/// that can be implemented using Facet and, optionally, Serde for comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementTarget {
    /// A unique, user-friendly name for this measurement target.
    /// Used for identification in reports. Example: "json-serialization-benchmark".
    pub name: String,

    /// The name of the binary or example to build for the Facet implementation.
    /// This is used with `cargo build --bin <name>` or `cargo build --example <name>`.
    /// Example: "json_benchmark_facet_bin".
    pub facet_binary_name: String,

    /// Optional: The name of the binary or example to build for the Serde implementation.
    /// If `None`, this target might not have a direct Serde comparison.
    /// Example: "json_benchmark_serde_bin".
    pub serde_binary_name: Option<String>,

    /// List of Facet-related crate names that should be specifically analyzed
    /// (e.g., for LLVM lines, .rlib sizes) when measuring the Facet variants.
    /// These names should match the crate names as defined in `Cargo.toml`.
    /// Example: `vec!["facet-core".to_string(), "ks-facet".to_string()]`.
    pub facet_crates_to_analyze: Vec<String>,

    /// List of Serde-related crate names for specific analysis when measuring
    /// the Serde variant.
    /// Example: `vec!["serde".to_string(), "serde_json".to_string(), "ks-serde-types".to_string()]`.
    pub serde_crates_to_analyze: Vec<String>,

    /// List of crate names considered as "core Facet" crates.
    /// When building the "main-facet" (hybrid) variant, the source code for these
    /// crates will be taken from the `main` branch worktree.
    /// Example: `vec!["facet-core".to_string(), "facet-runtime".to_string()]`.
    pub core_facet_crate_names: Vec<String>,

    /// Configuration for `ks-*` crates or other crates that are part of the broader
    /// workspace and are always sourced from `HEAD`.
    /// Each tuple contains:
    ///  1. The crate name (e.g., "ks-types").
    ///  2. The path to this crate relative to the repository root (e.g., "outside-workspace/ks-types").
    ///
    /// This is used for constructing the hybrid `main-facet` workspace.
    /// These paths are also used to locate the `Cargo.toml` files for path rewriting.
    pub head_specific_crates_config: Vec<(String, String)>,
}

/// Retrieves the list of measurement targets.
///
/// In the future, this could load from a configuration file (e.g., YAML or TOML).
/// For now, it returns a hardcoded list.
pub fn get_measurement_targets() -> Vec<MeasurementTarget> {
    vec![
        // Main comparison target: ks-facet vs ks-serde
        MeasurementTarget {
            name: "ks-facet".to_string(),
            facet_binary_name: "ks-facet".to_string(), // Binary in ks-facet crate
            serde_binary_name: Some("ks-serde".to_string()), // Binary in ks-serde crate
            facet_crates_to_analyze: vec![
                "ks-facet".to_string(),
                "ks-mock".to_string(),
                "ks-types".to_string(),
                "ks-facet-json-read".to_string(),
                "ks-facet-json-write".to_string(),
                "ks-facet-pretty".to_string(),
            ],
            serde_crates_to_analyze: vec![
                "ks-serde".to_string(),
                "ks-mock".to_string(),
                "ks-types".to_string(),
                "ks-serde-json-read".to_string(),
                "ks-serde-json-write".to_string(),
                "ks-debug".to_string(),
            ],
            core_facet_crate_names: get_core_facet_crate_names(),
            head_specific_crates_config: get_ks_crates_config(),
        },
    ]
}

/// Returns a list of crate names considered "core Facet" libraries.
/// These are the crates whose source code will be taken from the `main` branch
/// when constructing the hybrid `main-facet` build variant.
/// The names should match the directory names of these crates.
pub fn get_core_facet_crate_names() -> Vec<String> {
    // All facet-* crates from the main workspace that should be sourced from the main branch
    vec![
        "facet".to_string(),
        "facet-core".to_string(),
        "facet-macros".to_string(),
        "facet-macros-emit".to_string(),
        "facet-macros-parse".to_string(),
        "facet-reflect".to_string(),
        "facet-serialize".to_string(),
        "facet-deserialize".to_string(),
        "facet-json".to_string(),
        "facet-yaml".to_string(),
        "facet-toml".to_string(),
        "facet-msgpack".to_string(),
        "facet-csv".to_string(),
        "facet-kdl".to_string(),
        "facet-xdr".to_string(),
        "facet-urlencoded".to_string(),
        "facet-jsonschema".to_string(),
        "facet-args".to_string(),
        "facet-pretty".to_string(),
        "facet-bench".to_string(),
        "facet-dev".to_string(),
        "facet-testhelpers".to_string(),
        "facet-testhelpers-macros".to_string(),
    ]
}

/// Returns the configuration for `ks-*` (or other "outer workspace") crates.
/// These crates are always sourced from the `HEAD` checkout.
/// The configuration maps the crate name to its path relative to the repository root.
/// This is used for:
/// 1. Copying these crates into the hybrid `main-facet` workspace.
/// 2. Locating their `Cargo.toml` files for rewriting `path` dependencies to core Facet crates.
pub fn get_ks_crates_config() -> Vec<(String, String)> {
    // All crates in the outside-workspace directory that should be sourced from HEAD
    vec![
        // Core types and mock data
        (
            "ks-types".to_string(),
            "outside-workspace/ks-types".to_string(),
        ),
        (
            "ks-mock".to_string(),
            "outside-workspace/ks-mock".to_string(),
        ),
        // Facet-based crates
        (
            "ks-facet".to_string(),
            "outside-workspace/ks-facet".to_string(),
        ),
        (
            "ks-facet-json-read".to_string(),
            "outside-workspace/ks-facet-json-read".to_string(),
        ),
        (
            "ks-facet-json-write".to_string(),
            "outside-workspace/ks-facet-json-write".to_string(),
        ),
        (
            "ks-facet-pretty".to_string(),
            "outside-workspace/ks-facet-pretty".to_string(),
        ),
        // Serde-based crates
        (
            "ks-serde".to_string(),
            "outside-workspace/ks-serde".to_string(),
        ),
        (
            "ks-serde-json-read".to_string(),
            "outside-workspace/ks-serde-json-read".to_string(),
        ),
        (
            "ks-serde-json-write".to_string(),
            "outside-workspace/ks-serde-json-write".to_string(),
        ),
        (
            "ks-debug".to_string(),
            "outside-workspace/ks-debug".to_string(),
        ),
        // The measure-bloat tool itself (for path rewriting)
        (
            "measure-bloat".to_string(),
            "outside-workspace/measure-bloat".to_string(),
        ),
    ]
}

// TODO: Consider loading MeasurementTarget configurations from a file (e.g., `measure-config.toml`)
//       to make it easier to manage and extend without recompiling.
// TODO: Add validation for MeasurementTarget entries, e.g., ensuring crate names or paths are plausible.
