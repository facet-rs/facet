//! Build-time stencil extraction.
//!
//! On Apple Silicon, compile the Rust stencils with rustc (`--emit=obj`) and pull
//! each stencil's machine code — and its `phon_cont`/`phon_econt` relocation
//! offsets — out of the object file, emitting a generated Rust file the crate
//! `include!`s. The compile + extraction machinery is the backend-agnostic
//! `copypatch::extract` (shared with other stencil JITs); this script supplies
//! only phon's stencil symbol names and the generated-constant code-gen.
//!
//! Prefer nightly with `--cfg tailcall` so the stencils chain via guaranteed
//! tail calls (`become`); fall back to a stable, call-based compile when nightly
//! is absent (both extract identically). When the tail-call build is used, set
//! `--cfg phon_jit_tailcall` so the crate can report it.
//!
//! On every other target we emit an empty table; the portable threaded executor
//! is the fallback there.

use std::{env, fs, path::Path, path::PathBuf};

use copypatch::extract::{Stencil, compile_object, extract_stencil, nightly_available};

/// Every stencil symbol in the object — `copypatch::extract` needs the full set
/// to find where each stencil's code ends (the next symbol's address).
const SYMBOLS: &[&str] = &[
    "phon_stencil_smoke",
    "phon_stencil_scalar",
    "phon_stencil_sequence",
    "phon_stencil_bytes",
    "phon_stencil_borrow",
    "phon_stencil_option",
    "phon_stencil_result",
    "phon_stencil_pointer",
    "phon_stencil_opaque",
    "phon_stencil_dynamic",
    "phon_stencil_callblock",
    "phon_stencil_set",
    "phon_stencil_map",
    "phon_stencil_enum",
    "phon_stencil_default",
    "phon_stencil_skipwire",
    "phon_stencil_done",
    "phon_stencil_scalar_enc",
    "phon_stencil_sequence_enc",
    "phon_stencil_bytes_enc",
    "phon_stencil_option_enc",
    "phon_stencil_result_enc",
    "phon_stencil_pointer_enc",
    "phon_stencil_opaque_enc",
    "phon_stencil_dynamic_enc",
    "phon_stencil_callblock_enc",
    "phon_stencil_set_enc",
    "phon_stencil_map_enc",
    "phon_stencil_enum_enc",
    "phon_stencil_done_enc",
];

fn main() {
    // Declared here (always) so the cfg is known on every target, even those
    // where it is never set.
    println!("cargo:rustc-check-cfg=cfg(phon_jit_tailcall)");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let generated = out.join("stencils.rs");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    if target_os == "macos" && target_arch == "aarch64" {
        emit_arm64_macos(&out, &generated);
    } else {
        fs::write(
            &generated,
            "pub const SMOKE: &[u8] = &[];\n\
             pub const SCALAR: &[u8] = &[];\n\
             pub const SCALAR_CONT: &[usize] = &[];\n\
             pub const SEQUENCE: &[u8] = &[];\n\
             pub const SEQUENCE_CONT: &[usize] = &[];\n\
             pub const DONE: &[u8] = &[];\n",
        )
        .unwrap();
    }
}

fn emit_arm64_macos(out: &Path, generated: &Path) {
    println!("cargo:rerun-if-changed=stencils/stencils.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-check-cfg=cfg(phon_jit_tailcall)");

    let target = env::var("TARGET").expect("TARGET set by cargo");
    let obj = out.join("stencils.o");
    let src = Path::new("stencils/stencils.rs");

    // Prefer nightly tail-call stencils; fall back to stable call-based ones.
    let tailcall =
        nightly_available() && compile_object("rustc", &["+nightly"], src, &obj, &target, true);
    if tailcall {
        println!("cargo:rustc-cfg=phon_jit_tailcall");
    } else {
        let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
        assert!(
            compile_object(&rustc, &[], src, &obj, &target, false),
            "rustc failed to compile stencils"
        );
    }

    let bytes = fs::read(&obj).unwrap();
    let get = |symbol: &str, cont: &str| extract_stencil(&bytes, SYMBOLS, symbol, cont);
    // Decode stencils continue through `phon_cont`; encode stencils through
    // `phon_econt`. Each is the sole patched relocation in its stencil.
    let smoke = get("phon_stencil_smoke", "phon_cont");
    let scalar = get("phon_stencil_scalar", "phon_cont");
    let sequence = get("phon_stencil_sequence", "phon_cont");
    let bytes_dec = get("phon_stencil_bytes", "phon_cont");
    let borrow = get("phon_stencil_borrow", "phon_cont");
    let option = get("phon_stencil_option", "phon_cont");
    let result = get("phon_stencil_result", "phon_cont");
    let pointer = get("phon_stencil_pointer", "phon_cont");
    let opaque = get("phon_stencil_opaque", "phon_cont");
    let dynamic = get("phon_stencil_dynamic", "phon_cont");
    let callblock = get("phon_stencil_callblock", "phon_cont");
    let set_dec = get("phon_stencil_set", "phon_cont");
    let map_dec = get("phon_stencil_map", "phon_cont");
    let enum_dec = get("phon_stencil_enum", "phon_cont");
    let default = get("phon_stencil_default", "phon_cont");
    let skipwire = get("phon_stencil_skipwire", "phon_cont");
    let done = get("phon_stencil_done", "phon_cont");
    let scalar_enc = get("phon_stencil_scalar_enc", "phon_econt");
    let sequence_enc = get("phon_stencil_sequence_enc", "phon_econt");
    let bytes_enc = get("phon_stencil_bytes_enc", "phon_econt");
    let option_enc = get("phon_stencil_option_enc", "phon_econt");
    let result_enc = get("phon_stencil_result_enc", "phon_econt");
    let pointer_enc = get("phon_stencil_pointer_enc", "phon_econt");
    let opaque_enc = get("phon_stencil_opaque_enc", "phon_econt");
    let dynamic_enc = get("phon_stencil_dynamic_enc", "phon_econt");
    let callblock_enc = get("phon_stencil_callblock_enc", "phon_econt");
    let set_enc = get("phon_stencil_set_enc", "phon_econt");
    let map_enc = get("phon_stencil_map_enc", "phon_econt");
    let enum_enc = get("phon_stencil_enum_enc", "phon_econt");
    let done_enc = get("phon_stencil_done_enc", "phon_econt");

    let mode = if tailcall {
        "tail-call (nightly become)"
    } else {
        "call (stable)"
    };
    let mut s = String::new();
    s.push_str(&format!(
        "// Generated by build.rs from stencils/stencils.rs (rustc --emit=obj, {mode}).\n"
    ));
    emit(
        &mut s,
        "SMOKE",
        "`phon_stencil_smoke` (x*3+1) machine code; no relocations.",
        &smoke,
    );
    emit(
        &mut s,
        "SCALAR",
        "`phon_stencil_scalar`: decode one fixed scalar, continue.",
        &scalar,
    );
    emit_cont(&mut s, "SCALAR_CONT", "SCALAR", &scalar);
    emit(
        &mut s,
        "SEQUENCE",
        "`phon_stencil_sequence`: decode one owned sequence; element body via `SeqInfo.element_entry`.",
        &sequence,
    );
    emit_cont(&mut s, "SEQUENCE_CONT", "SEQUENCE", &sequence);
    emit(
        &mut s,
        "BYTES",
        "`phon_stencil_bytes`: decode one bulk byte run (non-UTF-8); one inline word-wise block copy.",
        &bytes_dec,
    );
    emit_cont(&mut s, "BYTES_CONT", "BYTES", &bytes_dec);
    emit(
        &mut s,
        "BORROW",
        "`phon_stencil_borrow`: decode one borrowed, zero-copy byte run (`&str`/`&[u8]`); fat pointer INTO the input via `BorrowInfo.set_borrowed`.",
        &borrow,
    );
    emit_cont(&mut s, "BORROW_CONT", "BORROW", &borrow);
    emit(
        &mut s,
        "OPTION",
        "`phon_stencil_option`: decode one `Option<T>` (presence branch); some-body via `OptInfo.some_entry`.",
        &option,
    );
    emit_cont(&mut s, "OPTION_CONT", "OPTION", &option);
    emit(
        &mut s,
        "RESULT",
        "`phon_stencil_result`: decode one `Result<T, E>` (Ok/Err branch); arm bodies via `ResultInfo` entries.",
        &result,
    );
    emit_cont(&mut s, "RESULT_CONT", "RESULT", &result);
    emit(
        &mut s,
        "POINTER",
        "`phon_stencil_pointer`: decode one owned pointer; pointee body via `PointerInfo.pointee_entry`.",
        &pointer,
    );
    emit_cont(&mut s, "POINTER_CONT", "POINTER", &pointer);
    emit(
        &mut s,
        "OPAQUE",
        "`phon_stencil_opaque`: decode one opaque adapter field (length-prefixed bytes); thunk builds value.",
        &opaque,
    );
    emit_cont(&mut s, "OPAQUE_CONT", "OPAQUE", &opaque);
    emit(
        &mut s,
        "DYNAMIC",
        "`phon_stencil_dynamic`: decode one self-describing `Value`; helper preserves exact decode errors.",
        &dynamic,
    );
    emit_cont(&mut s, "DYNAMIC_CONT", "DYNAMIC", &dynamic);
    emit(
        &mut s,
        "CALLBLOCK",
        "`phon_stencil_callblock`: call a precompiled recursive decode block.",
        &callblock,
    );
    emit_cont(&mut s, "CALLBLOCK_CONT", "CALLBLOCK", &callblock);
    emit(
        &mut s,
        "SET",
        "`phon_stencil_set`: decode one owned set (count loop, element sub-chain); element body via `SetInfo.element_entry`.",
        &set_dec,
    );
    emit_cont(&mut s, "SET_CONT", "SET", &set_dec);
    emit(
        &mut s,
        "MAP",
        "`phon_stencil_map`: decode one owned map (count loop, key+value sub-chains); key/value bodies via `MapInfo.key_entry`/`value_entry`.",
        &map_dec,
    );
    emit_cont(&mut s, "MAP_CONT", "MAP", &map_dec);
    emit(
        &mut s,
        "ENUM",
        "`phon_stencil_enum`: decode one `#[repr(int)]` enum (variant branch); payload via `EnumVariantInfo.payload_entry`.",
        &enum_dec,
    );
    emit_cont(&mut s, "ENUM_CONT", "ENUM", &enum_dec);
    emit(
        &mut s,
        "DEFAULT",
        "`phon_stencil_default`: write a reader-only field's default (no wire); thunk via `DefaultInfo.thunk`.",
        &default,
    );
    emit_cont(&mut s, "DEFAULT_CONT", "DEFAULT", &default);
    emit(
        &mut s,
        "SKIPWIRE",
        "`phon_stencil_skipwire`: consume a writer-only value's wire bytes; walker via `SkipInfo.walk`.",
        &skipwire,
    );
    emit_cont(&mut s, "SKIPWIRE_CONT", "SKIPWIRE", &skipwire);
    emit(&mut s, "DONE", "`phon_stencil_done`: a lone `ret`.", &done);
    emit(
        &mut s,
        "SCALAR_ENC",
        "`phon_stencil_scalar_enc`: encode one fixed scalar, continue.",
        &scalar_enc,
    );
    emit_cont(&mut s, "SCALAR_ENC_CONT", "SCALAR_ENC", &scalar_enc);
    emit(
        &mut s,
        "SEQUENCE_ENC",
        "`phon_stencil_sequence_enc`: encode one owned sequence; element body via `EncSeqInfo.element_entry`.",
        &sequence_enc,
    );
    emit_cont(&mut s, "SEQUENCE_ENC_CONT", "SEQUENCE_ENC", &sequence_enc);
    emit(
        &mut s,
        "BYTES_ENC",
        "`phon_stencil_bytes_enc`: encode one bulk byte run (non-UTF-8); one inline word-wise block copy.",
        &bytes_enc,
    );
    emit_cont(&mut s, "BYTES_ENC_CONT", "BYTES_ENC", &bytes_enc);
    emit(
        &mut s,
        "OPTION_ENC",
        "`phon_stencil_option_enc`: encode one `Option<T>` (presence branch); some-body via `EncOptInfo.some_entry`.",
        &option_enc,
    );
    emit_cont(&mut s, "OPTION_ENC_CONT", "OPTION_ENC", &option_enc);
    emit(
        &mut s,
        "RESULT_ENC",
        "`phon_stencil_result_enc`: encode one `Result<T, E>` (Ok/Err branch); arm bodies via `EncResultInfo` entries.",
        &result_enc,
    );
    emit_cont(&mut s, "RESULT_ENC_CONT", "RESULT_ENC", &result_enc);
    emit(
        &mut s,
        "POINTER_ENC",
        "`phon_stencil_pointer_enc`: encode one owned pointer; pointee body via `EncPointerInfo.pointee_entry`.",
        &pointer_enc,
    );
    emit_cont(&mut s, "POINTER_ENC_CONT", "POINTER_ENC", &pointer_enc);
    emit(
        &mut s,
        "OPAQUE_ENC",
        "`phon_stencil_opaque_enc`: encode one opaque adapter field (length-prefixed bytes); thunk appends inner bytes.",
        &opaque_enc,
    );
    emit_cont(&mut s, "OPAQUE_ENC_CONT", "OPAQUE_ENC", &opaque_enc);
    emit(
        &mut s,
        "DYNAMIC_ENC",
        "`phon_stencil_dynamic_enc`: encode one self-describing `Value`; helper appends bytes.",
        &dynamic_enc,
    );
    emit_cont(&mut s, "DYNAMIC_ENC_CONT", "DYNAMIC_ENC", &dynamic_enc);
    emit(
        &mut s,
        "CALLBLOCK_ENC",
        "`phon_stencil_callblock_enc`: call a precompiled recursive encode block.",
        &callblock_enc,
    );
    emit_cont(
        &mut s,
        "CALLBLOCK_ENC_CONT",
        "CALLBLOCK_ENC",
        &callblock_enc,
    );
    emit(
        &mut s,
        "SET_ENC",
        "`phon_stencil_set_enc`: encode one owned set (count + iterator loop, element sub-chain); element body via `EncSetInfo.element_entry`.",
        &set_enc,
    );
    emit_cont(&mut s, "SET_ENC_CONT", "SET_ENC", &set_enc);
    emit(
        &mut s,
        "MAP_ENC",
        "`phon_stencil_map_enc`: encode one owned map (count + iterator loop, key+value sub-chains); key/value bodies via `EncMapInfo.key_entry`/`value_entry`.",
        &map_enc,
    );
    emit_cont(&mut s, "MAP_ENC_CONT", "MAP_ENC", &map_enc);
    emit(
        &mut s,
        "ENUM_ENC",
        "`phon_stencil_enum_enc`: encode one `#[repr(int)]` enum (variant branch); payload via `EncEnumVariantInfo.payload_entry`.",
        &enum_enc,
    );
    emit_cont(&mut s, "ENUM_ENC_CONT", "ENUM_ENC", &enum_enc);
    emit(
        &mut s,
        "DONE_ENC",
        "`phon_stencil_done_enc`: a lone `ret`.",
        &done_enc,
    );

    fs::write(generated, s).unwrap();
}

/// Emit a `pub const NAME: &[u8] = &[..];` for a stencil's machine code.
fn emit(out: &mut String, name: &str, doc: &str, s: &Stencil) {
    out.push_str(&format!(
        "/// {doc}\npub const {name}: &[u8] = &{:?};\n",
        s.bytes
    ));
}

/// Emit a `pub const NAME: &[usize] = &[..];` of a stencil's continuation-reloc offsets.
fn emit_cont(out: &mut String, name: &str, of: &str, s: &Stencil) {
    out.push_str(&format!(
        "/// Byte offsets within `{of}` of the continuation `BRANCH26` relocations to patch.\n\
         pub const {name}: &[usize] = &{:?};\n",
        s.cont_relocs
    ));
}
