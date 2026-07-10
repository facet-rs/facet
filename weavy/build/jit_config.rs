// r[impl machine.execution.jit-single-feature]
//
/// Weavy's single build predicate: the `jit` feature AND an explicit OS+arch
/// support matrix. The matrix must match what `copypatch`'s native backend
/// actually supports (`copypatch/src/lib.rs`, `copypatch/src/exec.rs`) — an
/// OS-only check would turn `weavy_jit_active` on for targets copypatch has
/// no `ExecBuf`/relocation-patching support for (e.g. linux-aarch64), which
/// fails to compile.
pub fn jit_active(feature_jit: bool, target_os: &str, target_arch: &str) -> bool {
    feature_jit && native_copy_patch_target(target_os, target_arch)
}

/// The exact OS+arch pairs `copypatch` supports natively.
pub fn native_copy_patch_target(target_os: &str, target_arch: &str) -> bool {
    matches!(
        (target_os, target_arch),
        ("macos", "aarch64") | ("linux", "x86_64")
    )
}
