//! Build-time stencil extraction.
//!
//! On Apple Silicon, compile the C stencils with clang and pull each stencil's
//! machine code (and, later, its relocations) out of the object file with the
//! `object` crate, emitting a generated Rust file the crate `include!`s. clang
//! performs all instruction selection; this script only reads bytes back out.
//!
//! On every other target we emit an empty table — the portable threaded executor
//! is the fallback there.

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let generated = out.join("stencils.rs");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    if target_os == "macos" && target_arch == "aarch64" {
        emit_arm64_macos(&out, &generated);
    } else {
        fs::write(&generated, "pub const SMOKE: &[u8] = &[];\n").unwrap();
    }
}

fn emit_arm64_macos(out: &std::path::Path, generated: &std::path::Path) {
    println!("cargo:rerun-if-changed=stencils/smoke.c");
    println!("cargo:rerun-if-changed=build.rs");

    let obj = out.join("smoke.o");
    let status = Command::new("clang")
        .args(["-O2", "-c", "-fomit-frame-pointer", "-fno-stack-protector"])
        .arg("-o")
        .arg(&obj)
        .arg("stencils/smoke.c")
        .status()
        .expect("failed to run clang");
    assert!(status.success(), "clang failed to compile stencils");

    let bytes = fs::read(&obj).unwrap();
    let code = extract_text(&bytes);
    assert!(!code.is_empty(), "extracted empty __text");

    let mut src = String::new();
    src.push_str("/// Machine code of `phon_stencil_smoke` (x*3+1), emitted by clang and\n");
    src.push_str("/// extracted from its object file. Self-contained: no relocations.\n");
    src.push_str(&format!("pub const SMOKE: &[u8] = &{code:?};\n"));
    fs::write(generated, src).unwrap();
}

/// Return the bytes of the `__text` section — for a single-function object that
/// is exactly the compiled stencil.
fn extract_text(obj: &[u8]) -> Vec<u8> {
    use object::{Object, ObjectSection};
    let file = object::File::parse(obj).expect("parse object file");
    for section in file.sections() {
        if section.name() == Ok("__text") {
            return section.data().expect("read __text data").to_vec();
        }
    }
    panic!("no __text section in object");
}
