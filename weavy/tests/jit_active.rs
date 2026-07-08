#[path = "../build/jit_config.rs"]
mod jit_config;

#[test]
fn jit_feature_is_suppressed_on_wx_locked_apple_targets() {
    for target_os in ["ios", "tvos", "watchos", "visionos"] {
        assert!(!jit_config::jit_active(true, target_os), "{target_os}");
    }
}

#[test]
fn jit_feature_controls_non_locked_targets() {
    assert!(jit_config::jit_active(true, "macos"));
    assert!(jit_config::jit_active(true, "linux"));
    assert!(!jit_config::jit_active(false, "macos"));
}
