//! Helper binary management for cross-process tests.
//!
//! This module provides utilities for locating and using pre-built helper binaries
//! in cross-process tests. When `RAPACE_PREBUILT_HELPERS` is set, tests will only
//! use pre-built binaries and fail immediately if they're missing.
//!
//! # Environment Variables
//!
//! - `RAPACE_PREBUILT_HELPERS`: When set to `1` or `true`, enforce that helper
//!   binaries must be pre-built (skip inline building). Tests will panic if binaries
//!   are not found. This ensures tests don't rebuild binaries during execution.
//!
//! # Usage
//!
//! In your cross-process test:
//!
//! ```ignore
//! use rapace_testkit::helper_binary::find_helper_binary;
//!
//! #[tokio::test]
//! async fn test_my_service() {
//!     // Find the helper binary (will fail fast if not pre-built and RAPACE_PREBUILT_HELPERS is set)
//!     let helper_path = find_helper_binary("my-helper").unwrap();
//!
//!     // Spawn the helper
//!     let mut helper = Command::new(&helper_path)
//!         .args(&["--transport=stream", "--addr=127.0.0.1:9000"])
//!         .spawn()
//!         .expect("failed to spawn helper");
//!
//!     // ... test logic ...
//! }
//! ```

use std::path::PathBuf;

/// Check if pre-built helpers are enforced via environment variable.
///
/// When `RAPACE_PREBUILT_HELPERS=1` or `RAPACE_PREBUILT_HELPERS=true`,
/// tests must use pre-built binaries and will fail if they're missing.
pub fn enforce_prebuilt_helpers() -> bool {
    matches!(
        std::env::var("RAPACE_PREBUILT_HELPERS"),
        Ok(v) if v.to_lowercase() == "1" || v.to_lowercase() == "true"
    )
}

/// Find a pre-built helper binary in the target directory.
///
/// This function:
/// 1. Uses the current executable's path to locate the target directory
/// 2. Looks for the binary in the debug or release subdirectory
/// 3. If `RAPACE_PREBUILT_HELPERS` is set, fails immediately if not found
/// 4. Otherwise, returns an error that tests can use to decide whether to build inline
///
/// # Arguments
///
/// * `binary_name` - The name of the helper binary (e.g., "diagnostics-plugin-helper")
///
/// # Returns
///
/// `Ok(PathBuf)` if the binary is found, `Err(String)` with an error message otherwise.
///
/// # Panics
///
/// If `RAPACE_PREBUILT_HELPERS` is set and the binary is not found.
pub fn find_helper_binary(binary_name: &str) -> Result<PathBuf, String> {
    let enforce = enforce_prebuilt_helpers();

    // Get the current executable's directory
    let current_exe =
        std::env::current_exe().map_err(|e| format!("failed to get current executable: {}", e))?;

    // The test executable is in target/{debug|release}/deps/ (via nextest) or target/{debug|release}/ (via cargo test)
    // We need to find the profile directory (target/debug or target/release) containing the binary
    let mut search_dir = current_exe
        .parent()
        .ok_or_else(|| "could not find parent directory".to_string())?;

    // Try up to 3 levels up to find the profile directory containing helper binaries
    for _ in 0..3 {
        let candidate_path = search_dir.join(binary_name);
        if candidate_path.exists() {
            return Ok(candidate_path);
        }

        if let Some(parent) = search_dir.parent() {
            search_dir = parent;
        } else {
            break;
        }
    }

    // Fallback: Go up 2 levels from deps to get to profile directory
    let profile_dir = match current_exe.parent().and_then(|p| p.parent()) {
        Some(dir) => dir.to_path_buf(),
        None => {
            return Err(format!(
                "Could not determine target directory from executable path: {:?}",
                current_exe
            ));
        }
    };

    let binary_path = profile_dir.join(binary_name);

    let error_msg = format!(
        "helper binary '{}' not found. Searched in: {:?}. \
         Run 'cargo xtask test' or build helpers with 'cargo build --bin {} -p <package>'",
        binary_name, binary_path, binary_name
    );

    if enforce {
        panic!(
            "RAPACE_PREBUILT_HELPERS is set: {}\n\
             To build helpers manually: cargo xtask test --no-run\n\
             Then use: RAPACE_PREBUILT_HELPERS=1 cargo test",
            error_msg
        );
    }

    Err(error_msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enforce_prebuilt_helpers_off_by_default() {
        // Should be false when env var is not set
        std::env::remove_var("RAPACE_PREBUILT_HELPERS");
        assert!(!enforce_prebuilt_helpers());
    }

    #[test]
    fn test_enforce_prebuilt_helpers_true() {
        std::env::set_var("RAPACE_PREBUILT_HELPERS", "true");
        assert!(enforce_prebuilt_helpers());
        std::env::remove_var("RAPACE_PREBUILT_HELPERS");
    }

    #[test]
    fn test_enforce_prebuilt_helpers_1() {
        std::env::set_var("RAPACE_PREBUILT_HELPERS", "1");
        assert!(enforce_prebuilt_helpers());
        std::env::remove_var("RAPACE_PREBUILT_HELPERS");
    }

    #[test]
    fn test_enforce_prebuilt_helpers_false() {
        std::env::set_var("RAPACE_PREBUILT_HELPERS", "false");
        assert!(!enforce_prebuilt_helpers());
        std::env::remove_var("RAPACE_PREBUILT_HELPERS");
    }

    #[test]
    fn test_find_helper_binary_not_found_not_enforced() {
        std::env::remove_var("RAPACE_PREBUILT_HELPERS");
        // Should return an error without panicking
        let result = find_helper_binary("nonexistent-binary");
        assert!(result.is_err());
    }
}
