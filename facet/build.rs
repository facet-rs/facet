fn main() {
    // Emit a cfg flag when portable_simd is available (nightly only for now).
    println!("cargo::rustc-check-cfg=cfg(has_portable_simd)");

    let ac = autocfg::new();
    // Probe for portable_simd feature support (requires nightly).
    if ac
        .probe_raw(
            r#"
            #![feature(portable_simd)]
            fn _test() { let _: core::simd::Simd<f32, 4>; }
            "#,
        )
        .is_ok()
    {
        println!("cargo::rustc-cfg=has_portable_simd");
    }

    println!("cargo::rerun-if-changed=build.rs");
}
