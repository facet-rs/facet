//! GDB/LLDB JIT interface + profiler artifacts for Weavy JIT'd code.
//!
//! Three ways out of "anonymous executable buffer", from highest- to lowest-level tooling:
//!
//! - [`register_jit_source`] — the facade. Builds an in-memory ELF (`.symtab` per region +
//!   DWARF `.debug_line` mapping code ranges to source line:column) and registers it through
//!   the GDB JIT interface (`__jit_debug_descriptor` + `__jit_debug_register_code`), so
//!   debuggers resolve JIT'd PCs to source, take source/column breakpoints, and backtrace
//!   through JIT frames. Keep the returned [`JitRegistration`] alive while the code can run;
//!   dropping it unregisters. Also appends `/tmp/perf-<pid>.map`.
//! - [`write_jitdump`] — a perf **jitdump** (`/tmp/jit-<pid>.dump`, one `JIT_CODE_LOAD` per
//!   region with the actual code bytes), which `perf` and stax tail to symbolicate and
//!   per-instruction-annotate JIT'd code. Timestamps use wall-clock nanoseconds, not
//!   `CLOCK_MONOTONIC` as the spec strictly wants — fine for consumers that don't order
//!   samples against load records (stax), before-the-samples loads, or same-clock setups.
//! - [`register_jit_code`]/[`register_jit_code_with_dwarf`] — the underlying ELF builder for
//!   callers that construct their own [`jit_dwarf` sections](super::dwarf).
//!
//! Reference: <https://sourceware.org/gdb/current/onlinedocs/gdb.html/JIT-Interface.html>
//!
//! LLDB notes (macOS)
//! ------------------
//! LLDB keeps the GDB JIT loader disabled by default on macOS. Enable it first:
//!
//! `settings set plugin.jit-loader.gdb.enable on`
//!
//! Then source breakpoints on the registered file bind when the JIT registers
//! (`b template.jinja:4`, or by column: `breakpoint set -f template.jinja -l 1 -u 8`), and
//! `image lookup -a <pc>` resolves a crashing JIT PC to its symbol + source line. Set
//! `WEAVY_JIT_DUMP_ELF_DIR=<dir>` to dump each registered ELF for offline inspection
//! (`dwarfdump --debug-line <dir>/*.elf`).
//!
//! Provenance: salvaged from bearcove/kajit (scrapped) and adopted here; `allow(dead_code)`
//! because consumers use a subset of the salvaged API.
#![allow(dead_code)]

use std::io::Write;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// GDB JIT interface types
// ---------------------------------------------------------------------------

const JIT_NOACTION: u32 = 0;
const JIT_REGISTER_FN: u32 = 1;
const JIT_UNREGISTER_FN: u32 = 2;

#[repr(C)]
struct JitCodeEntry {
    next: *mut JitCodeEntry,
    prev: *mut JitCodeEntry,
    symfile_addr: *const u8,
    symfile_size: u64,
}

#[repr(C)]
struct JitDescriptor {
    version: u32,
    action_flag: u32,
    relevant_entry: *mut JitCodeEntry,
    first_entry: *mut JitCodeEntry,
}

// SAFETY: The linked list is protected by DESCRIPTOR_LOCK.
unsafe impl Send for JitDescriptor {}
unsafe impl Sync for JitDescriptor {}

#[unsafe(no_mangle)]
static mut __jit_debug_descriptor: JitDescriptor = JitDescriptor {
    version: 1,
    action_flag: JIT_NOACTION,
    relevant_entry: std::ptr::null_mut(),
    first_entry: std::ptr::null_mut(),
};

#[unsafe(no_mangle)]
#[inline(never)]
extern "C" fn __jit_debug_register_code() {
    // GDB sets a breakpoint here. The body must not be optimized away.
    unsafe { std::ptr::read_volatile(&0u8) };
}

static DESCRIPTOR_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A symbol entry for the JIT symbol table.
pub struct JitSymbolEntry {
    pub name: String,
    pub offset: usize,
    pub size: usize,
}

/// Produce a Rust v0 mangled symbol from path segments.
///
/// Example:
/// `rust_v0_mangle(&["kajit", "decode", "Bools"])` -> `"_RNvNtC5kajit6decode5Bools"`.
///
/// The first segment is the crate root. Middle segments use type namespace
/// (`Nt`). The last segment uses value namespace (`Nv`) since it is the
/// callable item.
pub(crate) fn rust_v0_mangle(segments: &[&str]) -> String {
    assert!(segments.len() >= 2, "need at least crate + item");

    let mut mangled = String::from("_R");

    for index in (1..segments.len()).rev() {
        if index == segments.len() - 1 {
            mangled.push_str("Nv");
        } else {
            mangled.push_str("Nt");
        }
    }

    mangled.push('C');
    let crate_name = segments[0];
    mangled.push_str(&format!("{}{}", crate_name.len(), crate_name));

    for segment in &segments[1..] {
        mangled.push_str(&format!("{}{}", segment.len(), segment));
    }

    mangled
}

/// Owns the GDB JIT registration. Unregisters on drop.
pub struct JitRegistration {
    entry: *mut JitCodeEntry,
    _elf: Vec<u8>,
}

// SAFETY: The JitCodeEntry is heap-allocated and only accessed under DESCRIPTOR_LOCK.
unsafe impl Send for JitRegistration {}
unsafe impl Sync for JitRegistration {}

impl Drop for JitRegistration {
    fn drop(&mut self) {
        let _lock = DESCRIPTOR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let entry = &mut *self.entry;

            // Unlink from the doubly-linked list.
            if !entry.prev.is_null() {
                (*entry.prev).next = entry.next;
            } else {
                __jit_debug_descriptor.first_entry = entry.next;
            }
            if !entry.next.is_null() {
                (*entry.next).prev = entry.prev;
            }

            __jit_debug_descriptor.action_flag = JIT_UNREGISTER_FN;
            __jit_debug_descriptor.relevant_entry = self.entry;
            __jit_debug_register_code();

            // Free the entry.
            drop(Box::from_raw(self.entry));
        }
    }
}

/// Register JIT-compiled code with the debugger.
///
/// `buf_base` is the start of the executable buffer, `buf_len` its length.
/// `symbols` contains (name, offset, size) for each function in the buffer.
///
/// Returns a `JitRegistration` that keeps the registration alive.
pub fn register_jit_code(
    buf_base: *const u8,
    buf_len: usize,
    symbols: &[JitSymbolEntry],
) -> JitRegistration {
    register_jit_code_with_dwarf(buf_base, buf_len, symbols, None)
}

/// Register JIT-compiled code with optional DWARF sections.
///
/// Existing call sites can keep using `register_jit_code`; this is the
/// preparatory API for attaching `.debug_line` payloads later.
pub fn register_jit_code_with_dwarf(
    buf_base: *const u8,
    buf_len: usize,
    symbols: &[JitSymbolEntry],
    dwarf: Option<&super::dwarf::JitDwarfSections>,
) -> JitRegistration {
    let elf = build_elf(buf_base as u64, buf_len, symbols, dwarf);
    maybe_dump_jit_elf(&elf, symbols);

    let entry = Box::into_raw(Box::new(JitCodeEntry {
        next: std::ptr::null_mut(),
        prev: std::ptr::null_mut(),
        symfile_addr: elf.as_ptr(),
        symfile_size: elf.len() as u64,
    }));

    let _lock = DESCRIPTOR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        // Prepend to linked list.
        let old_first = __jit_debug_descriptor.first_entry;
        (*entry).next = old_first;
        if !old_first.is_null() {
            (*old_first).prev = entry;
        }
        __jit_debug_descriptor.first_entry = entry;
        __jit_debug_descriptor.action_flag = JIT_REGISTER_FN;
        __jit_debug_descriptor.relevant_entry = entry;
        __jit_debug_register_code();
    }

    write_perf_map(buf_base, symbols);

    JitRegistration { entry, _elf: elf }
}

// ---------------------------------------------------------------------------
// High-level facade: one call to make JIT'd code debuggable + profilable
// ---------------------------------------------------------------------------

/// A JIT'd code region for the debugger/profiler: symbol `name`, byte `offset`+`size` in the code
/// buffer, and a 1-based source `line` (+ optional 1-based `column`, 0 = none) in the source file.
/// Columns let ONE source line carry many JIT regions — e.g. each sub-expression of a template
/// line — so the REAL source file (not a synthetic listing) can be the debug source. This is all a
/// weavy JIT consumer needs for lldb/gdb source-stepping + perf/stax symbolication.
pub struct JitSourceSymbol {
    pub name: String,
    pub offset: usize,
    pub size: usize,
    pub line: u32,
    pub column: u32,
}

/// Register JIT'd code with the debugger (GDB/LLDB JIT interface) with per-symbol source lines, so
/// a debugger resolves JIT'd PCs to `file_name:line` and can source-step. `file_name`+`directory`
/// must point at a real listing file for source display. Also writes `/tmp/perf-<pid>.map`. Keep
/// the returned [`JitRegistration`] alive while the code can run.
pub fn register_jit_source(
    code_ptr: *const u8,
    code_len: usize,
    file_name: &str,
    directory: Option<&str>,
    symbols: &[JitSourceSymbol],
) -> Result<JitRegistration, super::dwarf::DwarfPrepError> {
    // The DWARF line program requires strictly increasing code offsets; callers emit stencils
    // in whatever order suits them, so sort here instead of making it their problem.
    let mut symbols: Vec<&JitSourceSymbol> = symbols.iter().collect();
    symbols.sort_by_key(|s| s.offset);

    let entries: Vec<JitSymbolEntry> = symbols
        .iter()
        .map(|s| JitSymbolEntry { name: s.name.clone(), offset: s.offset, size: s.size })
        .collect();
    // `build_jit_dwarf_sections` maps (offset, line_index, column) -> line = line_index + 1.
    let source_map: Vec<(u32, u32, u32)> = symbols
        .iter()
        .map(|s| (s.offset as u32, s.line.saturating_sub(1), s.column))
        .collect();
    let dwarf = super::dwarf::build_jit_dwarf_sections(
        code_ptr as u64,
        code_len as u64,
        &source_map,
        file_name,
        directory,
    )?;
    Ok(register_jit_code_with_dwarf(code_ptr, code_len, &entries, Some(&dwarf)))
}

/// Write a perf **jitdump** (`/tmp/jit-<pid>.dump`) so `perf`/stax symbolicate + annotate JIT'd
/// code. One `JIT_CODE_LOAD` per symbol: `name` + the symbol's actual runtime bytes read from
/// `code_ptr + offset`. (perf jitdump format: 40-byte header magic `0x4A695444`, then records.)
///
/// # Safety
/// Every symbol's `code_ptr + offset .. + offset + size` range must be valid readable memory
/// (normally guaranteed by pointing at a live [`super::NativeProgram`]'s code buffer with
/// offsets/sizes from its layout).
pub unsafe fn write_jitdump(path: &str, code_ptr: *const u8, symbols: &[JitSourceSymbol]) -> std::io::Result<()> {
    let pid = std::process::id();
    let ts = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    };
    let mut out = Vec::new();
    // Header (40 bytes): magic, version, header_size, elf_mach, pad, pid, ts, flags.
    out.extend_from_slice(&0x4A69_5444u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&ELF_MACHINE_JITDUMP.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&pid.to_le_bytes());
    out.extend_from_slice(&ts().to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    for (i, s) in symbols.iter().enumerate() {
        let addr = code_ptr as u64 + s.offset as u64;
        let code = unsafe { std::slice::from_raw_parts(code_ptr.add(s.offset), s.size) };
        let mut payload = Vec::new();
        payload.extend_from_slice(&pid.to_le_bytes());
        payload.extend_from_slice(&pid.to_le_bytes());
        payload.extend_from_slice(&addr.to_le_bytes()); // vma (stax uses this)
        payload.extend_from_slice(&addr.to_le_bytes()); // code_addr
        payload.extend_from_slice(&(s.size as u64).to_le_bytes());
        payload.extend_from_slice(&(i as u64).to_le_bytes());
        payload.extend_from_slice(s.name.as_bytes());
        payload.push(0);
        payload.extend_from_slice(code);
        let total = 16 + payload.len();
        out.extend_from_slice(&0u32.to_le_bytes()); // id = JIT_CODE_LOAD
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&ts().to_le_bytes());
        out.extend_from_slice(&payload);
    }
    std::fs::write(path, &out)
}

#[cfg(target_arch = "aarch64")]
const ELF_MACHINE_JITDUMP: u32 = 183; // EM_AARCH64
#[cfg(target_arch = "x86_64")]
const ELF_MACHINE_JITDUMP: u32 = 62; // EM_X86_64
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
const ELF_MACHINE_JITDUMP: u32 = 0;

fn maybe_dump_jit_elf(elf: &[u8], symbols: &[JitSymbolEntry]) {
    let Ok(dir) = std::env::var("WEAVY_JIT_DUMP_ELF_DIR") else {
        return;
    };
    let path = std::path::Path::new(&dir);
    if std::fs::create_dir_all(path).is_err() {
        return;
    }
    let stem = symbols
        .first()
        .map(|s| {
            s.name
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .unwrap_or_else(|| "jit".to_string());
    let filename = format!("{stem}__pid{}__{}.elf", std::process::id(), elf.len());
    let _ = std::fs::write(path.join(filename), elf);
}

// ---------------------------------------------------------------------------
// perf map file — /tmp/perf-<pid>.map
// ---------------------------------------------------------------------------

fn write_perf_map(buf_base: *const u8, symbols: &[JitSymbolEntry]) {
    let path = format!("/tmp/perf-{}.map", std::process::id());
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    for sym in symbols {
        let addr = buf_base as usize + sym.offset;
        let _ = writeln!(f, "{addr:x} {:x} {}", sym.size, sym.name);
    }
}

// ---------------------------------------------------------------------------
// Minimal ELF64 builder
// ---------------------------------------------------------------------------

// ELF constants
const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 0x1;
const PF_R: u32 = 0x4;
const SHT_NULL: u32 = 0;
const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHF_ALLOC: u64 = 0x2;
const SHF_EXECINSTR: u64 = 0x4;
const STB_GLOBAL: u8 = 1;
const STT_FUNC: u8 = 2;

const EHDR_SIZE: usize = 64;
const PHDR_SIZE: usize = 56;
const SHDR_SIZE: usize = 64;
const SYM_SIZE: usize = 24;

#[cfg(target_arch = "x86_64")]
const EM_MACHINE: u16 = 0x3E; // EM_X86_64

#[cfg(target_arch = "aarch64")]
const EM_MACHINE: u16 = 0xB7; // EM_AARCH64

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const EM_MACHINE: u16 = 0;

struct ExtraSection<'a> {
    name: &'a str,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
    data: &'a [u8],
}

fn build_elf(
    text_addr: u64,
    text_len: usize,
    symbols: &[JitSymbolEntry],
    dwarf: Option<&super::dwarf::JitDwarfSections>,
) -> Vec<u8> {
    let entry_addr = symbols
        .iter()
        .map(|sym| text_addr + sym.offset as u64)
        .min()
        .unwrap_or(text_addr);

    // Build .strtab (symbol name strings)
    let mut strtab = vec![0u8]; // index 0 = empty string
    let mut name_offsets = Vec::with_capacity(symbols.len());
    for sym in symbols {
        name_offsets.push(strtab.len() as u32);
        strtab.extend_from_slice(sym.name.as_bytes());
        strtab.push(0);
    }

    // Build .symtab
    // Entry 0: null symbol
    let num_syms = 1 + symbols.len();
    let symtab_size = num_syms * SYM_SIZE;
    let mut symtab = Vec::with_capacity(symtab_size);
    // Null symbol (24 zero bytes)
    symtab.extend_from_slice(&[0u8; SYM_SIZE]);
    for (i, sym) in symbols.iter().enumerate() {
        // st_name (u32)
        symtab.extend_from_slice(&name_offsets[i].to_le_bytes());
        // st_info (u8): binding=STB_GLOBAL, type=STT_FUNC
        symtab.push((STB_GLOBAL << 4) | STT_FUNC);
        // st_other (u8)
        symtab.push(0);
        // st_shndx (u16): section index 1 = .text
        symtab.extend_from_slice(&1u16.to_le_bytes());
        // st_value (u64): absolute address
        let addr = text_addr + sym.offset as u64;
        symtab.extend_from_slice(&addr.to_le_bytes());
        // st_size (u64)
        symtab.extend_from_slice(&(sym.size as u64).to_le_bytes());
    }

    let mut extras = Vec::<ExtraSection<'_>>::new();
    if let Some(dwarf) = dwarf {
        if !dwarf.debug_line.is_empty() {
            extras.push(ExtraSection {
                name: ".debug_line",
                sh_type: SHT_PROGBITS,
                sh_flags: 0,
                sh_addr: 0,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
                data: &dwarf.debug_line,
            });
        }
        if !dwarf.debug_abbrev.is_empty() {
            extras.push(ExtraSection {
                name: ".debug_abbrev",
                sh_type: SHT_PROGBITS,
                sh_flags: 0,
                sh_addr: 0,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
                data: &dwarf.debug_abbrev,
            });
        }
        if !dwarf.debug_info.is_empty() {
            extras.push(ExtraSection {
                name: ".debug_info",
                sh_type: SHT_PROGBITS,
                sh_flags: 0,
                sh_addr: 0,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
                data: &dwarf.debug_info,
            });
        }
        if !dwarf.debug_loc.is_empty() {
            extras.push(ExtraSection {
                name: ".debug_loc",
                sh_type: SHT_PROGBITS,
                sh_flags: 0,
                sh_addr: 0,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
                data: &dwarf.debug_loc,
            });
        }
        if !dwarf.debug_ranges.is_empty() {
            extras.push(ExtraSection {
                name: ".debug_ranges",
                sh_type: SHT_PROGBITS,
                sh_flags: 0,
                sh_addr: 0,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
                data: &dwarf.debug_ranges,
            });
        }
    }

    // Build .shstrtab (section name strings)
    let mut shstrtab = vec![0u8];
    let sh_name_null = 0u32;
    let sh_name_text = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".text\0");
    let sh_name_symtab = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".symtab\0");
    let sh_name_strtab = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".strtab\0");
    let mut extra_name_offsets = Vec::with_capacity(extras.len());
    for extra in &extras {
        let off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(extra.name.as_bytes());
        shstrtab.push(0);
        extra_name_offsets.push(off);
    }
    let sh_name_shstrtab = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".shstrtab\0");

    // Layout: ELF header | program headers | section headers | section data blobs
    let num_program_headers = 1usize;
    let phdr_offset = EHDR_SIZE;
    let num_sections = 5 + extras.len(); // null, .text, .symtab, .strtab, extras..., .shstrtab
    let shstrtab_index = 4 + extras.len();
    let shdr_offset = EHDR_SIZE + num_program_headers * PHDR_SIZE;
    let data_offset = shdr_offset + num_sections * SHDR_SIZE;
    let symtab_off = data_offset;
    let strtab_off = symtab_off + symtab.len();
    let mut extra_offsets = Vec::with_capacity(extras.len());
    let mut cursor = strtab_off + strtab.len();
    for extra in &extras {
        extra_offsets.push(cursor);
        cursor += extra.data.len();
    }
    let shstrtab_off = cursor;
    let total_size = shstrtab_off + shstrtab.len();

    let mut elf = Vec::with_capacity(total_size);

    // ----- ELF header (64 bytes) -----
    elf.extend_from_slice(&ELFMAG); // e_ident[0..4]
    elf.push(ELFCLASS64); // e_ident[4]
    elf.push(ELFDATA2LSB); // e_ident[5]
    elf.push(EV_CURRENT); // e_ident[6]
    elf.extend_from_slice(&[0u8; 9]); // e_ident[7..16] padding
    elf.extend_from_slice(&ET_EXEC.to_le_bytes()); // e_type
    elf.extend_from_slice(&EM_MACHINE.to_le_bytes()); // e_machine
    elf.extend_from_slice(&1u32.to_le_bytes()); // e_version
    elf.extend_from_slice(&entry_addr.to_le_bytes()); // e_entry
    elf.extend_from_slice(&(phdr_offset as u64).to_le_bytes()); // e_phoff
    elf.extend_from_slice(&(shdr_offset as u64).to_le_bytes()); // e_shoff
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&(EHDR_SIZE as u16).to_le_bytes()); // e_ehsize
    elf.extend_from_slice(&(PHDR_SIZE as u16).to_le_bytes()); // e_phentsize
    elf.extend_from_slice(&(num_program_headers as u16).to_le_bytes()); // e_phnum
    elf.extend_from_slice(&(SHDR_SIZE as u16).to_le_bytes()); // e_shentsize
    elf.extend_from_slice(&(num_sections as u16).to_le_bytes()); // e_shnum
    elf.extend_from_slice(&(shstrtab_index as u16).to_le_bytes()); // e_shstrndx (index of .shstrtab)
    debug_assert_eq!(elf.len(), EHDR_SIZE);

    // ----- Program headers -----
    // [0] PT_LOAD covering runtime .text memory.
    write_phdr(
        &mut elf,
        PT_LOAD,
        PF_R | PF_X,
        0, // no backing bytes in this ELF for .text
        text_addr,
        text_addr,
        0,
        text_len as u64,
        16,
    );

    // ----- Section headers -----

    // [0] SHT_NULL
    write_shdr(&mut elf, sh_name_null, SHT_NULL, 0, 0, 0, 0, 0, 0, 0, 0);

    // [1] .text — points at the JIT buffer in memory (no data in ELF)
    write_shdr(
        &mut elf,
        sh_name_text,
        SHT_PROGBITS,
        SHF_ALLOC | SHF_EXECINSTR,
        text_addr,
        0, // sh_offset: no data in file
        text_len as u64,
        0,
        0,
        16,
        0,
    );

    // [2] .symtab
    write_shdr(
        &mut elf,
        sh_name_symtab,
        SHT_SYMTAB,
        0,
        0,
        symtab_off as u64,
        symtab.len() as u64,
        3, // sh_link = .strtab section index
        1, // sh_info = index of first non-local symbol
        8,
        SYM_SIZE as u64,
    );

    // [3] .strtab
    write_shdr(
        &mut elf,
        sh_name_strtab,
        SHT_STRTAB,
        0,
        0,
        strtab_off as u64,
        strtab.len() as u64,
        0,
        0,
        1,
        0,
    );

    for (index, extra) in extras.iter().enumerate() {
        write_shdr(
            &mut elf,
            extra_name_offsets[index],
            extra.sh_type,
            extra.sh_flags,
            extra.sh_addr,
            extra_offsets[index] as u64,
            extra.data.len() as u64,
            extra.sh_link,
            extra.sh_info,
            extra.sh_addralign,
            extra.sh_entsize,
        );
    }

    // [4] .shstrtab
    write_shdr(
        &mut elf,
        sh_name_shstrtab,
        SHT_STRTAB,
        0,
        0,
        shstrtab_off as u64,
        shstrtab.len() as u64,
        0,
        0,
        1,
        0,
    );

    debug_assert_eq!(elf.len(), data_offset);

    // ----- Section data -----
    elf.extend_from_slice(&symtab);
    elf.extend_from_slice(&strtab);
    for extra in &extras {
        elf.extend_from_slice(extra.data);
    }
    elf.extend_from_slice(&shstrtab);

    debug_assert_eq!(elf.len(), total_size);
    elf
}

#[allow(clippy::too_many_arguments)]
fn write_shdr(
    buf: &mut Vec<u8>,
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
) {
    buf.extend_from_slice(&sh_name.to_le_bytes());
    buf.extend_from_slice(&sh_type.to_le_bytes());
    buf.extend_from_slice(&sh_flags.to_le_bytes());
    buf.extend_from_slice(&sh_addr.to_le_bytes());
    buf.extend_from_slice(&sh_offset.to_le_bytes());
    buf.extend_from_slice(&sh_size.to_le_bytes());
    buf.extend_from_slice(&sh_link.to_le_bytes());
    buf.extend_from_slice(&sh_info.to_le_bytes());
    buf.extend_from_slice(&sh_addralign.to_le_bytes());
    buf.extend_from_slice(&sh_entsize.to_le_bytes());
}

#[allow(clippy::too_many_arguments)]
fn write_phdr(
    buf: &mut Vec<u8>,
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
) {
    buf.extend_from_slice(&p_type.to_le_bytes());
    buf.extend_from_slice(&p_flags.to_le_bytes());
    buf.extend_from_slice(&p_offset.to_le_bytes());
    buf.extend_from_slice(&p_vaddr.to_le_bytes());
    buf.extend_from_slice(&p_paddr.to_le_bytes());
    buf.extend_from_slice(&p_filesz.to_le_bytes());
    buf.extend_from_slice(&p_memsz.to_le_bytes());
    buf.extend_from_slice(&p_align.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn read_u64(bytes: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }

    fn read_section_names(elf: &[u8]) -> Vec<String> {
        let shoff = read_u64(elf, 40) as usize;
        let shentsize = read_u16(elf, 58) as usize;
        let shnum = read_u16(elf, 60) as usize;
        let shstrndx = read_u16(elf, 62) as usize;

        let shstr_off = read_u64(elf, shoff + shstrndx * shentsize + 24) as usize;
        let shstr_size = read_u64(elf, shoff + shstrndx * shentsize + 32) as usize;
        let shstr = &elf[shstr_off..shstr_off + shstr_size];

        (0..shnum)
            .map(|index| {
                let sh_name = read_u32(elf, shoff + index * shentsize) as usize;
                if sh_name == 0 {
                    return String::new();
                }
                let tail = &shstr[sh_name..];
                let end = tail.iter().position(|b| *b == 0).unwrap();
                String::from_utf8(tail[..end].to_vec()).unwrap()
            })
            .collect()
    }

    fn read_program_header(elf: &[u8], index: usize) -> [u8; PHDR_SIZE] {
        let phoff = read_u64(elf, 32) as usize;
        let phentsize = read_u16(elf, 54) as usize;
        assert_eq!(phentsize, PHDR_SIZE);
        let start = phoff + index * phentsize;
        let mut out = [0u8; PHDR_SIZE];
        out.copy_from_slice(&elf[start..start + PHDR_SIZE]);
        out
    }

    #[test]
    fn elf_without_dwarf_sections_keeps_base_layout() {
        let elf = build_elf(
            0x1000,
            32,
            &[JitSymbolEntry {
                name: "jit::root".to_string(),
                offset: 0,
                size: 32,
            }],
            None,
        );
        let names = read_section_names(&elf);
        assert!(names.contains(&".text".to_string()));
        assert!(names.contains(&".symtab".to_string()));
        assert!(names.contains(&".strtab".to_string()));
        assert!(!names.contains(&".debug_line".to_string()));

        assert_eq!(read_u64(&elf, 32), EHDR_SIZE as u64); // e_phoff
        assert_eq!(read_u16(&elf, 54), PHDR_SIZE as u16); // e_phentsize
        assert_eq!(read_u16(&elf, 56), 1); // e_phnum
        assert_eq!(read_u64(&elf, 24), 0x1000); // e_entry

        let ph = read_program_header(&elf, 0);
        assert_eq!(u32::from_le_bytes(ph[0..4].try_into().unwrap()), PT_LOAD);
        assert_eq!(
            u32::from_le_bytes(ph[4..8].try_into().unwrap()),
            PF_R | PF_X
        );
        assert_eq!(u64::from_le_bytes(ph[8..16].try_into().unwrap()), 0); // p_offset
        assert_eq!(u64::from_le_bytes(ph[16..24].try_into().unwrap()), 0x1000); // p_vaddr
        assert_eq!(u64::from_le_bytes(ph[24..32].try_into().unwrap()), 0x1000); // p_paddr
        assert_eq!(u64::from_le_bytes(ph[32..40].try_into().unwrap()), 0); // p_filesz
        assert_eq!(u64::from_le_bytes(ph[40..48].try_into().unwrap()), 32); // p_memsz
        assert_eq!(u64::from_le_bytes(ph[48..56].try_into().unwrap()), 16); // p_align
    }

    #[test]
    fn elf_with_dwarf_sections_contains_debug_line() {
        let dwarf = crate::jit::dwarf::build_jit_dwarf_sections(
            0x2000,
            16,
            &[(0, 0, 0), (4, 1, 0)],
            "decoder.ra",
            Some("jit"),
        )
        .unwrap();

        let elf = build_elf(
            0x2000,
            16,
            &[JitSymbolEntry {
                name: "jit::root".to_string(),
                offset: 0,
                size: 16,
            }],
            Some(&dwarf),
        );

        let names = read_section_names(&elf);
        assert!(names.contains(&".debug_line".to_string()));
        assert!(names.contains(&".debug_abbrev".to_string()));
        assert!(names.contains(&".debug_info".to_string()));
    }

    #[test]
    fn rust_v0_mangle_basic() {
        assert_eq!(
            rust_v0_mangle(&["kajit", "decode", "Bools"]),
            "_RNvNtC5kajit6decode5Bools"
        );
        assert_eq!(rust_v0_mangle(&["kajit", "decode"]), "_RNvC5kajit6decode");
    }

    #[test]
    fn rust_v0_mangle_has_v0_prefix_and_wrappers() {
        let mangled = rust_v0_mangle(&["kajit", "decode", "ra_mir_text"]);
        assert!(mangled.starts_with("_R"));
        assert!(mangled.contains("Nv"));
        assert!(mangled.contains("C5kajit"));
    }

    #[test]
    fn register_jit_source_accepts_unsorted_symbols_and_unregisters_on_drop() {
        // A stand-in "code buffer" — registration never executes it.
        let code = vec![0u8; 24];
        // Deliberately UNSORTED by offset: the facade must sort before building .debug_line
        // (which requires strictly increasing offsets).
        let symbols = vec![
            JitSourceSymbol { name: "jit::b".into(), offset: 8, size: 8, line: 1, column: 9 },
            JitSourceSymbol { name: "jit::a".into(), offset: 0, size: 8, line: 1, column: 4 },
            JitSourceSymbol { name: "jit::c".into(), offset: 16, size: 8, line: 2, column: 0 },
        ];
        let reg = register_jit_source(code.as_ptr(), code.len(), "t.jinja", None, &symbols)
            .expect("register with unsorted symbols");

        // The registration is linked into the GDB JIT descriptor while alive...
        unsafe {
            let first = __jit_debug_descriptor.first_entry;
            assert!(!first.is_null());
            assert_eq!((*first).symfile_size as usize, reg._elf.len());
        }
        drop(reg);
        // ...and unlinked once dropped (nextest = one process per test, no cross-talk).
        unsafe {
            assert!(__jit_debug_descriptor.first_entry.is_null());
        }
    }

    #[test]
    fn write_jitdump_round_trips_records() {
        let code: Vec<u8> = (0u8..32).collect();
        let symbols = vec![
            JitSourceSymbol { name: "jit::op0 [1 + 2]".into(), offset: 0, size: 16, line: 1, column: 4 },
            JitSourceSymbol { name: "jit::op1 [3]".into(), offset: 16, size: 16, line: 1, column: 8 },
        ];
        let dir = std::env::temp_dir();
        let path = dir.join(format!("weavy-jitdump-test-{}.dump", std::process::id()));
        let path = path.to_str().unwrap();
        // SAFETY: offsets/sizes lie within `code`, which outlives the call.
        unsafe { write_jitdump(path, code.as_ptr(), &symbols) }.expect("write jitdump");

        // Re-parse per the perf jitdump spec (and stax's tailer): 40-byte header with the
        // "JiTD" magic, then JIT_CODE_LOAD records: id/total_size/timestamp + payload of
        // pid/tid/vma/code_addr/code_size/code_index + name\0 + code bytes.
        let bytes = std::fs::read(path).unwrap();
        std::fs::remove_file(path).ok();
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0x4A69_5444);
        let mut cur = 40;
        let mut seen = Vec::new();
        while cur + 16 <= bytes.len() {
            let id = u32::from_le_bytes(bytes[cur..cur + 4].try_into().unwrap());
            let total = u32::from_le_bytes(bytes[cur + 4..cur + 8].try_into().unwrap()) as usize;
            assert_eq!(id, 0, "JIT_CODE_LOAD");
            let p = &bytes[cur + 16..cur + total];
            let vma = u64::from_le_bytes(p[8..16].try_into().unwrap());
            let size = u64::from_le_bytes(p[24..32].try_into().unwrap());
            let nul = p[40..].iter().position(|&b| b == 0).unwrap();
            let name = String::from_utf8_lossy(&p[40..40 + nul]).into_owned();
            let code_bytes = &p[40 + nul + 1..40 + nul + 1 + size as usize];
            seen.push((vma, size, name, code_bytes.to_vec()));
            cur += total;
        }
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].0, code.as_ptr() as u64);
        assert_eq!(seen[0].2, "jit::op0 [1 + 2]");
        assert_eq!(seen[0].3, &code[0..16], "record carries the actual code bytes");
        assert_eq!(seen[1].0, code.as_ptr() as u64 + 16);
        assert_eq!(seen[1].3, &code[16..32]);
    }
}
