pub fn jit_active(feature_jit: bool, target_os: &str) -> bool {
    feature_jit && !matches!(target_os, "ios" | "tvos" | "watchos" | "visionos")
}
