// r[impl machine.execution.jit-single-feature]
//
/// Weavy's negative JIT build policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JitPolicy {
    /// Permit Weavy's Cargo feature and target matrix to decide.
    Allow,
    /// Force JIT inactive for this Cargo invocation.
    Deny,
}

impl JitPolicy {
    pub const fn allows(self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Parse `WEAVY_JIT`.
///
/// Unset and `1` allow Weavy's feature/matrix decision. `0` denies it.
/// Anything else is rejected by the build script so a typo cannot silently
/// change execution mode.
pub fn parse_policy(value: Option<&str>) -> Result<JitPolicy, String> {
    match value {
        None | Some("1") => Ok(JitPolicy::Allow),
        Some("0") => Ok(JitPolicy::Deny),
        Some(other) => Err(format!("WEAVY_JIT must be unset, 1, or 0; got {other:?}")),
    }
}

/// Weavy's single build predicate: the `jit` feature, Weavy's negative policy,
/// and an explicit OS+arch support matrix. The matrix must match what
/// `copypatch`'s native backend actually supports (`copypatch/src/lib.rs`,
/// `copypatch/src/exec.rs`) — an OS-only check would turn `weavy_jit_active`
/// on for targets copypatch has no `ExecBuf`/relocation-patching support for
/// (e.g. linux-aarch64), which fails to compile.
pub fn jit_active(
    feature_jit: bool,
    policy: JitPolicy,
    target_os: &str,
    target_arch: &str,
) -> bool {
    feature_jit && policy.allows() && native_copy_patch_target(target_os, target_arch)
}

/// The exact OS+arch pairs `copypatch` supports natively.
pub fn native_copy_patch_target(target_os: &str, target_arch: &str) -> bool {
    matches!(
        (target_os, target_arch),
        ("macos", "aarch64") | ("linux", "x86_64")
    )
}

#[cfg(test)]
mod tests {
    use super::{JitPolicy, jit_active, native_copy_patch_target, parse_policy};

    #[test]
    fn parses_negative_policy_override() {
        assert_eq!(parse_policy(None), Ok(JitPolicy::Allow));
        assert_eq!(parse_policy(Some("1")), Ok(JitPolicy::Allow));
        assert_eq!(parse_policy(Some("0")), Ok(JitPolicy::Deny));

        let err = parse_policy(Some("")).unwrap_err();
        assert!(err.contains("WEAVY_JIT"));

        let err = parse_policy(Some("false")).unwrap_err();
        assert!(err.contains("WEAVY_JIT"));
    }

    #[test]
    fn exact_native_copy_patch_matrix() {
        assert!(native_copy_patch_target("macos", "aarch64"));
        assert!(native_copy_patch_target("linux", "x86_64"));

        assert!(!native_copy_patch_target("macos", "x86_64"));
        assert!(!native_copy_patch_target("linux", "aarch64"));
        assert!(!native_copy_patch_target("ios", "aarch64"));
    }

    #[test]
    fn jit_active_requires_feature_policy_and_target() {
        assert!(jit_active(true, JitPolicy::Allow, "macos", "aarch64"));
        assert!(!jit_active(false, JitPolicy::Allow, "macos", "aarch64"));
        assert!(!jit_active(true, JitPolicy::Deny, "macos", "aarch64"));
        assert!(!jit_active(true, JitPolicy::Allow, "linux", "aarch64"));
    }
}
