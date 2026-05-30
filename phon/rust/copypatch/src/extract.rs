//! Build-time stencil extraction (enabled by the `build` feature).
//!
//! Compile a stencil source file with rustc (`--emit=obj`) and read each
//! stencil's machine code — and the offsets of its continuation relocations — back
//! out of the object file with the `object` crate. rustc (LLVM) performs all
//! instruction selection; this only reads bytes and relocation offsets.
//!
//! Why a pre-link object: copy-and-patch must *patch* the continuation branch,
//! which needs the relocation table — present in the object, gone after linking.
//!
//! Intended for use from a `build.rs`, with copypatch as a `[build-dependencies]`
//! entry carrying `features = ["build"]`.

use std::path::Path;
use std::process::Command;

/// One extracted stencil: its machine code, and the offsets (relative to the
/// stencil's start) of the `BRANCH26` continuation relocations to patch.
pub struct Stencil {
    /// The stencil's machine-code bytes (a slice of `__text`).
    pub bytes: Vec<u8>,
    /// Offsets within `bytes` of `BRANCH26` relocations targeting the
    /// continuation symbol; the holes the JIT patches with [`patch_branch26`].
    ///
    /// [`patch_branch26`]: crate::patch_branch26
    pub cont_relocs: Vec<usize>,
}

/// One extracted stencil with relocations grouped by **several** continuation
/// symbols — for a stencil that branches to more than one successor, e.g. a
/// conditional branch whose `then`/`else` are two distinct external tail-calls.
/// (The conditional test itself stays internal to the stencil — a local branch,
/// no relocation — so only the unconditional continuations need patching.)
pub struct StencilN {
    /// The stencil's machine-code bytes (a slice of `__text`).
    pub bytes: Vec<u8>,
    /// `cont_relocs[i]` are the offsets within `bytes` of the `BRANCH26`
    /// relocations targeting `cont_symbols[i]` (the holes the JIT patches with
    /// [`patch_branch26`](crate::patch_branch26)), aligned to the `cont_symbols`
    /// argument order.
    pub cont_relocs: Vec<Vec<usize>>,
}

/// Whether a `+nightly` rustc toolchain is available (for tail-call stencils).
#[must_use]
pub fn nightly_available() -> bool {
    Command::new("rustc")
        .args(["+nightly", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Compile the stencil source at `src` to the object file `obj` for `target`.
///
/// `program` is the rustc to invoke (e.g. `"rustc"`); `pre_args` are inserted
/// before the flags (e.g. `["+nightly"]`). With `tailcall`, passes
/// `--cfg tailcall` so stencils can chain via guaranteed tail calls. Returns
/// whether compilation succeeded; output is captured so a clean build stays quiet
/// and a failed nightly attempt can fall back silently. Stderr is surfaced only
/// for a stable (non-`pre_args`) attempt — the final one.
#[must_use]
pub fn compile_object(
    program: &str,
    pre_args: &[&str],
    src: &Path,
    obj: &Path,
    target: &str,
    tailcall: bool,
) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(pre_args);
    cmd.args([
        "--edition",
        "2021",
        "--emit=obj",
        "--crate-type=lib",
        "-O",
        "-C",
        "panic=abort",
        "--target",
        target,
    ]);
    if tailcall {
        cmd.args(["--cfg", "tailcall"]);
    }
    cmd.arg("-o").arg(obj).arg(src);
    match cmd.output() {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            if pre_args.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&o.stderr));
            }
            false
        }
        Err(_) => false,
    }
}

/// Extract one stencil's bytes and continuation relocations from `obj`'s `__text`
/// section by symbol name.
///
/// `all_symbols` is every stencil symbol in the object (used to find where this
/// stencil's code ends — the next symbol's address, or the section end).
/// `cont_symbol` is the external continuation the stencil branches to; only its
/// `BRANCH26` relocations are reported (the holes the JIT patches). Symbol names
/// match with or without a leading underscore (Mach-O's C symbol mangling).
///
/// For a stencil with more than one successor (a conditional branch), use
/// [`extract_stencil_n`].
///
/// # Panics
/// If the object can't be parsed, has no `__text`, or `symbol` is absent.
#[must_use]
pub fn extract_stencil(
    obj: &[u8],
    all_symbols: &[&str],
    symbol: &str,
    cont_symbol: &str,
) -> Stencil {
    let mut s = extract_stencil_n(obj, all_symbols, symbol, &[cont_symbol]);
    Stencil {
        bytes: s.bytes,
        cont_relocs: s.cont_relocs.pop().expect("one cont symbol requested"),
    }
}

/// Extract one stencil's bytes and its continuation relocations grouped by
/// **several** continuation symbols (e.g. a conditional branch's `then`/`else`).
/// `cont_relocs[i]` are the `BRANCH26` holes targeting `cont_symbols[i]`, aligned
/// to argument order; a symbol with no relocations in the stencil yields an empty
/// inner vec. See [`extract_stencil`] for `all_symbols`/`symbol` semantics.
///
/// # Panics
/// If the object can't be parsed, has no `__text`, or `symbol` is absent.
#[must_use]
pub fn extract_stencil_n(
    obj: &[u8],
    all_symbols: &[&str],
    symbol: &str,
    cont_symbols: &[&str],
) -> StencilN {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};

    let file = object::File::parse(obj).expect("parse object file");
    let text = file
        .sections()
        .find(|s| s.name() == Ok("__text"))
        .expect("no __text section");
    let data = text.data().expect("read __text data");
    let text_index = text.index();

    let addr_of = |name: &str| -> u64 {
        file.symbols()
            .find(|s| {
                s.section_index() == Some(text_index)
                    && s.name().is_ok_and(|n| n == name || n == format!("_{name}"))
            })
            .unwrap_or_else(|| panic!("symbol {name} not found"))
            .address()
    };

    let mut boundaries: Vec<u64> = all_symbols.iter().map(|s| addr_of(s)).collect();
    boundaries.push(data.len() as u64);
    boundaries.sort_unstable();

    let start = addr_of(symbol);
    let end = *boundaries
        .iter()
        .find(|&&b| b > start)
        .expect("a boundary past the stencil");

    let bytes = data[start as usize..end as usize].to_vec();

    let mut cont_relocs: Vec<Vec<usize>> = vec![Vec::new(); cont_symbols.len()];
    for (offset, reloc) in text.relocations() {
        if offset < start || offset >= end {
            continue;
        }
        if let RelocationTarget::Symbol(idx) = reloc.target()
            && let Ok(sym) = file.symbol_by_index(idx)
            && let Ok(name) = sym.name()
        {
            for (i, cont) in cont_symbols.iter().enumerate() {
                if name == *cont || name == format!("_{cont}") {
                    cont_relocs[i].push((offset - start) as usize);
                }
            }
        }
    }
    for relocs in &mut cont_relocs {
        relocs.sort_unstable();
    }

    StencilN { bytes, cont_relocs }
}
