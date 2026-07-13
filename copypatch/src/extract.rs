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

/// One extracted stencil: its machine code, and the offsets of continuation
/// relocations to patch, relative to the stencil's start.
pub struct Stencil {
    /// The stencil's machine-code bytes.
    pub bytes: Vec<u8>,
    /// Offsets within `bytes` of relocations targeting the continuation symbol.
    pub cont_relocs: Vec<usize>,
}

/// One extracted stencil with relocations grouped by **several** continuation
/// symbols — for a stencil that branches to more than one successor, e.g. a
/// conditional branch whose `then`/`else` are two distinct external tail-calls.
/// (The conditional test itself stays internal to the stencil — a local branch,
/// no relocation — so only the unconditional continuations need patching.)
pub struct StencilN {
    /// The stencil's machine-code bytes.
    pub bytes: Vec<u8>,
    /// `cont_relocs[i]` are the offsets within `bytes` of the relocations
    /// targeting `cont_symbols[i]`, aligned to the `cont_symbols` argument order.
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
        "-C",
        "relocation-model=static",
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

/// Extract one stencil's bytes and continuation relocations from `obj`'s text
/// section by symbol name.
///
/// `all_symbols` is every stencil symbol in the object (used to find where this
/// stencil's code ends — the next symbol's address, or the section end).
/// `cont_symbol` is the external continuation the stencil branches to; only its
/// relocations are reported. Symbol names match with or without a leading
/// underscore (Mach-O's C symbol mangling).
///
/// For a stencil with more than one successor (a conditional branch), use
/// [`extract_stencil_n`].
///
/// # Panics
/// If the object can't be parsed or `symbol` is absent.
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
/// `cont_relocs[i]` are the holes targeting `cont_symbols[i]`, aligned to
/// argument order; a symbol with no relocations in the stencil yields an empty
/// inner vec. See [`extract_stencil`] for `all_symbols`/`symbol` semantics.
///
/// # Panics
/// If the object can't be parsed or `symbol` is absent.
#[must_use]
pub fn extract_stencil_n(
    obj: &[u8],
    all_symbols: &[&str],
    symbol: &str,
    cont_symbols: &[&str],
) -> StencilN {
    use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};

    let file = object::File::parse(obj).expect("parse object file");
    let wanted = |symbol_name: &str, wanted: &str| {
        symbol_name == wanted || symbol_name == format!("_{wanted}")
    };

    let stencil_symbol = file
        .symbols()
        .find(|s| s.name().is_ok_and(|n| wanted(n, symbol)))
        .unwrap_or_else(|| panic!("symbol {symbol} not found"));
    let section_index = stencil_symbol
        .section_index()
        .unwrap_or_else(|| panic!("symbol {symbol} is not in a section"));
    let text = file
        .section_by_index(section_index)
        .expect("read symbol section");
    let data = text.data().expect("read text data");
    let section_addr = text.address();

    let addr_of_in_section = |name: &str| -> Option<u64> {
        file.symbols()
            .find(|s| {
                s.section_index() == Some(section_index) && s.name().is_ok_and(|n| wanted(n, name))
            })
            .map(|s| s.address())
    };

    let mut boundaries: Vec<u64> = all_symbols
        .iter()
        .filter_map(|s| addr_of_in_section(s))
        .collect();
    boundaries.push(section_addr + data.len() as u64);
    boundaries.sort_unstable();

    let start = stencil_symbol.address();
    let end = *boundaries
        .iter()
        .find(|&&b| b > start)
        .expect("a boundary past the stencil");

    let start_offset = start
        .checked_sub(section_addr)
        .expect("symbol starts before its section");
    let end_offset = end
        .checked_sub(section_addr)
        .expect("symbol ends before its section");

    let bytes = data[start_offset as usize..end_offset as usize].to_vec();

    let mut cont_relocs: Vec<Vec<usize>> = vec![Vec::new(); cont_symbols.len()];
    for (offset, reloc) in text.relocations() {
        if offset < start_offset || offset >= end_offset {
            continue;
        }
        let RelocationTarget::Symbol(idx) = reloc.target() else {
            panic!(
                "stencil {symbol} contains unsupported non-symbol relocation at byte offset {}",
                offset - start_offset
            );
        };
        let sym = file
            .symbol_by_index(idx)
            .unwrap_or_else(|_| panic!("stencil {symbol} relocation has invalid symbol index"));
        let name = sym
            .name()
            .unwrap_or_else(|_| panic!("stencil {symbol} relocation target has no name"));

        let mut matched = false;
        for (i, cont) in cont_symbols.iter().enumerate() {
            if name == *cont || name == format!("_{cont}") {
                cont_relocs[i].push((offset - start_offset) as usize);
                matched = true;
            }
        }
        assert!(
            matched,
            "stencil {symbol} contains unsupported relocation to {name} at byte offset {}",
            offset - start_offset
        );
    }
    for relocs in &mut cont_relocs {
        relocs.sort_unstable();
    }

    StencilN { bytes, cont_relocs }
}
