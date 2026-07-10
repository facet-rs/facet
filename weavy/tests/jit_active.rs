//! Focused tests for Weavy's build predicate:
//! `jit_active = feature_enabled && policy_allows && target_supports_copy_patch`,
//! over an explicit OS+arch matrix. The negative `WEAVY_JIT=0` policy is
//! dominant, and the matrix is not an OS-only check: copypatch's native backend
//! is arch-specific too, so e.g. linux-aarch64 or macos-x86_64 must stay off
//! even though their OS alone would pass an OS-only gate.
//!
//! r[verify machine.execution.jit-single-feature]

#[path = "../build/jit_config.rs"]
mod jit_config;

use jit_config::JitPolicy;

#[test]
fn jit_active_on_supported_os_arch_pairs() {
    assert!(jit_config::jit_active(
        true,
        JitPolicy::Allow,
        "macos",
        "aarch64"
    ));
    assert!(jit_config::jit_active(
        true,
        JitPolicy::Allow,
        "linux",
        "x86_64"
    ));
}

#[test]
fn jit_inactive_on_unsupported_arch_even_with_a_supported_os() {
    // Same OS as a supported pair, different arch: an OS-only predicate would
    // wrongly turn this on (this is the exact defect the matrix predicate
    // fixes — copypatch has no ExecBuf/relocation support for these).
    assert!(!jit_config::jit_active(
        true,
        JitPolicy::Allow,
        "macos",
        "x86_64"
    ));
    assert!(!jit_config::jit_active(
        true,
        JitPolicy::Allow,
        "linux",
        "aarch64"
    ));
}

#[test]
fn jit_inactive_on_unsupported_os() {
    for (os, arch) in [
        ("windows", "x86_64"),
        ("ios", "aarch64"),
        ("tvos", "aarch64"),
        ("watchos", "aarch64"),
        ("visionos", "aarch64"),
        ("android", "aarch64"),
        ("freebsd", "x86_64"),
    ] {
        assert!(
            !jit_config::jit_active(true, JitPolicy::Allow, os, arch),
            "{os}-{arch}"
        );
    }
}

#[test]
fn jit_inactive_when_feature_off_even_on_supported_targets() {
    assert!(!jit_config::jit_active(
        false,
        JitPolicy::Allow,
        "macos",
        "aarch64"
    ));
    assert!(!jit_config::jit_active(
        false,
        JitPolicy::Allow,
        "linux",
        "x86_64"
    ));
}

#[test]
fn jit_inactive_when_policy_denies_even_on_supported_targets() {
    assert!(!jit_config::jit_active(
        true,
        JitPolicy::Deny,
        "macos",
        "aarch64"
    ));
    assert!(!jit_config::jit_active(
        true,
        JitPolicy::Deny,
        "linux",
        "x86_64"
    ));
}

#[test]
fn parses_weavy_jit_policy() {
    assert_eq!(jit_config::parse_policy(None), Ok(JitPolicy::Allow));
    assert_eq!(jit_config::parse_policy(Some("1")), Ok(JitPolicy::Allow));
    assert_eq!(jit_config::parse_policy(Some("0")), Ok(JitPolicy::Deny));
    assert!(jit_config::parse_policy(Some("")).is_err());
    assert!(jit_config::parse_policy(Some("false")).is_err());
}

#[test]
fn native_copy_patch_target_matches_copypatch_support_matrix() {
    // Mirrors the `#[cfg(any(all(target_os = "macos", target_arch =
    // "aarch64"), all(target_os = "linux", target_arch = "x86_64")))]` gates
    // in `copypatch/src/lib.rs` and `copypatch/src/exec.rs` exactly — this is
    // the single source of truth those cfgs encode as Rust attributes.
    assert!(jit_config::native_copy_patch_target("macos", "aarch64"));
    assert!(jit_config::native_copy_patch_target("linux", "x86_64"));
    assert!(!jit_config::native_copy_patch_target("macos", "x86_64"));
    assert!(!jit_config::native_copy_patch_target("linux", "aarch64"));
    assert!(!jit_config::native_copy_patch_target("windows", "x86_64"));
}
