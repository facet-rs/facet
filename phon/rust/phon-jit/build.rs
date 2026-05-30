//! Build-time stencil extraction.
//!
//! On Apple Silicon, compile the Rust stencils with rustc (`--emit=obj`) and pull
//! each stencil's machine code — and, later, its relocations — out of the object
//! file with the `object` crate, emitting a generated Rust file the crate
//! `include!`s. rustc (LLVM) performs all instruction selection; this script only
//! reads bytes back out.
//!
//! Why a separate object at all, rather than reading the linked binary or a
//! function pointer: copy-and-patch must *patch* each stencil (direct branches,
//! inlined immediates), which needs the relocation table — and relocations exist
//! only in the pre-link object. The linker resolves them away, so the final
//! executable no longer carries the holes.
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
    println!("cargo:rerun-if-changed=stencils/smoke.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let target = env::var("TARGET").expect("TARGET set by cargo");

    let obj = out.join("smoke.o");
    let status = Command::new(rustc)
        .args([
            "--edition",
            "2021",
            "--emit=obj",
            "--crate-type=lib",
            "-O",
            "-C",
            "panic=abort",
            "--target",
            &target,
        ])
        .arg("-o")
        .arg(&obj)
        .arg("stencils/smoke.rs")
        .status()
        .expect("failed to run rustc on stencils");
    assert!(status.success(), "rustc failed to compile stencils");

    let bytes = fs::read(&obj).unwrap();
    let code = extract_text(&bytes);
    assert!(!code.is_empty(), "extracted empty __text");

    let mut src = String::new();
    src.push_str("/// Machine code of `phon_stencil_smoke` (x*3+1), emitted by rustc and\n");
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
