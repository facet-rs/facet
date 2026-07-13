//! Hand-rolled DWARF v4 emission for JIT code (no gimli dependency).
//!
//! Two layers:
//! - [`JitDebugInfo`], a structured debug-info model (line table, subprogram, variables,
//!   lexical blocks) owned by the compiler that produced the code.
//! - a DWARF v4 serializer lowering that model to ELF section bytes
//!   (`.debug_line`/`.debug_abbrev`/`.debug_info`/`.debug_loc`/`.debug_ranges`), consumed by
//!   [`super::debug::register_jit_code_with_dwarf`].
//!
//! The common entry is [`build_jit_dwarf_sections`]: a `source_map` of
//! `(code_offset, line_index, column)` triples (line = index + 1; column 1-based, 0 = none)
//! becomes a `.debug_line` program of `advance_pc`/`advance_line`/`set_column` rows. Columns
//! let one source line carry many JIT regions — each sub-expression of a template line, say —
//! so the *real* source file can be the debug source. Every row also sets `prologue_end`
//! (JIT stencils have no prologue; without it debuggers slide breakpoints past a region's
//! first instruction). Offsets must be strictly increasing;
//! [`super::debug::register_jit_source`] sorts for you.
//!
//! Provenance: salvaged from bearcove/kajit (scrapped) and adopted here. Style lints allowed as
//! vendored code.
#![allow(dead_code, clippy::too_many_arguments, clippy::enum_variant_names)]

use std::collections::BTreeMap;
use std::marker::PhantomData;

use crate::mem::{
    Access, Descriptor, EnumAccess, FieldAccess, Layout, MapStorage, Presence, RecordAccess,
    SequenceStorage, Tag,
};

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
    /// 1-based source column; 0 = no column information (DWARF convention). Columns let ONE
    /// source line carry many JIT regions — e.g. each sub-expression of a template line.
    pub column: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DwarfTypeId(usize);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JitDebugTypeGraph {
    pub types: Vec<DwarfTypeDie>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwarfTypeDie {
    Base(DwarfBaseType),
    Structure(DwarfStructureType),
    Array(DwarfArrayType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfBaseType {
    pub name: String,
    pub encoding: DwarfScalarEncoding,
    pub byte_size: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DwarfScalarEncoding {
    Address,
    Boolean,
    Float,
    Signed,
    SignedChar,
    Unsigned,
    UnsignedChar,
}

impl DwarfScalarEncoding {
    fn dwarf_encoding(self) -> u8 {
        match self {
            Self::Address => DW_ATE_ADDRESS,
            Self::Boolean => DW_ATE_BOOLEAN,
            Self::Float => DW_ATE_FLOAT,
            Self::Signed => DW_ATE_SIGNED,
            Self::SignedChar => DW_ATE_SIGNED_CHAR,
            Self::Unsigned => DW_ATE_UNSIGNED,
            Self::UnsignedChar => DW_ATE_UNSIGNED_CHAR,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfStructureType {
    pub name: String,
    pub byte_size: u32,
    pub members: Vec<DwarfMember>,
    pub variant_part: Option<DwarfVariantPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfMember {
    pub name: String,
    pub type_id: DwarfTypeId,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfVariantPart {
    pub discriminant_member: usize,
    pub variants: Vec<DwarfVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfVariant {
    pub name: String,
    pub discriminant: u64,
    pub members: Vec<DwarfMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DwarfArrayType {
    pub name: String,
    pub byte_size: u32,
    pub element_type: DwarfTypeId,
    pub count: u32,
    pub stride: u32,
}

impl JitDebugTypeGraph {
    fn push_placeholder(&mut self, name: String, byte_size: u32) -> DwarfTypeId {
        let id = DwarfTypeId(self.types.len());
        self.types.push(DwarfTypeDie::Structure(DwarfStructureType {
            name,
            byte_size,
            members: Vec::new(),
            variant_part: None,
        }));
        id
    }
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
    VariableTypeCountMismatch {
        variables: usize,
        variable_descriptors: usize,
    },
    TypeValueOutOfBounds {
        field: &'static str,
        value: usize,
    },
    RecurseWithoutEnclosingType {
        schema_name: String,
    },
}

const DW_LNS_COPY: u8 = 1;
const DW_LNS_ADVANCE_PC: u8 = 2;
const DW_LNS_ADVANCE_LINE: u8 = 3;
const DW_LNS_SET_COLUMN: u8 = 5;
const DW_LNS_SET_PROLOGUE_END: u8 = 10;

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
const DW_TAG_ARRAY_TYPE: u8 = 0x01;
const DW_TAG_BASE_TYPE: u8 = 0x24;
const DW_TAG_LEXICAL_BLOCK: u8 = 0x0b;
const DW_TAG_MEMBER: u8 = 0x0d;
const DW_TAG_STRUCTURE_TYPE: u8 = 0x13;
const DW_TAG_SUBPROGRAM: u8 = 0x2e;
const DW_TAG_SUBRANGE_TYPE: u8 = 0x21;
const DW_TAG_VARIABLE: u8 = 0x34;
const DW_TAG_VARIANT: u8 = 0x19;
const DW_TAG_VARIANT_PART: u8 = 0x33;
const DW_CHILDREN_YES: u8 = 0x01;
const DW_CHILDREN_NO: u8 = 0x00;
const DW_AT_LOCATION: u8 = 0x02;
const DW_AT_NAME: u8 = 0x03;
const DW_AT_STMT_LIST: u8 = 0x10;
const DW_AT_LOW_PC: u8 = 0x11;
const DW_AT_HIGH_PC: u8 = 0x12;
const DW_AT_BYTE_SIZE: u8 = 0x0b;
const DW_AT_DISCR: u8 = 0x15;
const DW_AT_DISCR_VALUE: u8 = 0x16;
const DW_AT_FRAME_BASE: u8 = 0x40;
const DW_AT_RANGES: u8 = 0x55;
const DW_AT_TYPE: u8 = 0x49;
const DW_AT_ENCODING: u8 = 0x3e;
const DW_AT_COUNT: u8 = 0x37;
const DW_AT_DATA_MEMBER_LOCATION: u8 = 0x38;
const DW_AT_BYTE_STRIDE: u8 = 0x51;
const DW_FORM_ADDR: u8 = 0x01;
const DW_FORM_DATA4: u8 = 0x06;
const DW_FORM_DATA8: u8 = 0x07;
const DW_FORM_STRING: u8 = 0x08;
const DW_FORM_DATA1: u8 = 0x0b;
const DW_FORM_REF4: u8 = 0x13;
const DW_FORM_SEC_OFFSET: u8 = 0x17;
const DW_FORM_EXPRLOC: u8 = 0x18;
const DW_ATE_ADDRESS: u8 = 0x01;
const DW_ATE_BOOLEAN: u8 = 0x02;
const DW_ATE_FLOAT: u8 = 0x04;
const DW_ATE_SIGNED: u8 = 0x05;
const DW_ATE_SIGNED_CHAR: u8 = 0x06;
const DW_ATE_UNSIGNED: u8 = 0x07;
const DW_ATE_UNSIGNED_CHAR: u8 = 0x08;
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DescriptorKey {
    Base {
        name: String,
        encoding: DwarfScalarEncoding,
        byte_size: u8,
    },
    Record {
        name: String,
        layout: LayoutKey,
        fields: Vec<FieldKey>,
    },
    Enum {
        name: String,
        layout: LayoutKey,
        tag: TagKey,
        variants: Vec<VariantKey>,
    },
    Option {
        name: String,
        layout: LayoutKey,
        presence: PresenceKey,
        some: Box<DescriptorKey>,
    },
    Array {
        name: String,
        layout: LayoutKey,
        element: Box<DescriptorKey>,
        count: usize,
        stride: usize,
    },
    Handle {
        name: String,
        layout: LayoutKey,
        target: String,
    },
    Tensor {
        name: String,
        layout: LayoutKey,
        element: Box<DescriptorKey>,
        data: SequenceStorageKey,
        shape: String,
        reshape: String,
    },
    Sequence {
        name: String,
        layout: LayoutKey,
        element: Box<DescriptorKey>,
        storage: SequenceStorageKey,
    },
    Set {
        name: String,
        layout: LayoutKey,
        element: Box<DescriptorKey>,
        storage: &'static str,
    },
    Map {
        name: String,
        layout: LayoutKey,
        key: Box<DescriptorKey>,
        value: Box<DescriptorKey>,
        storage: MapStorageKey,
    },
    Result {
        name: String,
        layout: LayoutKey,
        ok: Box<DescriptorKey>,
        err: Box<DescriptorKey>,
    },
    Pointer {
        name: String,
        layout: LayoutKey,
        pointee: Box<DescriptorKey>,
    },
    Dynamic {
        name: String,
        layout: LayoutKey,
    },
    Opaque {
        name: String,
        layout: LayoutKey,
    },
    Recurse {
        name: String,
        layout: LayoutKey,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LayoutKey {
    size: usize,
    align: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FieldKey {
    offset: usize,
    descriptor: DescriptorKey,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct VariantKey {
    index: u32,
    selector: u64,
    payload_fields: Vec<FieldKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TagKey {
    Direct { offset: usize, width: usize },
    Niche { offset: usize, width: usize },
    Thunk { read: String, write: String },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PresenceKey {
    Tag {
        offset: usize,
        width: usize,
        none_value: u64,
    },
    Niche {
        offset: usize,
        width: usize,
        none_pattern: Vec<u8>,
    },
    Thunk {
        is_some: String,
        set_none: String,
        set_some: String,
    },
    Vtable,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SequenceStorageKey {
    Owned {
        ptr_offset: usize,
        len_offset: usize,
        cap_offset: Option<usize>,
        allocate: String,
    },
    Borrowed {
        ptr_offset: usize,
        len_offset: usize,
    },
    Thunk {
        len: String,
        get: String,
        push: String,
    },
    Vtable,
    BorrowedVtable,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MapStorageKey {
    Thunk {
        len: String,
        iterate: String,
        insert: String,
    },
    Vtable,
}

struct DescriptorTypeLowerer<'a, SchemaRef, NameFn>
where
    NameFn: Fn(&SchemaRef) -> String,
{
    schema_name: &'a NameFn,
    graph: JitDebugTypeGraph,
    keys: BTreeMap<DescriptorKey, DwarfTypeId>,
    active_schema_types: BTreeMap<String, DwarfTypeId>,
    stack: Vec<DwarfTypeId>,
    _schema: PhantomData<SchemaRef>,
}

impl<'a, SchemaRef, NameFn> DescriptorTypeLowerer<'a, SchemaRef, NameFn>
where
    NameFn: Fn(&SchemaRef) -> String,
{
    fn new(schema_name: &'a NameFn) -> Self {
        Self {
            schema_name,
            graph: JitDebugTypeGraph::default(),
            keys: BTreeMap::new(),
            active_schema_types: BTreeMap::new(),
            stack: Vec::new(),
            _schema: PhantomData,
        }
    }

    fn into_graph(self) -> JitDebugTypeGraph {
        self.graph
    }

    fn lower(&mut self, descriptor: &Descriptor<SchemaRef>) -> Result<DwarfTypeId, DwarfPrepError> {
        let name = self.checked_schema_name(&descriptor.schema)?;
        if matches!(descriptor.access, Access::Recurse) {
            if let Some(id) = self.active_schema_types.get(&name).copied() {
                return Ok(id);
            }
            if let Some(id) = self.stack.last().copied() {
                return Ok(id);
            }
            return Err(DwarfPrepError::RecurseWithoutEnclosingType { schema_name: name });
        }
        if matches!(descriptor.access, Access::Scalar) {
            let byte_size = checked_u8("scalar_byte_size", descriptor.layout.size)?;
            return self.lower_base_type(name, DwarfScalarEncoding::Unsigned, byte_size);
        }

        let key = self.descriptor_key(descriptor)?;
        if let Some(id) = self.keys.get(&key).copied() {
            return Ok(id);
        }

        let byte_size = checked_u32("type_byte_size", descriptor.layout.size)?;
        let id = self.graph.push_placeholder(name.clone(), byte_size);
        self.keys.insert(key, id);
        let active_name = name.clone();
        let previous_active = self.active_schema_types.insert(active_name.clone(), id);
        self.stack.push(id);
        let die = self.lower_non_recurse(descriptor, name);
        self.stack.pop();
        if let Some(previous) = previous_active {
            self.active_schema_types.insert(active_name, previous);
        } else {
            self.active_schema_types.remove(&active_name);
        }
        self.graph.types[id.0] = die?;
        Ok(id)
    }

    fn lower_base_type(
        &mut self,
        name: String,
        encoding: DwarfScalarEncoding,
        byte_size: u8,
    ) -> Result<DwarfTypeId, DwarfPrepError> {
        reject_nul(&name, "type_name")?;
        let key = DescriptorKey::Base {
            name: name.clone(),
            encoding,
            byte_size,
        };
        if let Some(id) = self.keys.get(&key).copied() {
            return Ok(id);
        }
        let id = DwarfTypeId(self.graph.types.len());
        self.graph.types.push(DwarfTypeDie::Base(DwarfBaseType {
            name,
            encoding,
            byte_size,
        }));
        self.keys.insert(key, id);
        Ok(id)
    }

    fn lower_non_recurse(
        &mut self,
        descriptor: &Descriptor<SchemaRef>,
        name: String,
    ) -> Result<DwarfTypeDie, DwarfPrepError> {
        match &descriptor.access {
            Access::Scalar | Access::Recurse => unreachable!("handled before placeholder"),
            Access::Record(record) => Ok(DwarfTypeDie::Structure(DwarfStructureType {
                byte_size: checked_u32("type_byte_size", descriptor.layout.size)?,
                members: self.lower_record_members(record)?,
                variant_part: None,
                name,
            })),
            Access::Enum(enum_access) => self.lower_enum(name, descriptor.layout, enum_access),
            Access::Option(_) => self.lower_opaque_structure(name, descriptor.layout),
            Access::Array {
                element,
                count,
                stride,
            } => Ok(DwarfTypeDie::Array(DwarfArrayType {
                name,
                byte_size: checked_u32("array_byte_size", descriptor.layout.size)?,
                element_type: self.lower(element)?,
                count: checked_u32("array_count", *count)?,
                stride: checked_u32("array_stride", *stride)?,
            })),
            Access::Handle { .. }
            | Access::Tensor(_)
            | Access::Sequence(_)
            | Access::Set(_)
            | Access::Map(_)
            | Access::Result(_)
            | Access::Pointer(_)
            | Access::Dynamic
            | Access::Opaque(_) => self.lower_opaque_structure(name, descriptor.layout),
        }
    }

    fn lower_record_members(
        &mut self,
        record: &RecordAccess<SchemaRef>,
    ) -> Result<Vec<DwarfMember>, DwarfPrepError> {
        let mut members = Vec::with_capacity(record.fields.len());
        for (index, field) in record.fields.iter().enumerate() {
            members.push(DwarfMember {
                name: format!("field{index}"),
                type_id: self.lower(&field.descriptor)?,
                offset: checked_u32("member_offset", field.offset)?,
            });
        }
        Ok(members)
    }

    fn lower_enum(
        &mut self,
        name: String,
        layout: Layout,
        enum_access: &EnumAccess<SchemaRef>,
    ) -> Result<DwarfTypeDie, DwarfPrepError> {
        match enum_access.tag {
            Tag::Direct { offset, width } => {
                self.lower_tagged_enum(name, layout, enum_access, offset, width, "__tag")
            }
            Tag::Niche { offset, width } => {
                self.lower_tagged_enum(name, layout, enum_access, offset, width, "__niche")
            }
            Tag::Thunk { .. } => self.lower_opaque_structure(name, layout),
        }
    }

    fn lower_tagged_enum(
        &mut self,
        name: String,
        layout: Layout,
        enum_access: &EnumAccess<SchemaRef>,
        tag_offset: usize,
        tag_width: usize,
        tag_member_name: &str,
    ) -> Result<DwarfTypeDie, DwarfPrepError> {
        if tag_width == 0 {
            return Ok(DwarfTypeDie::Structure(DwarfStructureType {
                name,
                byte_size: checked_u32("type_byte_size", layout.size)?,
                members: Vec::new(),
                variant_part: None,
            }));
        }

        let tag_type = self.lower_unsigned_base_for_width(tag_width)?;
        let mut members = vec![DwarfMember {
            name: tag_member_name.to_string(),
            type_id: tag_type,
            offset: checked_u32("enum_tag_offset", tag_offset)?,
        }];
        let discriminant_member = 0;

        let mut variants = Vec::with_capacity(enum_access.variants.len());
        for variant in &enum_access.variants {
            variants.push(DwarfVariant {
                name: format!("variant{}", variant.index),
                discriminant: variant.selector,
                members: self.lower_variant_members(&variant.payload)?,
            });
        }

        Ok(DwarfTypeDie::Structure(DwarfStructureType {
            name,
            byte_size: checked_u32("type_byte_size", layout.size)?,
            members: {
                members.shrink_to_fit();
                members
            },
            variant_part: Some(DwarfVariantPart {
                discriminant_member,
                variants,
            }),
        }))
    }

    fn lower_variant_members(
        &mut self,
        payload: &RecordAccess<SchemaRef>,
    ) -> Result<Vec<DwarfMember>, DwarfPrepError> {
        let mut members = Vec::with_capacity(payload.fields.len());
        for (index, field) in payload.fields.iter().enumerate() {
            members.push(DwarfMember {
                name: format!("field{index}"),
                type_id: self.lower(&field.descriptor)?,
                offset: checked_u32("variant_member_offset", field.offset)?,
            });
        }
        Ok(members)
    }

    fn lower_opaque_structure(
        &self,
        name: String,
        layout: Layout,
    ) -> Result<DwarfTypeDie, DwarfPrepError> {
        Ok(DwarfTypeDie::Structure(DwarfStructureType {
            name,
            byte_size: checked_u32("type_byte_size", layout.size)?,
            members: Vec::new(),
            variant_part: None,
        }))
    }

    fn lower_unsigned_base_for_width(
        &mut self,
        width: usize,
    ) -> Result<DwarfTypeId, DwarfPrepError> {
        let byte_size = checked_u8("tag_width", width)?;
        self.lower_base_type(
            format!("u{}", u16::from(byte_size) * 8),
            DwarfScalarEncoding::Unsigned,
            byte_size,
        )
    }

    fn descriptor_key(
        &self,
        descriptor: &Descriptor<SchemaRef>,
    ) -> Result<DescriptorKey, DwarfPrepError> {
        let name = self.checked_schema_name(&descriptor.schema)?;
        let layout = layout_key(descriptor.layout);
        Ok(match &descriptor.access {
            Access::Scalar => DescriptorKey::Base {
                name,
                encoding: DwarfScalarEncoding::Unsigned,
                byte_size: checked_u8("scalar_byte_size", descriptor.layout.size)?,
            },
            Access::Record(record) => DescriptorKey::Record {
                name,
                layout,
                fields: self.record_field_keys(&record.fields)?,
            },
            Access::Enum(enum_access) => DescriptorKey::Enum {
                name,
                layout,
                tag: tag_key(&enum_access.tag),
                variants: self.variant_keys(enum_access)?,
            },
            Access::Option(option) => DescriptorKey::Option {
                name,
                layout,
                presence: presence_key(&option.presence),
                some: Box::new(self.descriptor_key(&option.some)?),
            },
            Access::Array {
                element,
                count,
                stride,
            } => DescriptorKey::Array {
                name,
                layout,
                element: Box::new(self.descriptor_key(element)?),
                count: *count,
                stride: *stride,
            },
            Access::Handle { target } => DescriptorKey::Handle {
                name,
                layout,
                target: self.checked_schema_name(target)?,
            },
            Access::Tensor(tensor) => DescriptorKey::Tensor {
                name,
                layout,
                element: Box::new(self.descriptor_key(&tensor.element)?),
                data: sequence_storage_key(&tensor.data),
                shape: tensor.shape.name.clone(),
                reshape: tensor.reshape.name.clone(),
            },
            Access::Sequence(sequence) => DescriptorKey::Sequence {
                name,
                layout,
                element: Box::new(self.descriptor_key(&sequence.element)?),
                storage: sequence_storage_key(&sequence.storage),
            },
            Access::Set(set) => DescriptorKey::Set {
                name,
                layout,
                element: Box::new(self.descriptor_key(&set.element)?),
                storage: "vtable",
            },
            Access::Map(map) => DescriptorKey::Map {
                name,
                layout,
                key: Box::new(self.descriptor_key(&map.key)?),
                value: Box::new(self.descriptor_key(&map.value)?),
                storage: map_storage_key(&map.storage),
            },
            Access::Result(result) => DescriptorKey::Result {
                name,
                layout,
                ok: Box::new(self.descriptor_key(&result.ok)?),
                err: Box::new(self.descriptor_key(&result.err)?),
            },
            Access::Pointer(pointer) => DescriptorKey::Pointer {
                name,
                layout,
                pointee: Box::new(self.descriptor_key(&pointer.pointee)?),
            },
            Access::Dynamic => DescriptorKey::Dynamic { name, layout },
            Access::Opaque(_) => DescriptorKey::Opaque { name, layout },
            Access::Recurse => DescriptorKey::Recurse { name, layout },
        })
    }

    fn record_field_keys(
        &self,
        fields: &[FieldAccess<SchemaRef>],
    ) -> Result<Vec<FieldKey>, DwarfPrepError> {
        let mut keys = Vec::with_capacity(fields.len());
        for field in fields {
            keys.push(FieldKey {
                offset: field.offset,
                descriptor: self.descriptor_key(&field.descriptor)?,
            });
        }
        Ok(keys)
    }

    fn variant_keys(
        &self,
        enum_access: &EnumAccess<SchemaRef>,
    ) -> Result<Vec<VariantKey>, DwarfPrepError> {
        let mut keys = Vec::with_capacity(enum_access.variants.len());
        for variant in &enum_access.variants {
            keys.push(VariantKey {
                index: variant.index,
                selector: variant.selector,
                payload_fields: self.record_field_keys(&variant.payload.fields)?,
            });
        }
        Ok(keys)
    }

    fn checked_schema_name(&self, schema: &SchemaRef) -> Result<String, DwarfPrepError> {
        let name = (self.schema_name)(schema);
        reject_nul(&name, "type_name")?;
        Ok(name)
    }
}

fn layout_key(layout: Layout) -> LayoutKey {
    LayoutKey {
        size: layout.size,
        align: layout.align,
    }
}

fn tag_key(tag: &Tag) -> TagKey {
    match tag {
        Tag::Direct { offset, width } => TagKey::Direct {
            offset: *offset,
            width: *width,
        },
        Tag::Niche { offset, width } => TagKey::Niche {
            offset: *offset,
            width: *width,
        },
        Tag::Thunk { read, write } => TagKey::Thunk {
            read: read.name.clone(),
            write: write.name.clone(),
        },
    }
}

fn presence_key(presence: &Presence) -> PresenceKey {
    match presence {
        Presence::Tag {
            offset,
            width,
            none_value,
        } => PresenceKey::Tag {
            offset: *offset,
            width: *width,
            none_value: *none_value,
        },
        Presence::Niche {
            offset,
            width,
            none_pattern,
        } => PresenceKey::Niche {
            offset: *offset,
            width: *width,
            none_pattern: none_pattern.clone(),
        },
        Presence::Thunk {
            is_some,
            set_none,
            set_some,
        } => PresenceKey::Thunk {
            is_some: is_some.name.clone(),
            set_none: set_none.name.clone(),
            set_some: set_some.name.clone(),
        },
        Presence::Vtable(_) => PresenceKey::Vtable,
    }
}

fn sequence_storage_key(storage: &SequenceStorage) -> SequenceStorageKey {
    match storage {
        SequenceStorage::Owned {
            ptr_offset,
            len_offset,
            cap_offset,
            allocate,
        } => SequenceStorageKey::Owned {
            ptr_offset: *ptr_offset,
            len_offset: *len_offset,
            cap_offset: *cap_offset,
            allocate: allocate.name.clone(),
        },
        SequenceStorage::Borrowed {
            ptr_offset,
            len_offset,
        } => SequenceStorageKey::Borrowed {
            ptr_offset: *ptr_offset,
            len_offset: *len_offset,
        },
        SequenceStorage::Thunk { len, get, push } => SequenceStorageKey::Thunk {
            len: len.name.clone(),
            get: get.name.clone(),
            push: push.name.clone(),
        },
        SequenceStorage::Vtable(_) => SequenceStorageKey::Vtable,
        SequenceStorage::BorrowedVtable(_) => SequenceStorageKey::BorrowedVtable,
    }
}

fn map_storage_key(storage: &MapStorage) -> MapStorageKey {
    match storage {
        MapStorage::Thunk {
            len,
            iterate,
            insert,
        } => MapStorageKey::Thunk {
            len: len.name.clone(),
            iterate: iterate.name.clone(),
            insert: insert.name.clone(),
        },
        MapStorage::Vtable(_) => MapStorageKey::Vtable,
    }
}

fn checked_u8(field: &'static str, value: usize) -> Result<u8, DwarfPrepError> {
    u8::try_from(value).map_err(|_| DwarfPrepError::TypeValueOutOfBounds { field, value })
}

fn checked_u32(field: &'static str, value: usize) -> Result<u32, DwarfPrepError> {
    u32::try_from(value).map_err(|_| DwarfPrepError::TypeValueOutOfBounds { field, value })
}

fn reject_nul(value: &str, field: &'static str) -> Result<(), DwarfPrepError> {
    if value.as_bytes().contains(&0) {
        Err(DwarfPrepError::InteriorNul { field })
    } else {
        Ok(())
    }
}

/// Build DWARF sections for one JIT function.
///
/// `source_map` entries are `(code_offset, line_index, column)`:
/// - address: `code_address + code_offset`
/// - line: `line_index + 1`
/// - column: 1-based; 0 = no column info
pub fn build_jit_dwarf_sections(
    code_address: u64,
    code_size: u64,
    source_map: &[(u32, u32, u32)],
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
    source_map: &[(u32, u32, u32)],
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
                .map(|(code_offset, line_index, column)| JitDebugLineRow {
                    code_offset: *code_offset,
                    line: line_index.saturating_add(1),
                    column: *column,
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

/// Build DWARF sections with optional structural type DIEs for subprogram variables.
///
/// The `schema_name` callback is the only naming hook because `Descriptor` carries schema identity
/// and byte offsets, not source field/variant labels. Type DIE `DW_AT_name` values come from the
/// callback; record members and enum variants use stable synthetic names (`field0`, `variant0`, ...).
/// Direct enum tags are emitted as a synthetic `__tag` member. Niche tags are emitted the same way
/// at the recorded niche byte range as `__niche`, with the variant selectors as discriminant values.
pub fn build_jit_dwarf_sections_with_variable_descriptors<SchemaRef, NameFn>(
    target_arch: DwarfTargetArch,
    code_address: u64,
    code_size: u64,
    source_map: &[(u32, u32, u32)],
    file_name: &str,
    directory: Option<&str>,
    subprogram_name: &str,
    variables: &[DwarfVariable],
    variable_descriptors: &[Option<&Descriptor<SchemaRef>>],
    schema_name: NameFn,
) -> Result<JitDwarfSections, DwarfPrepError>
where
    NameFn: Fn(&SchemaRef) -> String,
{
    if variables.len() != variable_descriptors.len() {
        return Err(DwarfPrepError::VariableTypeCountMismatch {
            variables: variables.len(),
            variable_descriptors: variable_descriptors.len(),
        });
    }
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

    let mut lowerer = DescriptorTypeLowerer::new(&schema_name);
    let mut variable_type_ids = Vec::with_capacity(variable_descriptors.len());
    for descriptor in variable_descriptors {
        variable_type_ids.push(match descriptor {
            Some(descriptor) => Some(lowerer.lower(descriptor)?),
            None => None,
        });
    }
    let type_graph = lowerer.into_graph();

    let debug_info = JitDebugInfo {
        target_arch,
        code_address,
        code_size,
        line_table: JitDebugLineTable {
            file_name: file_name.to_owned(),
            directory: directory.map(ToOwned::to_owned),
            rows: source_map
                .iter()
                .map(|(code_offset, line_index, column)| JitDebugLineRow {
                    code_offset: *code_offset,
                    line: line_index.saturating_add(1),
                    column: *column,
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
    build_jit_dwarf_sections_from_debug_info_with_type_graph(
        &debug_info,
        &type_graph,
        &variable_type_ids,
    )
}

pub fn build_jit_dwarf_sections_from_debug_info(
    debug_info: &JitDebugInfo,
) -> Result<JitDwarfSections, DwarfPrepError> {
    build_jit_dwarf_sections_from_debug_info_with_type_graph(
        debug_info,
        &JitDebugTypeGraph::default(),
        &[],
    )
}

fn build_jit_dwarf_sections_from_debug_info_with_type_graph(
    debug_info: &JitDebugInfo,
    type_graph: &JitDebugTypeGraph,
    variable_type_ids: &[Option<DwarfTypeId>],
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
    let debug_info_section = build_debug_info_section_from_debug_info_with_types(
        debug_info,
        &variable_loc_offsets,
        &lexical_block_range_offsets,
        type_graph,
        variable_type_ids,
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
    debug_info_section: &[u8],
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

    if let Some(offset) =
        find_subprogram_low_pc_offset(debug_info_section, &debug_info.subprogram.name)
    {
        relocs.push((
            DwarfSection::DebugInfo,
            DwarfRelocation { offset, addend: 0 },
        ));
    }

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

fn find_subprogram_low_pc_offset(debug_info_section: &[u8], subprogram_name: &str) -> Option<u32> {
    let needle = subprogram_name.as_bytes();
    let mut i = 11usize;
    while i < debug_info_section.len() {
        if debug_info_section[i] == 3 {
            let name_start = i + 1;
            if name_start <= debug_info_section.len()
                && debug_info_section[name_start..].starts_with(needle)
            {
                let name_end = name_start + needle.len();
                if debug_info_section.get(name_end).copied() == Some(0) {
                    return u32::try_from(name_end + 1).ok();
                }
            }
        }
        i += 1;
    }
    None
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

    // Abbrev 7: structure type (records, enums, and opaque sized blobs).
    push_uleb128(&mut out, 7);
    out.push(DW_TAG_STRUCTURE_TYPE);
    out.push(DW_CHILDREN_YES);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(DW_AT_BYTE_SIZE);
    out.push(DW_FORM_DATA4);
    out.push(0);
    out.push(0);

    // Abbrev 8: structure/variant member.
    push_uleb128(&mut out, 8);
    out.push(DW_TAG_MEMBER);
    out.push(DW_CHILDREN_NO);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(DW_AT_TYPE);
    out.push(DW_FORM_REF4);
    out.push(DW_AT_DATA_MEMBER_LOCATION);
    out.push(DW_FORM_DATA4);
    out.push(0);
    out.push(0);

    // Abbrev 9: enum variant part with a discriminant member reference.
    push_uleb128(&mut out, 9);
    out.push(DW_TAG_VARIANT_PART);
    out.push(DW_CHILDREN_YES);
    out.push(DW_AT_DISCR);
    out.push(DW_FORM_REF4);
    out.push(0);
    out.push(0);

    // Abbrev 10: one variant arm.
    push_uleb128(&mut out, 10);
    out.push(DW_TAG_VARIANT);
    out.push(DW_CHILDREN_YES);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(DW_AT_DISCR_VALUE);
    out.push(DW_FORM_DATA8);
    out.push(0);
    out.push(0);

    // Abbrev 11: fixed-size array type.
    push_uleb128(&mut out, 11);
    out.push(DW_TAG_ARRAY_TYPE);
    out.push(DW_CHILDREN_YES);
    out.push(DW_AT_NAME);
    out.push(DW_FORM_STRING);
    out.push(DW_AT_TYPE);
    out.push(DW_FORM_REF4);
    out.push(DW_AT_BYTE_SIZE);
    out.push(DW_FORM_DATA4);
    out.push(0);
    out.push(0);

    // Abbrev 12: array subrange with count and byte stride.
    push_uleb128(&mut out, 12);
    out.push(DW_TAG_SUBRANGE_TYPE);
    out.push(DW_CHILDREN_NO);
    out.push(DW_AT_COUNT);
    out.push(DW_FORM_DATA4);
    out.push(DW_AT_BYTE_STRIDE);
    out.push(DW_FORM_DATA4);
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
    build_debug_info_section_with_types(
        code_address,
        code_size,
        file_name,
        subprogram_name,
        frame_base_expr,
        variables,
        lexical_blocks,
        variable_loc_offsets,
        lexical_block_range_offsets,
        &JitDebugTypeGraph::default(),
        &[],
    )
}

fn build_debug_info_section_with_types(
    code_address: u64,
    code_size: u64,
    file_name: &str,
    subprogram_name: &str,
    frame_base_expr: &[u8],
    variables: &[DwarfVariable],
    lexical_blocks: &[JitDebugLexicalBlock],
    variable_loc_offsets: &[u32],
    lexical_block_range_offsets: &[u32],
    type_graph: &JitDebugTypeGraph,
    variable_type_ids: &[Option<DwarfTypeId>],
) -> Vec<u8> {
    let mut die = Vec::new();
    // Compile unit DIE (abbrev 1).
    push_uleb128(&mut die, 1);
    die.extend_from_slice(&0u32.to_le_bytes()); // DW_AT_stmt_list -> .debug_line offset 0
    die.extend_from_slice(&code_address.to_le_bytes()); // DW_AT_low_pc
    die.extend_from_slice(&code_size.to_le_bytes()); // DW_AT_high_pc as length (DW_FORM_data8)
    die.extend_from_slice(file_name.as_bytes()); // DW_AT_name
    die.push(0);

    // Fallback base type DIE (abbrev 2), used by variables without a descriptor.
    let base_type_die_offset = 11u32 + (die.len() as u32);
    push_uleb128(&mut die, 2);
    die.extend_from_slice(b"u64");
    die.push(0);
    die.push(DW_ATE_UNSIGNED);
    die.push(8);

    let type_offsets = emit_type_graph_dies(&mut die, type_graph);
    let variable_type_offsets: Vec<Option<u32>> = variable_type_ids
        .iter()
        .map(|id| id.and_then(|id| type_offsets.get(id.0).copied()))
        .collect();

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
    let mut next_variable_type = 0usize;
    emit_variable_dies(
        &mut die,
        variables,
        lexical_blocks,
        variable_loc_offsets,
        lexical_block_range_offsets,
        &mut next_loc_offset,
        &mut next_range_offset,
        &mut next_variable_type,
        base_type_die_offset,
        &variable_type_offsets,
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

fn emit_type_graph_dies(die: &mut Vec<u8>, type_graph: &JitDebugTypeGraph) -> Vec<u32> {
    let first_type_offset = 11u32 + die.len() as u32;
    let zero_offsets = vec![0u32; type_graph.types.len()];
    let mut sizing = Vec::new();
    let mut offsets = Vec::with_capacity(type_graph.types.len());
    for ty in &type_graph.types {
        offsets.push(first_type_offset + sizing.len() as u32);
        emit_type_die(&mut sizing, ty, &zero_offsets, first_type_offset);
    }
    for ty in &type_graph.types {
        emit_type_die(die, ty, &offsets, 11);
    }
    offsets
}

fn emit_type_die(
    die: &mut Vec<u8>,
    ty: &DwarfTypeDie,
    type_offsets: &[u32],
    section_offset_base: u32,
) {
    match ty {
        DwarfTypeDie::Base(base) => {
            push_uleb128(die, 2);
            die.extend_from_slice(base.name.as_bytes());
            die.push(0);
            die.push(base.encoding.dwarf_encoding());
            die.push(base.byte_size);
        }
        DwarfTypeDie::Structure(structure) => {
            push_uleb128(die, 7);
            die.extend_from_slice(structure.name.as_bytes());
            die.push(0);
            die.extend_from_slice(&structure.byte_size.to_le_bytes());
            let mut member_offsets = Vec::with_capacity(structure.members.len());
            for member in &structure.members {
                member_offsets.push(emit_member_die(
                    die,
                    member,
                    type_offsets,
                    section_offset_base,
                ));
            }
            if let Some(variant_part) = &structure.variant_part {
                emit_variant_part_die(
                    die,
                    variant_part,
                    &member_offsets,
                    type_offsets,
                    section_offset_base,
                );
            }
            die.push(0);
        }
        DwarfTypeDie::Array(array) => {
            push_uleb128(die, 11);
            die.extend_from_slice(array.name.as_bytes());
            die.push(0);
            die.extend_from_slice(&type_ref(array.element_type, type_offsets).to_le_bytes());
            die.extend_from_slice(&array.byte_size.to_le_bytes());
            push_uleb128(die, 12);
            die.extend_from_slice(&array.count.to_le_bytes());
            die.extend_from_slice(&array.stride.to_le_bytes());
            die.push(0);
        }
    }
}

fn emit_member_die(
    die: &mut Vec<u8>,
    member: &DwarfMember,
    type_offsets: &[u32],
    section_offset_base: u32,
) -> u32 {
    let offset = section_offset_base + die.len() as u32;
    push_uleb128(die, 8);
    die.extend_from_slice(member.name.as_bytes());
    die.push(0);
    die.extend_from_slice(&type_ref(member.type_id, type_offsets).to_le_bytes());
    die.extend_from_slice(&member.offset.to_le_bytes());
    offset
}

fn emit_variant_part_die(
    die: &mut Vec<u8>,
    variant_part: &DwarfVariantPart,
    member_offsets: &[u32],
    type_offsets: &[u32],
    section_offset_base: u32,
) {
    push_uleb128(die, 9);
    let discr_offset = member_offsets
        .get(variant_part.discriminant_member)
        .copied()
        .unwrap_or(0);
    die.extend_from_slice(&discr_offset.to_le_bytes());
    for variant in &variant_part.variants {
        push_uleb128(die, 10);
        die.extend_from_slice(variant.name.as_bytes());
        die.push(0);
        die.extend_from_slice(&variant.discriminant.to_le_bytes());
        for member in &variant.members {
            emit_member_die(die, member, type_offsets, section_offset_base);
        }
        die.push(0);
    }
    die.push(0);
}

fn type_ref(id: DwarfTypeId, type_offsets: &[u32]) -> u32 {
    type_offsets.get(id.0).copied().unwrap_or(0)
}

fn emit_variable_dies(
    die: &mut Vec<u8>,
    variables: &[DwarfVariable],
    lexical_blocks: &[JitDebugLexicalBlock],
    variable_loc_offsets: &[u32],
    lexical_block_range_offsets: &[u32],
    next_loc_offset: &mut usize,
    next_range_offset: &mut usize,
    next_variable_type: &mut usize,
    base_type_die_offset: u32,
    variable_type_offsets: &[Option<u32>],
) {
    for variable in variables {
        let type_offset = variable_type_offsets
            .get(*next_variable_type)
            .copied()
            .flatten()
            .unwrap_or(base_type_die_offset);
        *next_variable_type += 1;
        match &variable.location {
            DwarfVariableLocation::Expr(expr) => {
                push_uleb128(die, 4);
                die.extend_from_slice(variable.name.as_bytes());
                die.push(0);
                push_uleb128(die, expr.len() as u64);
                die.extend_from_slice(expr);
                die.extend_from_slice(&type_offset.to_le_bytes());
            }
            DwarfVariableLocation::List(_ranges) => {
                let loc_offset = variable_loc_offsets[*next_loc_offset];
                *next_loc_offset += 1;
                push_uleb128(die, 5);
                die.extend_from_slice(variable.name.as_bytes());
                die.push(0);
                die.extend_from_slice(&loc_offset.to_le_bytes());
                die.extend_from_slice(&type_offset.to_le_bytes());
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
            next_variable_type,
            base_type_die_offset,
            variable_type_offsets,
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
    next_variable_type: &mut usize,
    base_type_die_offset: u32,
    variable_type_offsets: &[Option<u32>],
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
        next_variable_type,
        base_type_die_offset,
        variable_type_offsets,
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
    build_debug_info_section_from_debug_info_with_types(
        debug_info,
        variable_loc_offsets,
        lexical_block_range_offsets,
        &JitDebugTypeGraph::default(),
        &[],
    )
}

fn build_debug_info_section_from_debug_info_with_types(
    debug_info: &JitDebugInfo,
    variable_loc_offsets: &[u32],
    lexical_block_range_offsets: &[u32],
    type_graph: &JitDebugTypeGraph,
    variable_type_ids: &[Option<DwarfTypeId>],
) -> Vec<u8> {
    build_debug_info_section_with_types(
        debug_info.code_address,
        debug_info.code_size,
        &debug_info.line_table.file_name,
        &debug_info.subprogram.name,
        &debug_info.subprogram.frame_base_expression,
        &debug_info.subprogram.variables,
        &debug_info.subprogram.lexical_blocks,
        variable_loc_offsets,
        lexical_block_range_offsets,
        type_graph,
        variable_type_ids,
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
    source_map: &[(u32, u32, u32)],
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

    for (offset, _, _) in source_map {
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
    let mut current_column = 0u64; // DWARF line-machine initial column register

    for (offset, line_index, column) in source_map {
        let offset = *offset as u64;
        let line = (*line_index as i64) + 1;
        let column = *column as u64;

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

        if column != current_column {
            program.push(DW_LNS_SET_COLUMN);
            push_uleb128(&mut program, column);
            current_column = column;
        }

        // Every row is a recommended breakpoint location: JIT stencils have no prologue, so
        // without this debuggers "skip the prologue" past a region's first instruction (lldb
        // otherwise needs `breakpoint set ... -K false`).
        program.push(DW_LNS_SET_PROLOGUE_END);
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
        .map(|row| (row.code_offset, row.line.saturating_sub(1), row.column))
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
    use crate::mem::{
        Access, Construct, Descriptor, EnumAccess, FieldAccess, Layout, RecordAccess,
        RecordByteOwnership, Tag, VariantAccess,
    };

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

    fn parse_debug_line_rows(section: &[u8]) -> Vec<(u64, i64, u64)> {
        let unit_length = u32::from_le_bytes(section[0..4].try_into().unwrap()) as usize;
        let version = u16::from_le_bytes(section[4..6].try_into().unwrap());
        assert_eq!(version, 4);
        let header_length = u32::from_le_bytes(section[6..10].try_into().unwrap()) as usize;
        let mut program_i = 10 + header_length;
        assert_eq!(section.len(), 4 + unit_length);

        let mut rows = Vec::new();
        let mut address = 0u64;
        let mut line = 1i64;
        let mut column = 0u64;

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
                DW_LNS_SET_COLUMN => {
                    column = parse_uleb(section, &mut program_i);
                }
                DW_LNS_SET_PROLOGUE_END => {}
                DW_LNS_COPY => rows.push((address, line, column)),
                other => panic!("unexpected standard opcode {other}"),
            }
        }

        rows
    }

    #[derive(Debug)]
    struct ParsedAbbrev {
        tag: u64,
        has_children: bool,
        attrs: Vec<(u64, u64)>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ParsedAttr {
        Addr(u64),
        Data1(u8),
        Data4(u32),
        Data8(u64),
        String(String),
        Ref4(u32),
        SecOffset(u32),
        Expr(Vec<u8>),
    }

    #[derive(Debug)]
    struct ParsedDie {
        offset: u32,
        abbrev: u64,
        tag: u64,
        attrs: Vec<(u64, ParsedAttr)>,
        children: Vec<ParsedDie>,
    }

    impl ParsedDie {
        fn attr(&self, attr: u8) -> &ParsedAttr {
            self.attrs
                .iter()
                .find_map(|(name, value)| (*name == u64::from(attr)).then_some(value))
                .unwrap_or_else(|| panic!("missing attr {attr:#x} on DIE {self:?}"))
        }

        fn name(&self) -> &str {
            match self.attr(DW_AT_NAME) {
                ParsedAttr::String(name) => name,
                other => panic!("DW_AT_name was {other:?}"),
            }
        }

        fn data1(&self, attr: u8) -> u8 {
            match self.attr(attr) {
                ParsedAttr::Data1(value) => *value,
                other => panic!("attr {attr:#x} was {other:?}"),
            }
        }

        fn data4(&self, attr: u8) -> u32 {
            match self.attr(attr) {
                ParsedAttr::Data4(value) => *value,
                other => panic!("attr {attr:#x} was {other:?}"),
            }
        }

        fn data8(&self, attr: u8) -> u64 {
            match self.attr(attr) {
                ParsedAttr::Data8(value) => *value,
                other => panic!("attr {attr:#x} was {other:?}"),
            }
        }

        fn ref4(&self, attr: u8) -> u32 {
            match self.attr(attr) {
                ParsedAttr::Ref4(value) => *value,
                other => panic!("attr {attr:#x} was {other:?}"),
            }
        }
    }

    fn parse_debug_abbrev(section: &[u8]) -> BTreeMap<u64, ParsedAbbrev> {
        let mut i = 0usize;
        let mut abbrevs = BTreeMap::new();
        loop {
            let code = parse_uleb(section, &mut i);
            if code == 0 {
                break;
            }
            let tag = parse_uleb(section, &mut i);
            let has_children = section[i] == DW_CHILDREN_YES;
            i += 1;
            let mut attrs = Vec::new();
            loop {
                let name = parse_uleb(section, &mut i);
                let form = parse_uleb(section, &mut i);
                if name == 0 && form == 0 {
                    break;
                }
                attrs.push((name, form));
            }
            abbrevs.insert(
                code,
                ParsedAbbrev {
                    tag,
                    has_children,
                    attrs,
                },
            );
        }
        abbrevs
    }

    fn parse_debug_info(section: &[u8], abbrevs: &BTreeMap<u64, ParsedAbbrev>) -> ParsedDie {
        let unit_length = u32::from_le_bytes(section[0..4].try_into().unwrap()) as usize;
        assert_eq!(section.len(), unit_length + 4);
        assert_eq!(u16::from_le_bytes(section[4..6].try_into().unwrap()), 4);
        assert_eq!(u32::from_le_bytes(section[6..10].try_into().unwrap()), 0);
        assert_eq!(section[10], 8);
        let mut i = 11usize;
        let cu = parse_die(section, abbrevs, &mut i).expect("compile unit DIE");
        assert_eq!(i, section.len());
        cu
    }

    fn parse_die(
        section: &[u8],
        abbrevs: &BTreeMap<u64, ParsedAbbrev>,
        i: &mut usize,
    ) -> Option<ParsedDie> {
        let offset = *i as u32;
        let abbrev = parse_uleb(section, i);
        if abbrev == 0 {
            return None;
        }
        let spec = abbrevs
            .get(&abbrev)
            .unwrap_or_else(|| panic!("missing abbrev {abbrev}"));
        let mut attrs = Vec::with_capacity(spec.attrs.len());
        for (name, form) in &spec.attrs {
            attrs.push((*name, parse_attr(section, i, *form)));
        }
        let mut children = Vec::new();
        if spec.has_children {
            while let Some(child) = parse_die(section, abbrevs, i) {
                children.push(child);
            }
        }
        Some(ParsedDie {
            offset,
            abbrev,
            tag: spec.tag,
            attrs,
            children,
        })
    }

    fn parse_attr(section: &[u8], i: &mut usize, form: u64) -> ParsedAttr {
        match form as u8 {
            DW_FORM_ADDR => {
                let value = u64::from_le_bytes(section[*i..*i + 8].try_into().unwrap());
                *i += 8;
                ParsedAttr::Addr(value)
            }
            DW_FORM_DATA1 => {
                let value = section[*i];
                *i += 1;
                ParsedAttr::Data1(value)
            }
            DW_FORM_DATA4 => {
                let value = u32::from_le_bytes(section[*i..*i + 4].try_into().unwrap());
                *i += 4;
                ParsedAttr::Data4(value)
            }
            DW_FORM_DATA8 => {
                let value = u64::from_le_bytes(section[*i..*i + 8].try_into().unwrap());
                *i += 8;
                ParsedAttr::Data8(value)
            }
            DW_FORM_STRING => {
                let end = section[*i..].iter().position(|b| *b == 0).unwrap();
                let value = String::from_utf8(section[*i..*i + end].to_vec()).unwrap();
                *i += end + 1;
                ParsedAttr::String(value)
            }
            DW_FORM_REF4 => {
                let value = u32::from_le_bytes(section[*i..*i + 4].try_into().unwrap());
                *i += 4;
                ParsedAttr::Ref4(value)
            }
            DW_FORM_SEC_OFFSET => {
                let value = u32::from_le_bytes(section[*i..*i + 4].try_into().unwrap());
                *i += 4;
                ParsedAttr::SecOffset(value)
            }
            DW_FORM_EXPRLOC => {
                let len = parse_uleb(section, i) as usize;
                let value = section[*i..*i + len].to_vec();
                *i += len;
                ParsedAttr::Expr(value)
            }
            other => panic!("unsupported form {other:#x}"),
        }
    }

    fn find_die_by_offset(die: &ParsedDie, offset: u32) -> Option<&ParsedDie> {
        if die.offset == offset {
            return Some(die);
        }
        die.children
            .iter()
            .find_map(|child| find_die_by_offset(child, offset))
    }

    fn find_child_by_name<'a>(die: &'a ParsedDie, tag: u8, name: &str) -> &'a ParsedDie {
        die.children
            .iter()
            .find(|child| child.tag == u64::from(tag) && child.name() == name)
            .unwrap_or_else(|| panic!("missing child tag {tag:#x} name {name} in {die:?}"))
    }

    fn children_with_tag(die: &ParsedDie, tag: u8) -> Vec<&ParsedDie> {
        die.children
            .iter()
            .filter(|child| child.tag == u64::from(tag))
            .collect()
    }

    fn parsed_typed_info(descriptor: &Descriptor<&'static str>) -> ParsedDie {
        let variables = [DwarfVariable {
            name: "value".to_string(),
            location: DwarfVariableLocation::Expr(expr_reg(0)),
        }];
        let descriptors = [Some(descriptor)];
        let sections = build_jit_dwarf_sections_with_variable_descriptors(
            DwarfTargetArch::X86_64,
            0x1000,
            8,
            &[(0, 0, 0)],
            "decoder.ra",
            None,
            "jit::typed",
            &variables,
            &descriptors,
            |schema| (*schema).to_string(),
        )
        .unwrap();
        let abbrevs = parse_debug_abbrev(&sections.debug_abbrev);
        parse_debug_info(&sections.debug_info, &abbrevs)
    }

    fn scalar_desc(schema: &'static str, size: usize, align: usize) -> Descriptor<&'static str> {
        Descriptor {
            schema,
            layout: Layout { size, align },
            access: Access::Scalar,
        }
    }

    fn recurse_desc(schema: &'static str, size: usize, align: usize) -> Descriptor<&'static str> {
        Descriptor {
            schema,
            layout: Layout { size, align },
            access: Access::Recurse,
        }
    }

    fn field(offset: usize, descriptor: Descriptor<&'static str>) -> FieldAccess<&'static str> {
        FieldAccess {
            offset,
            descriptor,
            default: None,
        }
    }

    fn record_access(fields: Vec<FieldAccess<&'static str>>) -> RecordAccess<&'static str> {
        RecordAccess {
            fields,
            byte_ownership: RecordByteOwnership::default(),
            construct: Construct::InPlace,
        }
    }

    fn record_desc(
        schema: &'static str,
        size: usize,
        align: usize,
        fields: Vec<FieldAccess<&'static str>>,
    ) -> Descriptor<&'static str> {
        Descriptor {
            schema,
            layout: Layout { size, align },
            access: Access::Record(record_access(fields)),
        }
    }

    fn variable_type_ref(cu: &ParsedDie) -> u32 {
        let subprogram = cu
            .children
            .iter()
            .find(|child| child.tag == u64::from(DW_TAG_SUBPROGRAM))
            .expect("subprogram DIE");
        let variable = find_child_by_name(subprogram, DW_TAG_VARIABLE, "value");
        variable.ref4(DW_AT_TYPE)
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
            &[(0, 0, 0), (4, 3, 0), (9, 7, 0)],
            "decoder.ra",
            Some("jit"),
        )
        .unwrap();

        let rows = parse_debug_line_rows(&section);
        assert_eq!(rows, vec![(0x1000, 1, 0), (0x1004, 4, 0), (0x1009, 8, 0)]);
    }

    #[test]
    fn debug_line_rows_carry_columns() {
        // Several sub-expressions of ONE source line, pinned by column (the template case).
        let section = build_debug_line_section(
            0x1000,
            12,
            &[(0, 0, 4), (4, 0, 8), (9, 0, 4)],
            "template.jinja",
            None,
        )
        .unwrap();

        let rows = parse_debug_line_rows(&section);
        assert_eq!(rows, vec![(0x1000, 1, 4), (0x1004, 1, 8), (0x1009, 1, 4)]);
    }

    #[test]
    fn debug_line_allows_empty_source_map() {
        let section = build_debug_line_section(0x2000, 5, &[], "decoder.ra", None).unwrap();
        let rows = parse_debug_line_rows(&section);
        assert!(rows.is_empty());
    }

    #[test]
    fn rejects_invalid_source_map_and_inputs() {
        let err = build_debug_line_section(0, 8, &[(4, 1, 0), (4, 2, 0)], "f", None).unwrap_err();
        assert!(matches!(
            err,
            DwarfPrepError::SourceMapNotStrictlyIncreasing {
                previous_offset: 4,
                next_offset: 4
            }
        ));

        let err = build_debug_line_section(0, 7, &[(8, 0, 0)], "f", None).unwrap_err();
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
    fn descriptor_record_lowers_to_structure_members() {
        let pair = record_desc(
            "Pair",
            12,
            4,
            vec![
                field(0, scalar_desc("u32", 4, 4)),
                field(8, scalar_desc("u16", 2, 2)),
            ],
        );

        let cu = parsed_typed_info(&pair);
        let pair_die = find_child_by_name(&cu, DW_TAG_STRUCTURE_TYPE, "Pair");
        assert_eq!(pair_die.abbrev, 7);
        assert_eq!(pair_die.data4(DW_AT_BYTE_SIZE), 12);
        assert_eq!(variable_type_ref(&cu), pair_die.offset);

        let members = children_with_tag(pair_die, DW_TAG_MEMBER);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name(), "field0");
        assert_eq!(members[0].data4(DW_AT_DATA_MEMBER_LOCATION), 0);
        assert_eq!(members[1].name(), "field1");
        assert_eq!(members[1].data4(DW_AT_DATA_MEMBER_LOCATION), 8);

        let u32_die = find_die_by_offset(&cu, members[0].ref4(DW_AT_TYPE)).unwrap();
        assert_eq!(u32_die.tag, u64::from(DW_TAG_BASE_TYPE));
        assert_eq!(u32_die.name(), "u32");
        assert_eq!(u32_die.data1(DW_AT_BYTE_SIZE), 4);
    }

    #[test]
    fn descriptor_nested_record_uses_member_type_references() {
        let inner = record_desc("Inner", 4, 4, vec![field(0, scalar_desc("u32", 4, 4))]);
        let outer = record_desc(
            "Outer",
            16,
            8,
            vec![field(0, scalar_desc("u8", 1, 1)), field(8, inner)],
        );

        let cu = parsed_typed_info(&outer);
        let outer_die = find_child_by_name(&cu, DW_TAG_STRUCTURE_TYPE, "Outer");
        let inner_die = find_child_by_name(&cu, DW_TAG_STRUCTURE_TYPE, "Inner");
        let outer_members = children_with_tag(outer_die, DW_TAG_MEMBER);
        assert_eq!(outer_members[1].name(), "field1");
        assert_eq!(outer_members[1].data4(DW_AT_DATA_MEMBER_LOCATION), 8);
        assert_eq!(outer_members[1].ref4(DW_AT_TYPE), inner_die.offset);

        let inner_members = children_with_tag(inner_die, DW_TAG_MEMBER);
        assert_eq!(inner_members.len(), 1);
        assert_eq!(inner_members[0].name(), "field0");
        assert_eq!(inner_members[0].data4(DW_AT_DATA_MEMBER_LOCATION), 0);
    }

    #[test]
    fn descriptor_enum_lowers_to_variant_part_with_discriminants() {
        let choice = Descriptor {
            schema: "Choice",
            layout: Layout { size: 16, align: 8 },
            access: Access::Enum(EnumAccess {
                tag: Tag::Direct {
                    offset: 0,
                    width: 1,
                },
                variants: vec![
                    VariantAccess {
                        index: 0,
                        selector: 0,
                        payload: record_access(vec![field(8, scalar_desc("u64", 8, 8))]),
                    },
                    VariantAccess {
                        index: 1,
                        selector: 7,
                        payload: record_access(vec![
                            field(8, scalar_desc("u32", 4, 4)),
                            field(12, scalar_desc("u32", 4, 4)),
                        ]),
                    },
                ],
            }),
        };

        let cu = parsed_typed_info(&choice);
        let choice_die = find_child_by_name(&cu, DW_TAG_STRUCTURE_TYPE, "Choice");
        assert_eq!(choice_die.data4(DW_AT_BYTE_SIZE), 16);
        let tag_member = find_child_by_name(choice_die, DW_TAG_MEMBER, "__tag");
        assert_eq!(tag_member.data4(DW_AT_DATA_MEMBER_LOCATION), 0);

        let variant_part = children_with_tag(choice_die, DW_TAG_VARIANT_PART)
            .into_iter()
            .next()
            .expect("variant part");
        assert_eq!(variant_part.ref4(DW_AT_DISCR), tag_member.offset);
        let variants = children_with_tag(variant_part, DW_TAG_VARIANT);
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].name(), "variant0");
        assert_eq!(variants[0].data8(DW_AT_DISCR_VALUE), 0);
        assert_eq!(variants[1].name(), "variant1");
        assert_eq!(variants[1].data8(DW_AT_DISCR_VALUE), 7);

        let variant0_members = children_with_tag(variants[0], DW_TAG_MEMBER);
        assert_eq!(variant0_members[0].data4(DW_AT_DATA_MEMBER_LOCATION), 8);
        let variant1_members = children_with_tag(variants[1], DW_TAG_MEMBER);
        assert_eq!(variant1_members[0].data4(DW_AT_DATA_MEMBER_LOCATION), 8);
        assert_eq!(variant1_members[1].data4(DW_AT_DATA_MEMBER_LOCATION), 12);
    }

    #[test]
    fn descriptor_array_lowers_to_array_type_and_subrange() {
        let array = Descriptor {
            schema: "U32x3",
            layout: Layout { size: 12, align: 4 },
            access: Access::Array {
                element: Box::new(scalar_desc("u32", 4, 4)),
                count: 3,
                stride: 4,
            },
        };

        let cu = parsed_typed_info(&array);
        let array_die = find_child_by_name(&cu, DW_TAG_ARRAY_TYPE, "U32x3");
        assert_eq!(array_die.abbrev, 11);
        assert_eq!(array_die.data4(DW_AT_BYTE_SIZE), 12);
        assert_eq!(variable_type_ref(&cu), array_die.offset);
        let element = find_die_by_offset(&cu, array_die.ref4(DW_AT_TYPE)).unwrap();
        assert_eq!(element.name(), "u32");

        let subranges = children_with_tag(array_die, DW_TAG_SUBRANGE_TYPE);
        assert_eq!(subranges.len(), 1);
        assert_eq!(subranges[0].data4(DW_AT_COUNT), 3);
        assert_eq!(subranges[0].data4(DW_AT_BYTE_STRIDE), 4);
    }

    #[test]
    fn descriptor_recurse_member_refs_enclosing_structure() {
        let node = record_desc(
            "Node",
            24,
            8,
            vec![
                field(0, scalar_desc("i64", 8, 8)),
                field(8, recurse_desc("Node", 24, 8)),
            ],
        );

        let cu = parsed_typed_info(&node);
        let node_die = find_child_by_name(&cu, DW_TAG_STRUCTURE_TYPE, "Node");
        assert_eq!(variable_type_ref(&cu), node_die.offset);
        let members = children_with_tag(node_die, DW_TAG_MEMBER);
        assert_eq!(members.len(), 2);
        assert_eq!(members[1].name(), "field1");
        assert_eq!(members[1].data4(DW_AT_DATA_MEMBER_LOCATION), 8);
        assert_eq!(members[1].ref4(DW_AT_TYPE), node_die.offset);
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
            &[(0, 0, 0)],
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
            &[(0, 0, 0)],
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
                    column: 0,
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
