//! DWARF preparation utilities for JIT code.
//!
//! This module has two layers:
//! - `JitDebugInfo`, a structured debug-info model owned by the compiler.
//! - a DWARF v4 serializer that lowers that model to ELF sections.
//!
//! SALVAGED from bearcove/kajit (scrapped): hand-rolled DWARF v4 (.debug_line PC->source line,
//! abbrev, info). We use the `build_jit_dwarf_sections` line-mapping subset for the stencil JIT.
//! Style lints allowed as vendored code (we call a subset; the shape is kajit's).
#![allow(dead_code, clippy::too_many_arguments, clippy::enum_variant_names)]

/// A relocation needed in a DWARF section: an 8-byte absolute address
/// at the given offset that should point to (text_symbol + addend).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfRelocation {
    /// Byte offset within the DWARF section.
    pub offset: u32,
    /// Addend (offset from the start of the text section).
    pub addend: i64,
}

/// Which DWARF section a relocation belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DwarfSection {
    DebugInfo,
    DebugLine,
    DebugAranges,
}

/// Owned DWARF sections ready to be attached to the JIT ELF.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JitDwarfSections {
    pub debug_line: Vec<u8>,
    pub debug_abbrev: Vec<u8>,
    pub debug_info: Vec<u8>,
    pub debug_aranges: Vec<u8>,
    pub debug_loc: Vec<u8>,
    pub debug_ranges: Vec<u8>,
    /// Relocations needed for standalone binaries (code_address=0).
    /// Each entry: (section, relocation).
    pub relocations: Vec<(DwarfSection, DwarfRelocation)>,
}

impl JitDwarfSections {
    pub fn is_empty(&self) -> bool {
        self.debug_line.is_empty()
            && self.debug_abbrev.is_empty()
            && self.debug_info.is_empty()
            && self.debug_loc.is_empty()
            && self.debug_ranges.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitDebugLineRow {
    pub code_offset: u32,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfLocationRange {
    pub start: u64,
    pub end: u64,
    pub expression: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwarfVariableLocation {
    Expr(Vec<u8>),
    List(Vec<DwarfLocationRange>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfVariable {
    pub name: String,
    pub location: DwarfVariableLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitDebugRange {
    pub low_pc: u64,
    pub high_pc: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitDebugLexicalBlock {
    pub ranges: Vec<JitDebugRange>,
    pub variables: Vec<DwarfVariable>,
    pub lexical_blocks: Vec<JitDebugLexicalBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DwarfTargetArch {
    X86_64,
    Aarch64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitDebugLineTable {
    pub file_name: String,
    pub directory: Option<String>,
    pub rows: Vec<JitDebugLineRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitDebugSubprogram {
    pub name: String,
    pub frame_base_expression: Vec<u8>,
    pub variables: Vec<DwarfVariable>,
    pub lexical_blocks: Vec<JitDebugLexicalBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitDebugInfo {
    pub target_arch: DwarfTargetArch,
    pub code_address: u64,
    pub code_size: u64,
    pub line_table: JitDebugLineTable,
    pub subprogram: JitDebugSubprogram,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwarfPrepError {
    InteriorNul {
        field: &'static str,
    },
    UnsupportedTargetArch,
    SourceMapNotStrictlyIncreasing {
        previous_offset: u32,
        next_offset: u32,
    },
    SourceOffsetOutOfBounds {
        offset: u32,
        code_size: u64,
    },
    CodeSizeOutOfBounds {
        code_size: u64,
    },
}

const DW_LNS_COPY: u8 = 1;
const DW_LNS_ADVANCE_PC: u8 = 2;
const DW_LNS_ADVANCE_LINE: u8 = 3;

const DW_LNE_END_SEQUENCE: u8 = 1;
const DW_LNE_SET_ADDRESS: u8 = 2;

const LINE_VERSION: u16 = 4;
const MIN_INSN_LEN: u8 = 1;
const MAX_OPS_PER_INSN: u8 = 1;
const DEFAULT_IS_STMT: u8 = 1;
const LINE_BASE: i8 = -5;
const LINE_RANGE: u8 = 14;
const OPCODE_BASE: u8 = 13;
const STANDARD_OPCODE_LENGTHS: [u8; 12] = [0, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1];

const DW_TAG_COMPILE_UNIT: u8 = 0x11;
const DW_TAG_BASE_TYPE: u8 = 0x24;
const DW_TAG_LEXICAL_BLOCK: u8 = 0x0b;
const DW_TAG_SUBPROGRAM: u8 = 0x2e;
const DW_TAG_VARIABLE: u8 = 0x34;
const DW_CHILDREN_YES: u8 = 0x01;
const DW_CHILDREN_NO: u8 = 0x00;
const DW_AT_LOCATION: u8 = 0x02;
const DW_AT_NAME: u8 = 0x03;
const DW_AT_STMT_LIST: u8 = 0x10;
const DW_AT_LOW_PC: u8 = 0x11;
const DW_AT_HIGH_PC: u8 = 0x12;
const DW_AT_BYTE_SIZE: u8 = 0x0b;
const DW_AT_FRAME_BASE: u8 = 0x40;
const DW_AT_RANGES: u8 = 0x55;
const DW_AT_TYPE: u8 = 0x49;
const DW_AT_ENCODING: u8 = 0x3e;
const DW_FORM_ADDR: u8 = 0x01;
const DW_FORM_DATA8: u8 = 0x07;
const DW_FORM_STRING: u8 = 0x08;
const DW_FORM_DATA1: u8 = 0x0b;
const DW_FORM_REF4: u8 = 0x13;
const DW_FORM_SEC_OFFSET: u8 = 0x17;
const DW_FORM_EXPRLOC: u8 = 0x18;
const DW_ATE_UNSIGNED: u8 = 0x07;
const INFO_VERSION: u16 = 4;
const ADDRESS_SIZE_64: u8 = 8;
const DW_OP_REG0: u8 = 0x50;
const DW_OP_BREG0: u8 = 0x70;
const DW_OP_REGX: u8 = 0x90;
const DW_OP_FBREG: u8 = 0x91;
const DW_OP_BREGX: u8 = 0x92;
const DW_OP_DEREF_SIZE: u8 = 0x94;
const DW_OP_STACK_VALUE: u8 = 0x9f;
const DW_OP_PLUS_UCONST: u8 = 0x23;

/// Build DWARF sections for one JIT function.
///
/// `source_map` entries are interpreted as:
/// - address: `code_address + code_offset`
/// - line: `ra_mir_inst_index + 1`
pub fn build_jit_dwarf_sections(
    code_address: u64,
    code_size: u64,
    source_map: &[(u32, u32)],
    file_name: &str,
    directory: Option<&str>,
) -> Result<JitDwarfSections, DwarfPrepError> {
    let target_arch = if cfg!(target_arch = "x86_64") {
        DwarfTargetArch::X86_64
    } else if cfg!(target_arch = "aarch64") {
        DwarfTargetArch::Aarch64
    } else {
        return Err(DwarfPrepError::UnsupportedTargetArch);
    };
    build_jit_dwarf_sections_with_variables(
        target_arch,
        code_address,
        code_size,
        source_map,
        file_name,
        directory,
        file_name,
        &[],
    )
}

pub fn build_jit_dwarf_sections_with_variables(
    target_arch: DwarfTargetArch,
    code_address: u64,
    code_size: u64,
    source_map: &[(u32, u32)],
    file_name: &str,
    directory: Option<&str>,
    subprogram_name: &str,
    variables: &[DwarfVariable],
) -> Result<JitDwarfSections, DwarfPrepError> {
    if file_name.as_bytes().contains(&0) {
        return Err(DwarfPrepError::InteriorNul { field: "file_name" });
    }
    if let Some(dir) = directory
        && dir.as_bytes().contains(&0)
    {
        return Err(DwarfPrepError::InteriorNul { field: "directory" });
    }
    if subprogram_name.as_bytes().contains(&0) {
        return Err(DwarfPrepError::InteriorNul {
            field: "subprogram_name",
        });
    }
    for variable in variables {
        if variable.name.as_bytes().contains(&0) {
            return Err(DwarfPrepError::InteriorNul {
                field: "variable_name",
            });
        }
    }

    let debug_info = JitDebugInfo {
        target_arch,
        code_address,
        code_size,
        line_table: JitDebugLineTable {
            file_name: file_name.to_owned(),
            directory: directory.map(ToOwned::to_owned),
            rows: source_map
                .iter()
                .map(|(code_offset, ra_mir_inst_index)| JitDebugLineRow {
                    code_offset: *code_offset,
                    line: ra_mir_inst_index.saturating_add(1),
                })
                .collect(),
        },
        subprogram: JitDebugSubprogram {
            name: subprogram_name.to_owned(),
            frame_base_expression: expr_breg(frame_base_register(target_arch), 0),
            variables: variables.to_vec(),
            lexical_blocks: Vec::new(),
        },
    };
    build_jit_dwarf_sections_from_debug_info(&debug_info)
}

pub fn build_jit_dwarf_sections_from_debug_info(
    debug_info: &JitDebugInfo,
) -> Result<JitDwarfSections, DwarfPrepError> {
    if debug_info.line_table.file_name.as_bytes().contains(&0) {
        return Err(DwarfPrepError::InteriorNul { field: "file_name" });
    }
    if let Some(dir) = &debug_info.line_table.directory
        && dir.as_bytes().contains(&0)
    {
        return Err(DwarfPrepError::InteriorNul { field: "directory" });
    }
    if debug_info.subprogram.name.as_bytes().contains(&0) {
        return Err(DwarfPrepError::InteriorNul {
            field: "subprogram_name",
        });
    }
    for variable in subprogram_variables_in_preorder(&debug_info.subprogram) {
        if variable.name.as_bytes().contains(&0) {
            return Err(DwarfPrepError::InteriorNul {
                field: "variable_name",
            });
        }
    }

    let (debug_loc, variable_loc_offsets) = build_debug_loc_section_from_debug_info(debug_info);
    let (debug_ranges, lexical_block_range_offsets) =
        build_debug_ranges_section_from_debug_info(debug_info);
    let debug_line = build_debug_line_section_from_debug_info(debug_info)?;
    let debug_info_section = build_debug_info_section_from_debug_info(
        debug_info,
        &variable_loc_offsets,
        &lexical_block_range_offsets,
    );

    // Compute relocation offsets for standalone binary use.
    // These are the byte positions of code_address fields that need
    // relocation when code_address=0 (i.e., for object files where the
    // linker resolves addresses).
    let relocations = if debug_info.code_address == 0 {
        compute_dwarf_relocations(debug_info, &debug_info_section, &debug_line)
    } else {
        Vec::new()
    };

    // Build .debug_aranges: maps address ranges to compile units
    let (debug_aranges, aranges_relocs) =
        build_debug_aranges(debug_info.code_address, debug_info.code_size);

    let mut relocations = relocations;
    relocations.extend(aranges_relocs);

    Ok(JitDwarfSections {
        debug_line,
        debug_abbrev: build_debug_abbrev_section(),
        debug_info: debug_info_section,
        debug_aranges,
        debug_loc,
        debug_ranges,
        relocations,
    })
}

/// Build .debug_aranges section.
///
/// Maps one address range (the entire CU) to CU offset 0 in .debug_info.
/// Format: DWARF32 header + one (address, length) pair + terminator.
fn build_debug_aranges(
    code_address: u64,
    code_size: u64,
) -> (Vec<u8>, Vec<(DwarfSection, DwarfRelocation)>) {
    let mut out = Vec::new();
    let mut relocs = Vec::new();

    // Header (fixed size for DWARF32 + 8-byte addresses)
    // length: will fill in at the end
    let length_pos = out.len();
    out.extend_from_slice(&0u32.to_le_bytes()); // unit_length (placeholder)
    out.extend_from_slice(&2u16.to_le_bytes()); // version
    out.extend_from_slice(&0u32.to_le_bytes()); // debug_info_offset (CU at offset 0)
    out.push(8); // address_size
    out.push(0); // segment_selector_size

    // Pad to 2*address_size alignment (16 bytes from start of header)
    // Header so far: 4 + 2 + 4 + 1 + 1 = 12 bytes. Need to align to 16.
    out.extend_from_slice(&0u32.to_le_bytes()); // 4 bytes padding → 16

    // One address range entry: (address, length)
    let addr_offset = out.len();
    out.extend_from_slice(&code_address.to_le_bytes()); // start address
    out.extend_from_slice(&code_size.to_le_bytes()); // length

    // Terminator: (0, 0)
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());

    // Fill in unit_length (everything after the initial 4-byte length field)
    let unit_length = (out.len() - 4) as u32;
    out[length_pos..length_pos + 4].copy_from_slice(&unit_length.to_le_bytes());

    // Add relocation for the address field (if code_address=0)
    if code_address == 0 {
        relocs.push((
            DwarfSection::DebugAranges,
            DwarfRelocation {
                offset: addr_offset as u32,
                addend: 0,
            },
        ));
    }

    (out, relocs)
}

/// Compute DWARF relocation offsets for a standalone binary.
///
/// When code_address=0, these offsets mark where 8-byte absolute addresses
/// appear in the DWARF sections. The linker needs to add the actual text
/// section address at these positions.
fn compute_dwarf_relocations(
    debug_info: &JitDebugInfo,
    _debug_info_section: &[u8],
    debug_line_section: &[u8],
) -> Vec<(DwarfSection, DwarfRelocation)> {
    let mut relocs = Vec::new();

    // debug_info section layout:
    //   4 bytes: unit_length
    //   2 bytes: version
    //   4 bytes: debug_abbrev_offset
    //   1 byte:  address_size
    //   --- DIE content starts here (offset 11) ---
    //   1 byte:  abbrev index (ULEB128 = 1)
    //   4 bytes: DW_AT_stmt_list
    //   8 bytes: DW_AT_low_pc  ← RELOCATION #1 at offset 16
    //   8 bytes: DW_AT_high_pc (length, not address — no reloc needed)
    //   N bytes: DW_AT_name (null-terminated string)
    //   ... base type DIE ...
    //   1 byte:  abbrev index (ULEB128 = 3, subprogram)
    //   N bytes: DW_AT_name (subprogram name, null-terminated)
    //   8 bytes: DW_AT_low_pc  ← RELOCATION #2

    // CU DW_AT_low_pc: always at offset 16
    relocs.push((
        DwarfSection::DebugInfo,
        DwarfRelocation {
            offset: 16,
            addend: 0,
        },
    ));

    // Subprogram DW_AT_low_pc: need to find its offset.
    // After the CU header (11 bytes) + DIE content:
    //   1 (abbrev) + 4 (stmt_list) + 8 (low_pc) + 8 (high_pc) = 21 bytes
    //   Then file_name string + null terminator
    //   Then base_type DIE: 1 (abbrev=2) + "u64\0" (4) + 1 (encoding) + 1 (byte_size) = 7
    //   Then subprogram DIE: 1 (abbrev=3) + name + null
    //   Then 8 bytes DW_AT_low_pc ← this is what we want
    let file_name_len = debug_info.line_table.file_name.len() + 1; // +1 for null
    let subprogram_name_len = debug_info.subprogram.name.len() + 1;

    // Offset within the DIE content (after the 11-byte CU header):
    //   1 + 4 + 8 + 8 + file_name_len + 7 (base type) + 1 + subprogram_name_len
    let subprogram_low_pc_die_offset = 1 + 4 + 8 + 8 + file_name_len + 7 + 1 + subprogram_name_len;
    let subprogram_low_pc_section_offset = 11 + subprogram_low_pc_die_offset;

    relocs.push((
        DwarfSection::DebugInfo,
        DwarfRelocation {
            offset: subprogram_low_pc_section_offset as u32,
            addend: 0,
        },
    ));

    // debug_line section: the DW_LNE_SET_ADDRESS extended opcode.
    // The line program starts after the header. The header has a known
    // structure but variable length (depends on directory/file names).
    // The SET_ADDRESS opcode is: 0x00 (extended), length ULEB128, 0x02, then 8 bytes.
    // We need to find the 8-byte address. Search for the pattern:
    // 0x00, <uleb128 for 9>, 0x02, <8 zero bytes>
    // This is fragile but works for our generated DWARF.
    for i in 0..debug_line_section.len().saturating_sub(11) {
        if debug_line_section[i] == 0x00 // extended opcode marker
            && debug_line_section[i + 2] == 0x02
        // DW_LNE_SET_ADDRESS
        {
            // The address starts at i+3
            let addr_offset = i + 3;
            // Verify it's 8 zero bytes (our code_address=0)
            if addr_offset + 8 <= debug_line_section.len()
                && debug_line_section[addr_offset..addr_offset + 8] == [0; 8]
            {
                relocs.push((
                    DwarfSection::DebugLine,
                    DwarfRelocation {
                        offset: addr_offset as u32,
                        addend: 0,
                    },
                ));
                break;
            }
        }
    }

    relocs
}

pub fn build_debug_abbrev_section() -> Vec<u8> {
    let mut out = Vec::new();
    // Abbrev 1: compile unit (has children: subprogram DIE).
    push_uleb128(&mut out, 1);
    out.push(DW_TAG_COMPILE_UNIT);
    out.push(DW_CHILDREN_YES);
    out.push(DW_AT_STMT_LIST);
    out.push(DW_FORM_SEC_OFFSET);
    out.push(DW_AT_LOW_PC);
    out.push(DW_FORM_ADDR);
    out.push(DW_AT_HIGH_PC);
    out.push(DW_FORM_DATA8);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(0);
    out.push(0);

    // Abbrev 2: base type (u64).
    push_uleb128(&mut out, 2);
    out.push(DW_TAG_BASE_TYPE);
    out.push(DW_CHILDREN_NO);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(DW_AT_ENCODING);
    out.push(DW_FORM_DATA1);
    out.push(DW_AT_BYTE_SIZE);
    out.push(DW_FORM_DATA1);
    out.push(0);
    out.push(0);

    // Abbrev 3: subprogram (has children: variable DIEs).
    push_uleb128(&mut out, 3);
    out.push(DW_TAG_SUBPROGRAM);
    out.push(DW_CHILDREN_YES);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(DW_AT_LOW_PC);
    out.push(DW_FORM_ADDR);
    out.push(DW_AT_HIGH_PC);
    out.push(DW_FORM_DATA8);
    out.push(DW_AT_FRAME_BASE);
    out.push(DW_FORM_EXPRLOC);
    out.push(0);
    out.push(0);

    // Abbrev 4: variable with inline exprloc.
    push_uleb128(&mut out, 4);
    out.push(DW_TAG_VARIABLE);
    out.push(DW_CHILDREN_NO);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(DW_AT_LOCATION);
    out.push(DW_FORM_EXPRLOC);
    out.push(DW_AT_TYPE);
    out.push(DW_FORM_REF4);
    out.push(0);
    out.push(0);

    // Abbrev 5: variable with .debug_loc location list.
    push_uleb128(&mut out, 5);
    out.push(DW_TAG_VARIABLE);
    out.push(DW_CHILDREN_NO);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(DW_AT_LOCATION);
    out.push(DW_FORM_SEC_OFFSET);
    out.push(DW_AT_TYPE);
    out.push(DW_FORM_REF4);
    out.push(0);
    out.push(0);

    // Abbrev 6: lexical block (has children: vars and nested lexical blocks).
    push_uleb128(&mut out, 6);
    out.push(DW_TAG_LEXICAL_BLOCK);
    out.push(DW_CHILDREN_YES);
    out.push(DW_AT_RANGES);
    out.push(DW_FORM_SEC_OFFSET);
    out.push(0);
    out.push(0);

    // End abbrev table.
    out.push(0);
    out
}

pub fn build_debug_info_section(
    code_address: u64,
    code_size: u64,
    file_name: &str,
    subprogram_name: &str,
    frame_base_expr: &[u8],
    variables: &[DwarfVariable],
    lexical_blocks: &[JitDebugLexicalBlock],
    variable_loc_offsets: &[u32],
    lexical_block_range_offsets: &[u32],
) -> Vec<u8> {
    let mut die = Vec::new();
    // Compile unit DIE (abbrev 1).
    push_uleb128(&mut die, 1);
    die.extend_from_slice(&0u32.to_le_bytes()); // DW_AT_stmt_list -> .debug_line offset 0
    die.extend_from_slice(&code_address.to_le_bytes()); // DW_AT_low_pc
    die.extend_from_slice(&code_size.to_le_bytes()); // DW_AT_high_pc as length (DW_FORM_data8)
    die.extend_from_slice(file_name.as_bytes()); // DW_AT_name
    die.push(0);

    // Base type DIE (abbrev 2), used by variable DIEs in phase #173.
    let base_type_die_offset = 11u32 + (die.len() as u32);
    push_uleb128(&mut die, 2);
    die.extend_from_slice(b"u64");
    die.push(0);
    die.push(DW_ATE_UNSIGNED);
    die.push(8);

    // Subprogram DIE (abbrev 3) as a child of CU.
    push_uleb128(&mut die, 3);
    die.extend_from_slice(subprogram_name.as_bytes());
    die.push(0);
    die.extend_from_slice(&code_address.to_le_bytes()); // DW_AT_low_pc
    die.extend_from_slice(&code_size.to_le_bytes()); // DW_AT_high_pc as length
    push_uleb128(&mut die, frame_base_expr.len() as u64);
    die.extend_from_slice(frame_base_expr);

    // Variable DIEs and lexical blocks as children of subprogram.
    let mut next_loc_offset = 0usize;
    let mut next_range_offset = 0usize;
    emit_variable_dies(
        &mut die,
        variables,
        lexical_blocks,
        variable_loc_offsets,
        lexical_block_range_offsets,
        &mut next_loc_offset,
        &mut next_range_offset,
        base_type_die_offset,
    );

    // End of subprogram children, then end of CU children.
    die.push(0);
    die.push(0);

    let unit_length = 2u32 + 4u32 + 1u32 + (die.len() as u32);
    let mut section = Vec::with_capacity(4 + unit_length as usize);
    section.extend_from_slice(&unit_length.to_le_bytes());
    section.extend_from_slice(&INFO_VERSION.to_le_bytes());
    section.extend_from_slice(&0u32.to_le_bytes()); // debug_abbrev_offset
    section.push(ADDRESS_SIZE_64);
    section.extend_from_slice(&die);
    section
}

fn emit_variable_dies(
    die: &mut Vec<u8>,
    variables: &[DwarfVariable],
    lexical_blocks: &[JitDebugLexicalBlock],
    variable_loc_offsets: &[u32],
    lexical_block_range_offsets: &[u32],
    next_loc_offset: &mut usize,
    next_range_offset: &mut usize,
    base_type_die_offset: u32,
) {
    for variable in variables {
        match &variable.location {
            DwarfVariableLocation::Expr(expr) => {
                push_uleb128(die, 4);
                die.extend_from_slice(variable.name.as_bytes());
                die.push(0);
                push_uleb128(die, expr.len() as u64);
                die.extend_from_slice(expr);
                die.extend_from_slice(&base_type_die_offset.to_le_bytes());
            }
            DwarfVariableLocation::List(_ranges) => {
                let loc_offset = variable_loc_offsets[*next_loc_offset];
                *next_loc_offset += 1;
                push_uleb128(die, 5);
                die.extend_from_slice(variable.name.as_bytes());
                die.push(0);
                die.extend_from_slice(&loc_offset.to_le_bytes());
                die.extend_from_slice(&base_type_die_offset.to_le_bytes());
            }
        }
    }
    for lexical_block in lexical_blocks {
        emit_lexical_block_die(
            die,
            lexical_block,
            variable_loc_offsets,
            lexical_block_range_offsets,
            next_loc_offset,
            next_range_offset,
            base_type_die_offset,
        );
    }
}

fn emit_lexical_block_die(
    die: &mut Vec<u8>,
    lexical_block: &JitDebugLexicalBlock,
    variable_loc_offsets: &[u32],
    lexical_block_range_offsets: &[u32],
    next_loc_offset: &mut usize,
    next_range_offset: &mut usize,
    base_type_die_offset: u32,
) {
    push_uleb128(die, 6);
    let range_offset = lexical_block_range_offsets[*next_range_offset];
    *next_range_offset += 1;
    die.extend_from_slice(&range_offset.to_le_bytes());
    emit_variable_dies(
        die,
        &lexical_block.variables,
        &lexical_block.lexical_blocks,
        variable_loc_offsets,
        lexical_block_range_offsets,
        next_loc_offset,
        next_range_offset,
        base_type_die_offset,
    );
    die.push(0);
}

fn variables_in_preorder<'a>(
    variables: &'a [DwarfVariable],
    lexical_blocks: &'a [JitDebugLexicalBlock],
) -> Vec<&'a DwarfVariable> {
    let mut out = Vec::new();
    collect_variables_in_preorder(variables, lexical_blocks, &mut out);
    out
}

fn collect_variables_in_preorder<'a>(
    variables: &'a [DwarfVariable],
    lexical_blocks: &'a [JitDebugLexicalBlock],
    out: &mut Vec<&'a DwarfVariable>,
) {
    out.extend(variables.iter());
    for lexical_block in lexical_blocks {
        collect_variables_in_preorder(&lexical_block.variables, &lexical_block.lexical_blocks, out);
    }
}

fn subprogram_variables_in_preorder(subprogram: &JitDebugSubprogram) -> Vec<&DwarfVariable> {
    variables_in_preorder(&subprogram.variables, &subprogram.lexical_blocks)
}

pub fn build_debug_info_section_from_debug_info(
    debug_info: &JitDebugInfo,
    variable_loc_offsets: &[u32],
    lexical_block_range_offsets: &[u32],
) -> Vec<u8> {
    build_debug_info_section(
        debug_info.code_address,
        debug_info.code_size,
        &debug_info.line_table.file_name,
        &debug_info.subprogram.name,
        &debug_info.subprogram.frame_base_expression,
        &debug_info.subprogram.variables,
        &debug_info.subprogram.lexical_blocks,
        variable_loc_offsets,
        lexical_block_range_offsets,
    )
}

pub fn build_debug_loc_section(
    variables: &[DwarfVariable],
    lexical_blocks: &[JitDebugLexicalBlock],
    code_address: u64,
) -> (Vec<u8>, Vec<u32>) {
    let mut section = Vec::<u8>::new();
    let mut offsets = Vec::<u32>::new();
    for variable in variables_in_preorder(variables, lexical_blocks) {
        if let DwarfVariableLocation::List(locations) = &variable.location {
            offsets.push(section.len() as u32);
            for loc in locations {
                if loc.end <= loc.start {
                    continue;
                }
                let start = loc.start.saturating_sub(code_address);
                let end = loc.end.saturating_sub(code_address);
                section.extend_from_slice(&start.to_le_bytes());
                section.extend_from_slice(&end.to_le_bytes());
                section.extend_from_slice(&(loc.expression.len() as u16).to_le_bytes());
                section.extend_from_slice(&loc.expression);
            }
            // End-of-list marker for one variable.
            section.extend_from_slice(&0u64.to_le_bytes());
            section.extend_from_slice(&0u64.to_le_bytes());
        }
    }
    (section, offsets)
}

pub fn build_debug_loc_section_from_debug_info(debug_info: &JitDebugInfo) -> (Vec<u8>, Vec<u32>) {
    build_debug_loc_section(
        &debug_info.subprogram.variables,
        &debug_info.subprogram.lexical_blocks,
        debug_info.code_address,
    )
}

fn lexical_blocks_in_preorder(
    lexical_blocks: &[JitDebugLexicalBlock],
) -> Vec<&JitDebugLexicalBlock> {
    let mut out = Vec::new();
    collect_lexical_blocks_in_preorder(lexical_blocks, &mut out);
    out
}

fn collect_lexical_blocks_in_preorder<'a>(
    lexical_blocks: &'a [JitDebugLexicalBlock],
    out: &mut Vec<&'a JitDebugLexicalBlock>,
) {
    for lexical_block in lexical_blocks {
        out.push(lexical_block);
        collect_lexical_blocks_in_preorder(&lexical_block.lexical_blocks, out);
    }
}

pub fn build_debug_ranges_section(
    lexical_blocks: &[JitDebugLexicalBlock],
    code_address: u64,
) -> (Vec<u8>, Vec<u32>) {
    let mut section = Vec::<u8>::new();
    let mut offsets = Vec::<u32>::new();
    for lexical_block in lexical_blocks_in_preorder(lexical_blocks) {
        offsets.push(section.len() as u32);
        for range in &lexical_block.ranges {
            if range.high_pc <= range.low_pc {
                continue;
            }
            section.extend_from_slice(&range.low_pc.saturating_sub(code_address).to_le_bytes());
            section.extend_from_slice(&range.high_pc.saturating_sub(code_address).to_le_bytes());
        }
        section.extend_from_slice(&0u64.to_le_bytes());
        section.extend_from_slice(&0u64.to_le_bytes());
    }
    (section, offsets)
}

pub fn build_debug_ranges_section_from_debug_info(
    debug_info: &JitDebugInfo,
) -> (Vec<u8>, Vec<u32>) {
    build_debug_ranges_section(
        &debug_info.subprogram.lexical_blocks,
        debug_info.code_address,
    )
}

pub fn dwarf_register_from_hw_encoding(target_arch: DwarfTargetArch, hw_enc: u8) -> Option<u16> {
    match target_arch {
        DwarfTargetArch::X86_64 => Some(match hw_enc {
            0 => 0,   // rax
            1 => 2,   // rcx
            2 => 1,   // rdx
            3 => 3,   // rbx
            4 => 7,   // rsp
            5 => 6,   // rbp
            6 => 4,   // rsi
            7 => 5,   // rdi
            8 => 8,   // r8
            9 => 9,   // r9
            10 => 10, // r10
            11 => 11, // r11
            12 => 12, // r12
            13 => 13, // r13
            14 => 14, // r14
            15 => 15, // r15
            _ => return None,
        }),
        DwarfTargetArch::Aarch64 => (hw_enc <= 31).then_some(hw_enc as u16),
    }
}

pub fn frame_base_register(target_arch: DwarfTargetArch) -> u16 {
    match target_arch {
        DwarfTargetArch::X86_64 => 7,   // rsp
        DwarfTargetArch::Aarch64 => 29, // x29 (fp)
    }
}

pub fn expr_reg(dwarf_reg: u16) -> Vec<u8> {
    if dwarf_reg < 32 {
        return vec![DW_OP_REG0 + (dwarf_reg as u8)];
    }
    let mut out = vec![DW_OP_REGX];
    push_uleb128(&mut out, dwarf_reg as u64);
    out
}

pub fn expr_breg(dwarf_reg: u16, offset: i64) -> Vec<u8> {
    let mut out = Vec::new();
    if dwarf_reg < 32 {
        out.push(DW_OP_BREG0 + (dwarf_reg as u8));
    } else {
        out.push(DW_OP_BREGX);
        push_uleb128(&mut out, dwarf_reg as u64);
    }
    push_sleb128(&mut out, offset);
    out
}

pub fn expr_fbreg(offset: i64) -> Vec<u8> {
    let mut out = vec![DW_OP_FBREG];
    push_sleb128(&mut out, offset);
    out
}

pub fn expr_plus_uconst(value: u64) -> Vec<u8> {
    let mut out = vec![DW_OP_PLUS_UCONST];
    push_uleb128(&mut out, value);
    out
}

pub fn expr_deref_size(size: u8) -> Vec<u8> {
    vec![DW_OP_DEREF_SIZE, size]
}

pub fn expr_stack_value() -> Vec<u8> {
    vec![DW_OP_STACK_VALUE]
}

pub fn expr_breg_deref_size(dwarf_reg: u16, offset: i64, size: u8) -> Vec<u8> {
    let mut out = expr_breg(dwarf_reg, offset);
    out.push(DW_OP_DEREF_SIZE);
    out.push(size);
    out
}

pub fn expr_breg_deref_size_stack_value(dwarf_reg: u16, offset: i64, size: u8) -> Vec<u8> {
    let mut out = expr_breg_deref_size(dwarf_reg, offset, size);
    out.push(DW_OP_STACK_VALUE);
    out
}

pub fn expr_fbreg_deref_size(offset: i64, size: u8) -> Vec<u8> {
    let mut out = expr_fbreg(offset);
    out.push(DW_OP_DEREF_SIZE);
    out.push(size);
    out
}

pub fn build_debug_line_section(
    code_address: u64,
    code_size: u64,
    source_map: &[(u32, u32)],
    file_name: &str,
    directory: Option<&str>,
) -> Result<Vec<u8>, DwarfPrepError> {
    if code_size > u32::MAX as u64 {
        return Err(DwarfPrepError::CodeSizeOutOfBounds { code_size });
    }
    if file_name.as_bytes().contains(&0) {
        return Err(DwarfPrepError::InteriorNul { field: "file_name" });
    }
    if let Some(dir) = directory
        && dir.as_bytes().contains(&0)
    {
        return Err(DwarfPrepError::InteriorNul { field: "directory" });
    }

    for window in source_map.windows(2) {
        let previous = window[0].0;
        let next = window[1].0;
        if next <= previous {
            return Err(DwarfPrepError::SourceMapNotStrictlyIncreasing {
                previous_offset: previous,
                next_offset: next,
            });
        }
    }

    for (offset, _) in source_map {
        if (*offset as u64) > code_size {
            return Err(DwarfPrepError::SourceOffsetOutOfBounds {
                offset: *offset,
                code_size,
            });
        }
    }

    let mut header_body = vec![
        MIN_INSN_LEN,
        MAX_OPS_PER_INSN,
        DEFAULT_IS_STMT,
        LINE_BASE as u8,
        LINE_RANGE,
        OPCODE_BASE,
    ];
    header_body.extend_from_slice(&STANDARD_OPCODE_LENGTHS);

    let has_dir = directory.is_some_and(|dir| !dir.is_empty());
    if let Some(dir) = directory.filter(|dir| !dir.is_empty()) {
        header_body.extend_from_slice(dir.as_bytes());
        header_body.push(0);
    }
    header_body.push(0); // end include_directories

    header_body.extend_from_slice(file_name.as_bytes());
    header_body.push(0);
    push_uleb128(&mut header_body, if has_dir { 1 } else { 0 });
    push_uleb128(&mut header_body, 0); // mtime
    push_uleb128(&mut header_body, 0); // size
    header_body.push(0); // end file_names

    let mut program = Vec::new();
    // Extended opcode: set absolute text address for sequence.
    program.push(0);
    push_uleb128(&mut program, 1 + 8);
    program.push(DW_LNE_SET_ADDRESS);
    program.extend_from_slice(&code_address.to_le_bytes());

    let mut current_offset = 0u64;
    let mut current_line = 1i64;

    for (offset, ra_mir_inst_index) in source_map {
        let offset = *offset as u64;
        let line = (*ra_mir_inst_index as i64) + 1;

        if offset > current_offset {
            program.push(DW_LNS_ADVANCE_PC);
            push_uleb128(&mut program, offset - current_offset);
            current_offset = offset;
        }

        let delta_line = line - current_line;
        if delta_line != 0 {
            program.push(DW_LNS_ADVANCE_LINE);
            push_sleb128(&mut program, delta_line);
            current_line = line;
        }

        program.push(DW_LNS_COPY);
    }

    if code_size > current_offset {
        program.push(DW_LNS_ADVANCE_PC);
        push_uleb128(&mut program, code_size - current_offset);
    }

    program.push(0);
    push_uleb128(&mut program, 1);
    program.push(DW_LNE_END_SEQUENCE);

    let header_length = header_body.len() as u32;
    let unit_length = 2u32 + 4u32 + header_length + (program.len() as u32);

    let mut section = Vec::with_capacity(4 + unit_length as usize);
    section.extend_from_slice(&unit_length.to_le_bytes());
    section.extend_from_slice(&LINE_VERSION.to_le_bytes());
    section.extend_from_slice(&header_length.to_le_bytes());
    section.extend_from_slice(&header_body);
    section.extend_from_slice(&program);
    section.shrink_to_fit();
    Ok(section)
}

pub fn build_debug_line_section_from_debug_info(
    debug_info: &JitDebugInfo,
) -> Result<Vec<u8>, DwarfPrepError> {
    let rows = debug_info
        .line_table
        .rows
        .iter()
        .map(|row| (row.code_offset, row.line.saturating_sub(1)))
        .collect::<Vec<_>>();
    build_debug_line_section(
        debug_info.code_address,
        debug_info.code_size,
        &rows,
        &debug_info.line_table.file_name,
        debug_info.line_table.directory.as_deref(),
    )
}

fn push_uleb128(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_sleb128(out: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value as u8) & 0x7f;
        let sign_bit_set = (byte & 0x40) != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set);
        out.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_uleb(bytes: &[u8], i: &mut usize) -> u64 {
        let mut shift = 0u32;
        let mut out = 0u64;
        loop {
            let byte = bytes[*i];
            *i += 1;
            out |= ((byte & 0x7f) as u64) << shift;
            if (byte & 0x80) == 0 {
                return out;
            }
            shift += 7;
        }
    }

    fn parse_sleb(bytes: &[u8], i: &mut usize) -> i64 {
        let mut shift = 0u32;
        let mut out = 0i64;
        let mut byte;
        loop {
            byte = bytes[*i];
            *i += 1;
            out |= ((byte & 0x7f) as i64) << shift;
            shift += 7;
            if (byte & 0x80) == 0 {
                break;
            }
        }
        if shift < 64 && (byte & 0x40) != 0 {
            out |= !0i64 << shift;
        }
        out
    }

    fn parse_debug_line_rows(section: &[u8]) -> Vec<(u64, i64)> {
        let unit_length = u32::from_le_bytes(section[0..4].try_into().unwrap()) as usize;
        let version = u16::from_le_bytes(section[4..6].try_into().unwrap());
        assert_eq!(version, 4);
        let header_length = u32::from_le_bytes(section[6..10].try_into().unwrap()) as usize;
        let mut program_i = 10 + header_length;
        assert_eq!(section.len(), 4 + unit_length);

        let mut rows = Vec::new();
        let mut address = 0u64;
        let mut line = 1i64;

        while program_i < section.len() {
            let opcode = section[program_i];
            program_i += 1;
            match opcode {
                0 => {
                    let len = parse_uleb(section, &mut program_i) as usize;
                    let sub = section[program_i];
                    program_i += 1;
                    match sub {
                        DW_LNE_SET_ADDRESS => {
                            assert_eq!(len, 9);
                            address = u64::from_le_bytes(
                                section[program_i..program_i + 8].try_into().unwrap(),
                            );
                            program_i += 8;
                        }
                        DW_LNE_END_SEQUENCE => {
                            break;
                        }
                        _ => panic!("unexpected extended opcode {sub}"),
                    }
                }
                DW_LNS_ADVANCE_PC => {
                    address += parse_uleb(section, &mut program_i);
                }
                DW_LNS_ADVANCE_LINE => {
                    line += parse_sleb(section, &mut program_i);
                }
                DW_LNS_COPY => rows.push((address, line)),
                other => panic!("unexpected standard opcode {other}"),
            }
        }

        rows
    }

    #[test]
    fn debug_line_v4_header_has_max_ops_field() {
        let section = build_debug_line_section(0x3000, 0, &[], "decoder.ra", None).unwrap();
        // unit_length (4), version (2), header_length (4), then header body.
        assert_eq!(u16::from_le_bytes(section[4..6].try_into().unwrap()), 4);
        assert_eq!(section[10], MIN_INSN_LEN);
        assert_eq!(section[11], MAX_OPS_PER_INSN);
        assert_eq!(section[12], DEFAULT_IS_STMT);
    }

    #[test]
    fn debug_line_rows_match_source_map() {
        let section = build_debug_line_section(
            0x1000,
            12,
            &[(0, 0), (4, 3), (9, 7)],
            "decoder.ra",
            Some("jit"),
        )
        .unwrap();

        let rows = parse_debug_line_rows(&section);
        assert_eq!(rows, vec![(0x1000, 1), (0x1004, 4), (0x1009, 8)]);
    }

    #[test]
    fn debug_line_allows_empty_source_map() {
        let section = build_debug_line_section(0x2000, 5, &[], "decoder.ra", None).unwrap();
        let rows = parse_debug_line_rows(&section);
        assert!(rows.is_empty());
    }

    #[test]
    fn rejects_invalid_source_map_and_inputs() {
        let err = build_debug_line_section(0, 8, &[(4, 1), (4, 2)], "f", None).unwrap_err();
        assert!(matches!(
            err,
            DwarfPrepError::SourceMapNotStrictlyIncreasing {
                previous_offset: 4,
                next_offset: 4
            }
        ));

        let err = build_debug_line_section(0, 7, &[(8, 0)], "f", None).unwrap_err();
        assert!(matches!(
            err,
            DwarfPrepError::SourceOffsetOutOfBounds {
                offset: 8,
                code_size: 7
            }
        ));

        let err = build_debug_line_section(0, 8, &[], "bad\0file", None).unwrap_err();
        assert!(matches!(
            err,
            DwarfPrepError::InteriorNul { field: "file_name" }
        ));
    }

    #[test]
    fn debug_abbrev_contains_cu_subprogram_variable_abbrevs() {
        let abbrev = build_debug_abbrev_section();
        assert_eq!(abbrev[0], 0x01); // CU abbrev code
        assert!(abbrev.contains(&DW_TAG_COMPILE_UNIT));
        assert!(abbrev.contains(&DW_TAG_LEXICAL_BLOCK));
        assert!(abbrev.contains(&DW_TAG_SUBPROGRAM));
        assert!(abbrev.contains(&DW_TAG_VARIABLE));
        assert_eq!(abbrev.last().copied(), Some(0));
    }

    #[test]
    fn debug_info_contains_cu_subprogram_and_variables() {
        let variables = vec![DwarfVariable {
            name: "my_struct.count".to_string(),
            location: DwarfVariableLocation::Expr(expr_reg(0)),
        }];
        let info = build_debug_info_section(
            0x1234,
            0x56,
            "decoder.ra",
            "kajit::decode::Example",
            &expr_reg(frame_base_register(DwarfTargetArch::X86_64)),
            &variables,
            &[],
            &[0],
            &[],
        );

        let unit_length = u32::from_le_bytes(info[0..4].try_into().unwrap()) as usize;
        assert_eq!(info.len(), 4 + unit_length);
        assert_eq!(u16::from_le_bytes(info[4..6].try_into().unwrap()), 4);
        assert_eq!(u32::from_le_bytes(info[6..10].try_into().unwrap()), 0);
        assert_eq!(info[10], 8);

        let mut i = 11usize;
        assert_eq!(parse_uleb(&info, &mut i), 1);
        assert_eq!(u32::from_le_bytes(info[i..i + 4].try_into().unwrap()), 0);
        i += 4;
        assert_eq!(
            u64::from_le_bytes(info[i..i + 8].try_into().unwrap()),
            0x1234
        );
        i += 8;
        assert_eq!(u64::from_le_bytes(info[i..i + 8].try_into().unwrap()), 0x56);
        i += 8;
        let end = info[i..].iter().position(|b| *b == 0).unwrap();
        assert_eq!(&info[i..i + end], b"decoder.ra");
        i += end + 1;

        let base_type_offset = i as u32;
        assert_eq!(parse_uleb(&info, &mut i), 2); // base type
        let end = info[i..].iter().position(|b| *b == 0).unwrap();
        assert_eq!(&info[i..i + end], b"u64");
        i += end + 1;
        assert_eq!(info[i], DW_ATE_UNSIGNED);
        i += 1;
        assert_eq!(info[i], 8);
        i += 1;

        assert_eq!(parse_uleb(&info, &mut i), 3); // subprogram
        let end = info[i..].iter().position(|b| *b == 0).unwrap();
        assert_eq!(&info[i..i + end], b"kajit::decode::Example");
        i += end + 1;
        assert_eq!(
            u64::from_le_bytes(info[i..i + 8].try_into().unwrap()),
            0x1234
        );
        i += 8;
        assert_eq!(u64::from_le_bytes(info[i..i + 8].try_into().unwrap()), 0x56);
        i += 8;
        let frame_base_len = parse_uleb(&info, &mut i) as usize;
        assert!(frame_base_len > 0);
        i += frame_base_len;

        assert_eq!(parse_uleb(&info, &mut i), 4); // variable
        let end = info[i..].iter().position(|b| *b == 0).unwrap();
        assert_eq!(&info[i..i + end], b"my_struct.count");
        i += end + 1;
        assert_eq!(parse_uleb(&info, &mut i), 1);
        assert_eq!(info[i], DW_OP_REG0);
        i += 1;
        assert_eq!(
            u32::from_le_bytes(info[i..i + 4].try_into().unwrap()),
            base_type_offset
        );
        i += 4;

        assert_eq!(info[i], 0); // end subprogram children
        assert_eq!(info[i + 1], 0); // end CU children
    }

    #[test]
    fn debug_loc_builds_per_variable_lists_with_terminators() {
        let vars = vec![
            DwarfVariable {
                name: "a".to_string(),
                location: DwarfVariableLocation::List(vec![
                    DwarfLocationRange {
                        start: 0x10,
                        end: 0x20,
                        expression: expr_reg(10),
                    },
                    DwarfLocationRange {
                        start: 0x20,
                        end: 0x40,
                        expression: expr_fbreg(-24),
                    },
                ]),
            },
            DwarfVariable {
                name: "b".to_string(),
                location: DwarfVariableLocation::Expr(expr_reg(0)),
            },
        ];

        let (loc, offsets) = build_debug_loc_section(&vars, &[], 0);
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0], 0);
        assert!(!loc.is_empty());

        let first_start = u64::from_le_bytes(loc[0..8].try_into().unwrap());
        let first_end = u64::from_le_bytes(loc[8..16].try_into().unwrap());
        assert_eq!(first_start, 0x10);
        assert_eq!(first_end, 0x20);
    }

    #[test]
    fn debug_loc_encodes_ranges_relative_to_cu_low_pc() {
        let vars = vec![DwarfVariable {
            name: "a".to_string(),
            location: DwarfVariableLocation::List(vec![DwarfLocationRange {
                start: 0x1010,
                end: 0x1020,
                expression: expr_reg(10),
            }]),
        }];

        let (loc, offsets) = build_debug_loc_section(&vars, &[], 0x1000);
        assert_eq!(offsets, vec![0]);
        let start = u64::from_le_bytes(loc[0..8].try_into().unwrap());
        let end = u64::from_le_bytes(loc[8..16].try_into().unwrap());
        assert_eq!(start, 0x10);
        assert_eq!(end, 0x20);
    }

    #[test]
    fn debug_loc_collects_lexical_block_variables() {
        let lexical_blocks = vec![JitDebugLexicalBlock {
            ranges: vec![JitDebugRange {
                low_pc: 0x1000,
                high_pc: 0x1010,
            }],
            variables: vec![DwarfVariable {
                name: "scoped".to_string(),
                location: DwarfVariableLocation::List(vec![DwarfLocationRange {
                    start: 0x1004,
                    end: 0x1008,
                    expression: expr_reg(9),
                }]),
            }],
            lexical_blocks: Vec::new(),
        }];

        let (loc, offsets) = build_debug_loc_section(&[], &lexical_blocks, 0x1000);
        assert_eq!(offsets, vec![0]);
        let start = u64::from_le_bytes(loc[0..8].try_into().unwrap());
        let end = u64::from_le_bytes(loc[8..16].try_into().unwrap());
        assert_eq!(start, 0x4);
        assert_eq!(end, 0x8);
    }

    #[test]
    fn debug_ranges_encode_ranges_relative_to_cu_low_pc() {
        let lexical_blocks = vec![JitDebugLexicalBlock {
            ranges: vec![
                JitDebugRange {
                    low_pc: 0x1010,
                    high_pc: 0x1020,
                },
                JitDebugRange {
                    low_pc: 0x1030,
                    high_pc: 0x1040,
                },
            ],
            variables: Vec::new(),
            lexical_blocks: Vec::new(),
        }];

        let (ranges, offsets) = build_debug_ranges_section(&lexical_blocks, 0x1000);
        assert_eq!(offsets, vec![0]);
        let first_start = u64::from_le_bytes(ranges[0..8].try_into().unwrap());
        let first_end = u64::from_le_bytes(ranges[8..16].try_into().unwrap());
        let second_start = u64::from_le_bytes(ranges[16..24].try_into().unwrap());
        let second_end = u64::from_le_bytes(ranges[24..32].try_into().unwrap());
        let terminator_start = u64::from_le_bytes(ranges[32..40].try_into().unwrap());
        let terminator_end = u64::from_le_bytes(ranges[40..48].try_into().unwrap());
        assert_eq!(first_start, 0x10);
        assert_eq!(first_end, 0x20);
        assert_eq!(second_start, 0x30);
        assert_eq!(second_end, 0x40);
        assert_eq!(terminator_start, 0);
        assert_eq!(terminator_end, 0);
    }

    #[test]
    fn register_mapping_is_arch_specific_at_runtime() {
        let x64_rcx = dwarf_register_from_hw_encoding(DwarfTargetArch::X86_64, 1).unwrap();
        let arm_x1 = dwarf_register_from_hw_encoding(DwarfTargetArch::Aarch64, 1).unwrap();
        assert_eq!(x64_rcx, 2);
        assert_eq!(arm_x1, 1);
    }

    #[test]
    fn expr_breg_encodes_register_plus_offset() {
        let expr = expr_breg(7, 0);
        assert_eq!(expr, vec![DW_OP_BREG0 + 7, 0]);
        let expr = expr_breg(35, -8);
        assert_eq!(expr[0], DW_OP_BREGX);
    }

    #[test]
    fn expr_breg_deref_size_stack_value_marks_scalar_value() {
        let expr = expr_breg_deref_size_stack_value(7, 4, 1);
        assert_eq!(
            expr,
            vec![DW_OP_BREG0 + 7, 4, DW_OP_DEREF_SIZE, 1, DW_OP_STACK_VALUE]
        );
    }

    #[test]
    fn build_jit_dwarf_sections_populates_all_sections() {
        let sections = build_jit_dwarf_sections_with_variables(
            DwarfTargetArch::X86_64,
            0x1000,
            8,
            &[(0, 0)],
            "decoder.ra",
            Some("jit"),
            "kajit::decode::Bools",
            &[DwarfVariable {
                name: "flag".to_string(),
                location: DwarfVariableLocation::Expr(expr_reg(0)),
            }],
        )
        .unwrap();
        assert!(!sections.debug_line.is_empty());
        assert!(!sections.debug_abbrev.is_empty());
        assert!(!sections.debug_info.is_empty());
        assert!(!sections.is_empty());
    }

    #[test]
    fn build_jit_dwarf_sections_uses_debug_loc_for_ranged_variables() {
        let sections = build_jit_dwarf_sections_with_variables(
            DwarfTargetArch::X86_64,
            0x1000,
            8,
            &[(0, 0)],
            "decoder.ra",
            Some("jit"),
            "kajit::decode::Bools",
            &[DwarfVariable {
                name: "flag".to_string(),
                location: DwarfVariableLocation::List(vec![DwarfLocationRange {
                    start: 0x1000,
                    end: 0x1004,
                    expression: expr_reg(0),
                }]),
            }],
        )
        .unwrap();
        assert!(!sections.debug_loc.is_empty());
    }

    #[test]
    fn build_jit_dwarf_sections_from_debug_info_populates_all_sections() {
        let debug_info = JitDebugInfo {
            target_arch: DwarfTargetArch::X86_64,
            code_address: 0x1000,
            code_size: 8,
            line_table: JitDebugLineTable {
                file_name: "decoder.ra".to_string(),
                directory: Some("jit".to_string()),
                rows: vec![JitDebugLineRow {
                    code_offset: 0,
                    line: 1,
                }],
            },
            subprogram: JitDebugSubprogram {
                name: "kajit::decode::Bools".to_string(),
                frame_base_expression: expr_breg(frame_base_register(DwarfTargetArch::X86_64), 0),
                variables: vec![DwarfVariable {
                    name: "flag".to_string(),
                    location: DwarfVariableLocation::Expr(expr_reg(0)),
                }],
                lexical_blocks: vec![JitDebugLexicalBlock {
                    ranges: vec![JitDebugRange {
                        low_pc: 0x1000,
                        high_pc: 0x1008,
                    }],
                    variables: vec![DwarfVariable {
                        name: "v0".to_string(),
                        location: DwarfVariableLocation::List(vec![DwarfLocationRange {
                            start: 0x1004,
                            end: 0x1008,
                            expression: expr_reg(1),
                        }]),
                    }],
                    lexical_blocks: Vec::new(),
                }],
            },
        };

        let sections = build_jit_dwarf_sections_from_debug_info(&debug_info).unwrap();
        assert!(!sections.debug_line.is_empty());
        assert!(!sections.debug_abbrev.is_empty());
        assert!(!sections.debug_info.is_empty());
        assert!(!sections.debug_loc.is_empty());
        assert!(!sections.is_empty());
    }
}
