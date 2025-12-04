fn main() {
    // Emit a cfg flag when we're on nightly, so we can combine it with
    // the "nightly" Cargo feature to conditionally enable unstable features.
    println!("cargo::rustc-check-cfg=cfg(nightly)");
    if rustversion::cfg!(nightly) {
        println!("cargo::rustc-cfg=nightly");
    }
}
