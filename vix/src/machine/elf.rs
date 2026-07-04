use std::collections::{BTreeMap, BTreeSet};

use crate::oracle::Value;

const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;

const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_DYNAMIC: u32 = 6;
const SHT_DYNSYM: u32 = 11;
const SHT_GNU_VERNEED: u32 = 0x6fff_fffe;

const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Projection {
    Arch,
    Kind,
    DynamicDeps,
    NeedsGlibc,
    Sections,
    Symbols,
    LinkerMetadata,
}

impl Projection {
    pub(super) const ALL: [Projection; 7] = [
        Projection::Arch,
        Projection::Kind,
        Projection::DynamicDeps,
        Projection::NeedsGlibc,
        Projection::Sections,
        Projection::Symbols,
        Projection::LinkerMetadata,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Projection::Arch => "arch",
            Projection::Kind => "kind",
            Projection::DynamicDeps => "dynamic_deps",
            Projection::NeedsGlibc => "needs_glibc",
            Projection::Sections => "sections",
            Projection::Symbols => "symbols",
            Projection::LinkerMetadata => "linker_metadata",
        }
    }

    pub(super) fn to_word(self) -> i64 {
        match self {
            Projection::Arch => 0,
            Projection::Kind => 1,
            Projection::DynamicDeps => 2,
            Projection::NeedsGlibc => 3,
            Projection::Sections => 4,
            Projection::Symbols => 5,
            Projection::LinkerMetadata => 6,
        }
    }

    pub(super) fn from_word(word: i64) -> Result<Self, String> {
        Ok(match word {
            0 => Projection::Arch,
            1 => Projection::Kind,
            2 => Projection::DynamicDeps,
            3 => Projection::NeedsGlibc,
            4 => Projection::Sections,
            5 => Projection::Symbols,
            6 => Projection::LinkerMetadata,
            other => return Err(format!("unknown elf projection {other}")),
        })
    }
}

pub(super) fn project(bytes: &[u8], projection: Projection) -> Result<Value, String> {
    match projection {
        Projection::Arch => Ok(Value::Str(parse_header(bytes)?.arch().to_string())),
        Projection::Kind => Ok(Value::Str(parse_header(bytes)?.kind().to_string())),
        Projection::DynamicDeps => Elf::parse(bytes)?.dynamic_deps(),
        Projection::NeedsGlibc => Elf::parse(bytes)?.needs_glibc(),
        Projection::Sections => Elf::parse(bytes)?.sections(),
        Projection::Symbols => Elf::parse(bytes)?.symbols(),
        Projection::LinkerMetadata => Elf::parse(bytes)?.linker_metadata(),
    }
}

#[derive(Clone, Copy)]
struct Header {
    e_type: u16,
    e_machine: u16,
    e_shoff: u64,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[derive(Clone)]
struct Section {
    name: String,
    sh_type: u32,
    offset: u64,
    size: u64,
    link: u32,
    entsize: u64,
}

struct Elf<'a> {
    bytes: &'a [u8],
    sections: Vec<Section>,
}

impl<'a> Elf<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let header = parse_header(bytes)?;
        let sections = parse_sections(bytes, header)?;
        Ok(Self { bytes, sections })
    }

    fn dynamic_deps(&self) -> Result<Value, String> {
        let mut deps = BTreeSet::new();
        for section in self.sections.iter().filter(|s| s.sh_type == SHT_DYNAMIC) {
            let strings = self.linked_strings(section)?;
            for (tag, value) in self.dynamic_entries(section)? {
                if tag == DT_NULL {
                    break;
                }
                if tag == DT_NEEDED {
                    deps.insert(cstr(strings, usize_from_u64(value)?)?.to_string());
                }
            }
        }
        Ok(Value::Array(deps.into_iter().map(Value::Str).collect()))
    }

    fn needs_glibc(&self) -> Result<Value, String> {
        let mut max: Option<(Vec<u64>, String)> = None;
        for section in self
            .sections
            .iter()
            .filter(|s| s.sh_type == SHT_GNU_VERNEED)
        {
            let strings = self.linked_strings(section)?;
            let mut offset = usize_from_u64(section.offset)?;
            let end = checked_end(offset, usize_from_u64(section.size)?, self.bytes.len())?;
            while offset < end {
                let aux = u32_at(self.bytes, offset + 8)? as usize;
                let next = u32_at(self.bytes, offset + 12)? as usize;
                let mut aux_offset = checked_add(offset, aux, end)?;
                loop {
                    let name_offset = u32_at(self.bytes, aux_offset + 8)? as usize;
                    let name = cstr(strings, name_offset)?;
                    if let Some(version) = glibc_version(name) {
                        let candidate = (version, name.to_string());
                        if max.as_ref().is_none_or(|current| candidate.0 > current.0) {
                            max = Some(candidate);
                        }
                    }
                    let aux_next = u32_at(self.bytes, aux_offset + 12)? as usize;
                    if aux_next == 0 {
                        break;
                    }
                    aux_offset = checked_add(aux_offset, aux_next, end)?;
                }
                if next == 0 {
                    break;
                }
                offset = checked_add(offset, next, end)?;
            }
        }
        Ok(Value::Str(max.map(|(_, name)| name).unwrap_or_default()))
    }

    fn sections(&self) -> Result<Value, String> {
        Ok(Value::Array(
            self.sections
                .iter()
                .map(|section| {
                    Value::Map(BTreeMap::from([
                        (
                            Value::Str("name".to_string()),
                            Value::Str(section.name.clone()),
                        ),
                        (
                            Value::Str("size".to_string()),
                            Value::Int(i64_from_u64(section.size)),
                        ),
                    ]))
                })
                .collect(),
        ))
    }

    fn symbols(&self) -> Result<Value, String> {
        let mut names = BTreeSet::new();
        for section in self
            .sections
            .iter()
            .filter(|s| matches!(s.sh_type, SHT_SYMTAB | SHT_DYNSYM))
        {
            if section.entsize == 0 {
                continue;
            }
            let strings = self.linked_strings(section)?;
            let mut offset = usize_from_u64(section.offset)?;
            let end = checked_end(offset, usize_from_u64(section.size)?, self.bytes.len())?;
            let step = usize_from_u64(section.entsize)?;
            while offset + 24 <= end {
                let name_offset = u32_at(self.bytes, offset)? as usize;
                if name_offset != 0
                    && let Ok(name) = cstr(strings, name_offset)
                    && !name.is_empty()
                {
                    names.insert(name.to_string());
                }
                offset = checked_add(offset, step, end)?;
            }
        }
        Ok(Value::Array(names.into_iter().map(Value::Str).collect()))
    }

    fn linker_metadata(&self) -> Result<Value, String> {
        let mut values = BTreeSet::new();
        for section in self
            .sections
            .iter()
            .filter(|s| matches!(s.name.as_str(), ".comment" | ".ident"))
        {
            for value in cstrings(self.section_bytes(section)?) {
                values.insert(value);
            }
        }
        Ok(Value::Array(values.into_iter().map(Value::Str).collect()))
    }

    fn linked_strings(&self, section: &Section) -> Result<&'a [u8], String> {
        let strings = self
            .sections
            .get(section.link as usize)
            .ok_or_else(|| format!("section {} has bad string-table link", section.name))?;
        if strings.sh_type != SHT_STRTAB {
            return Err(format!(
                "section {} link is not a string table",
                section.name
            ));
        }
        self.section_bytes(strings)
    }

    fn section_bytes(&self, section: &Section) -> Result<&'a [u8], String> {
        slice(self.bytes, section.offset, section.size)
    }

    fn dynamic_entries(&self, section: &Section) -> Result<Vec<(i64, u64)>, String> {
        let data = self.section_bytes(section)?;
        let mut entries = Vec::new();
        for chunk in data.chunks_exact(16) {
            let tag = i64::from_le_bytes(chunk[0..8].try_into().expect("dynamic tag"));
            let value = u64::from_le_bytes(chunk[8..16].try_into().expect("dynamic value"));
            entries.push((tag, value));
        }
        Ok(entries)
    }
}

impl Header {
    fn arch(self) -> &'static str {
        match self.e_machine {
            62 => "x86_64",
            183 => "aarch64",
            3 => "x86",
            40 => "arm",
            243 => "riscv",
            _ => "unknown",
        }
    }

    fn kind(self) -> &'static str {
        match self.e_type {
            1 => "rel",
            2 => "exec",
            3 => "dyn",
            4 => "core",
            _ => "unknown",
        }
    }
}

fn parse_header(bytes: &[u8]) -> Result<Header, String> {
    if bytes.len() < 64 || &bytes[0..4] != b"\x7fELF" {
        return Err("not an ELF file".into());
    }
    if bytes[EI_CLASS] != ELFCLASS64 {
        return Err("elf reader supports ELF64 only".into());
    }
    if bytes[EI_DATA] != ELFDATA2LSB {
        return Err("elf reader supports little-endian ELF only".into());
    }
    Ok(Header {
        e_type: u16_at(bytes, 16)?,
        e_machine: u16_at(bytes, 18)?,
        e_shoff: u64_at(bytes, 40)?,
        e_shentsize: u16_at(bytes, 58)?,
        e_shnum: u16_at(bytes, 60)?,
        e_shstrndx: u16_at(bytes, 62)?,
    })
}

fn parse_sections(bytes: &[u8], header: Header) -> Result<Vec<Section>, String> {
    if header.e_shoff == 0 || header.e_shnum == 0 {
        return Ok(Vec::new());
    }
    let count = header.e_shnum as usize;
    let entsize = header.e_shentsize as usize;
    if entsize < 64 {
        return Err("ELF64 section headers are shorter than 64 bytes".into());
    }
    let shoff = usize_from_u64(header.e_shoff)?;
    let table_end = checked_end(shoff, count.saturating_mul(entsize), bytes.len())?;
    let shstr = header.e_shstrndx as usize;
    if shstr >= count {
        return Err("section-name string table index is out of range".into());
    }
    let shstr_at = shoff + shstr * entsize;
    let shstr_bytes = slice(
        bytes,
        u64_at(bytes, shstr_at + 24)?,
        u64_at(bytes, shstr_at + 32)?,
    )?;
    let mut sections = Vec::with_capacity(count);
    for index in 0..count {
        let at = shoff + index * entsize;
        if at + 64 > table_end {
            return Err("section header extends past section table".into());
        }
        let name_offset = u32_at(bytes, at)? as usize;
        sections.push(Section {
            name: cstr(shstr_bytes, name_offset).unwrap_or("").to_string(),
            sh_type: u32_at(bytes, at + 4)?,
            offset: u64_at(bytes, at + 24)?,
            size: u64_at(bytes, at + 32)?,
            link: u32_at(bytes, at + 40)?,
            entsize: u64_at(bytes, at + 56)?,
        });
    }
    Ok(sections)
}

fn glibc_version(name: &str) -> Option<Vec<u64>> {
    let rest = name.strip_prefix("GLIBC_")?;
    rest.split('.').map(|part| part.parse().ok()).collect()
}

fn cstrings(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .filter_map(|part| std::str::from_utf8(part).ok())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn cstr(bytes: &[u8], offset: usize) -> Result<&str, String> {
    if offset >= bytes.len() {
        return Err(format!("string offset {offset} out of range"));
    }
    let end = bytes[offset..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|end| offset + end)
        .ok_or_else(|| format!("unterminated string at offset {offset}"))?;
    std::str::from_utf8(&bytes[offset..end]).map_err(|err| err.to_string())
}

fn slice(bytes: &[u8], offset: u64, len: u64) -> Result<&[u8], String> {
    let offset = usize_from_u64(offset)?;
    let len = usize_from_u64(len)?;
    let end = checked_end(offset, len, bytes.len())?;
    Ok(&bytes[offset..end])
}

fn checked_end(offset: usize, len: usize, limit: usize) -> Result<usize, String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| "ELF offset overflow".to_string())?;
    if end > limit {
        return Err("ELF range extends past end of file".into());
    }
    Ok(end)
}

fn checked_add(offset: usize, len: usize, limit: usize) -> Result<usize, String> {
    let next = offset
        .checked_add(len)
        .ok_or_else(|| "ELF offset overflow".to_string())?;
    if next > limit {
        return Err("ELF linked offset extends past section".into());
    }
    Ok(next)
}

fn usize_from_u64(value: u64) -> Result<usize, String> {
    usize::try_from(value).map_err(|_| format!("ELF value {value} does not fit usize"))
}

fn i64_from_u64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or_else(|| format!("ELF u16 at {at} out of range"))?
            .try_into()
            .expect("u16 slice length"),
    ))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| format!("ELF u32 at {at} out of range"))?
            .try_into()
            .expect("u32 slice length"),
    ))
}

fn u64_at(bytes: &[u8], at: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(at..at + 8)
            .ok_or_else(|| format!("ELF u64 at {at} out of range"))?
            .try_into()
            .expect("u64 slice length"),
    ))
}
