fn main() {
    // Tell Cargo that if the given file changes, to rerun this build script.
    println!("cargo:rerun-if-changed=src");

    // Use the `cc` crate to build `example.c` and statically link it.
    cc::Build::new().file("src/example.c").compile("example");
}
