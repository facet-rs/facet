//! Conservative Mach-O relocation-walk test selection.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use object::read::archive::ArchiveFile;
use object::{
    BinaryFormat, Object, ObjectSection, ObjectSymbol, RelocationEncoding, RelocationFlags,
    RelocationKind, RelocationTarget, SectionIndex, SectionKind, SymbolKind, macho,
};
use tracing::{debug, warn};

pub type AtomId = String;

#[derive(Clone, Debug)]
pub struct Analysis {
    pub stats: AnalysisStats,
    pub atoms: BTreeMap<AtomId, Atom>,
    pub graph: BTreeMap<AtomId, BTreeSet<AtomId>>,
    pub defs_by_name: BTreeMap<String, BTreeSet<AtomId>>,
    pub masked_edges: Vec<MaskedEdge>,
    pub unknown_edges: BTreeMap<AtomId, Vec<UnknownReason>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalysisStats {
    pub objects: usize,
    pub archive_objects: usize,
    pub loose_objects: usize,
    pub duplicate_objects_skipped: usize,
    pub physical_sections: usize,
    pub physical_text_sections: usize,
    pub text_atoms: usize,
}

#[derive(Clone, Debug)]
pub struct Atom {
    pub id: AtomId,
    pub object_label: String,
    pub section_name: String,
    pub segment_name: String,
    pub section_kind: SectionKind,
    pub aliases: Vec<String>,
    pub display_name: String,
    pub range_start: u64,
    pub range_end: u64,
    pub bytes: Vec<u8>,
    pub relocations: Vec<RelocRef>,
    pub hash: String,
    pub masked_location_metadata: bool,
}

#[derive(Clone, Debug)]
pub struct RelocRef {
    pub offset: u64,
    pub width: usize,
    pub target_name: String,
    pub target_display: String,
    pub kind: RelocationKind,
    pub encoding: RelocationEncoding,
    pub size_bits: u8,
    pub addend: i64,
    pub flags: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaskedEdge {
    pub source: AtomId,
    pub target: AtomId,
    pub reason: MaskReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaskReason {
    PanicLocationMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnknownReason {
    NonMachOInput { label: String, format: String },
    ParseObject { label: String, error: String },
    UnknownRelocationKind { atom: AtomId, target: String },
    UnknownRelocationEncoding { atom: AtomId, target: String },
    UnknownRelocationTarget { atom: AtomId, target: String },
    RelocationSubtractor { atom: AtomId, target: String },
    UnresolvedRelocation { atom: AtomId, target: String },
    MissingTestRoot { test: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestReach {
    pub root_atom: AtomId,
    pub reachable_atoms: BTreeSet<AtomId>,
    pub reachable_hashes: BTreeSet<String>,
    pub unknowns: Vec<UnknownReason>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionOutcome {
    Reached { changed_hashes: BTreeSet<String> },
    NotReached,
    UnknownRun { reasons: Vec<UnknownReason> },
}

#[derive(Clone, Debug)]
struct SectionInfo {
    ordinal: usize,
    name: String,
    segment_name: String,
    kind: SectionKind,
    address: u64,
    data: Vec<u8>,
}

#[derive(Clone, Debug)]
struct SymbolCandidate {
    raw_name: String,
    display_name: String,
    offset: u64,
    is_global: bool,
    kind: SymbolKind,
}

#[derive(Clone, Debug)]
struct AtomRange {
    id: AtomId,
    start: u64,
    end: u64,
}

#[derive(Default)]
struct AnalysisBuilder {
    stats: AnalysisStats,
    atoms: BTreeMap<AtomId, Atom>,
    defs_by_name: BTreeMap<String, BTreeSet<AtomId>>,
    section_aliases: BTreeMap<String, AtomId>,
    input_unknowns: Vec<UnknownReason>,
}

pub fn analyze_target_deps(deps_dir: &Path) -> Result<Analysis, String> {
    let mut object_inputs = Vec::new();
    collect_files_with_ext(deps_dir, OsStr::new("rlib"), &mut object_inputs)?;
    collect_files_with_ext(deps_dir, OsStr::new("o"), &mut object_inputs)?;
    analyze_object_paths(deps_dir, &object_inputs)
}

pub fn analyze_object_paths(root: &Path, object_inputs: &[PathBuf]) -> Result<Analysis, String> {
    let mut seen_hashes = HashSet::new();
    let mut builder = AnalysisBuilder::default();
    let mut inputs = object_inputs.to_vec();
    inputs.sort();

    for path in inputs {
        let data = fs::read(&path).map_err(|err| format!("read {path:?}: {err}"))?;
        if path.extension() == Some(OsStr::new("rlib")) {
            let archive = ArchiveFile::parse(&*data)
                .map_err(|err| format!("parse archive {path:?}: {err}"))?;
            for member in archive.members() {
                let member = member.map_err(|err| format!("archive member {path:?}: {err}"))?;
                let name = String::from_utf8_lossy(member.name()).to_string();
                if !name.ends_with(".o") {
                    continue;
                }
                let member_data = member
                    .data(&*data)
                    .map_err(|err| format!("archive member data {path:?}({name}): {err}"))?;
                if !seen_hashes.insert(fnv_hex(member_data)) {
                    builder.stats.duplicate_objects_skipped += 1;
                    continue;
                }
                let label = format!("{}({name})", path_label(root, &path));
                parse_object(&label, member_data, &mut builder);
                builder.stats.objects += 1;
                builder.stats.archive_objects += 1;
            }
        } else {
            if !seen_hashes.insert(fnv_hex(&data)) {
                builder.stats.duplicate_objects_skipped += 1;
                continue;
            }
            let label = path_label(root, &path);
            parse_object(&label, &data, &mut builder);
            builder.stats.objects += 1;
            builder.stats.loose_objects += 1;
        }
    }

    compute_atom_hashes(&mut builder);
    mark_panic_location_metadata(&mut builder);
    Ok(build_analysis(builder))
}

pub fn compare_test_reachability(
    before: &Analysis,
    after: &Analysis,
    test_name: &str,
) -> SelectionOutcome {
    let before_reach = before.test_reach(test_name);
    let after_reach = after.test_reach(test_name);
    let (before_reach, after_reach) = match (before_reach, after_reach) {
        (Ok(before_reach), Ok(after_reach)) => (before_reach, after_reach),
        (Err(reason), Ok(after_reach)) => {
            let mut reasons = vec![reason];
            reasons.extend(after_reach.unknowns);
            return SelectionOutcome::UnknownRun { reasons };
        }
        (Ok(before_reach), Err(reason)) => {
            let mut reasons = before_reach.unknowns;
            reasons.push(reason);
            return SelectionOutcome::UnknownRun { reasons };
        }
        (Err(a), Err(b)) => {
            return SelectionOutcome::UnknownRun {
                reasons: vec![a, b],
            };
        }
    };

    let mut reasons = before_reach.unknowns;
    reasons.extend(after_reach.unknowns);
    if !reasons.is_empty() {
        return SelectionOutcome::UnknownRun { reasons };
    }

    let changed_hashes = before_reach
        .reachable_hashes
        .symmetric_difference(&after_reach.reachable_hashes)
        .cloned()
        .collect::<BTreeSet<_>>();
    if changed_hashes.is_empty() {
        SelectionOutcome::NotReached
    } else {
        SelectionOutcome::Reached { changed_hashes }
    }
}

impl Analysis {
    pub fn test_reach(&self, test_name: &str) -> Result<TestReach, UnknownReason> {
        let root =
            self.find_test_root(test_name)
                .ok_or_else(|| UnknownReason::MissingTestRoot {
                    test: test_name.to_string(),
                })?;
        let reachable_atoms = reachable_from(&self.graph, &root);
        let reachable_hashes = reachable_atoms
            .iter()
            .filter_map(|id| self.atoms.get(id))
            .filter(|atom| is_loadable_kind(atom.section_kind))
            .map(|atom| atom.hash.clone())
            .collect();
        let unknowns = reachable_atoms
            .iter()
            .filter_map(|id| self.unknown_edges.get(id))
            .flatten()
            .cloned()
            .collect();
        Ok(TestReach {
            root_atom: root,
            reachable_atoms,
            reachable_hashes,
            unknowns,
        })
    }

    pub fn find_test_root(&self, test_name: &str) -> Option<AtomId> {
        let mut candidates = self
            .atoms
            .values()
            .filter(|atom| {
                atom.section_kind == SectionKind::Text
                    && atom
                        .aliases
                        .iter()
                        .chain(std::iter::once(&atom.display_name))
                        .any(|name| {
                            demangle_symbol(name).contains(test_name) || name.contains(test_name)
                        })
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|atom| {
            (
                atom.display_name.contains("{{closure}}"),
                atom.display_name.len(),
                atom.id.clone(),
            )
        });
        candidates.first().map(|atom| atom.id.clone())
    }
}

fn parse_object(label: &str, data: &[u8], builder: &mut AnalysisBuilder) {
    let file = match object::File::parse(data) {
        Ok(file) => file,
        Err(err) => {
            builder.input_unknowns.push(UnknownReason::ParseObject {
                label: label.to_string(),
                error: err.to_string(),
            });
            warn!(label, error = %err, "could not parse object input");
            return;
        }
    };

    if file.format() != BinaryFormat::MachO {
        builder.input_unknowns.push(UnknownReason::NonMachOInput {
            label: label.to_string(),
            format: format!("{:?}", file.format()),
        });
        debug!(label, format = ?file.format(), "unsupported object format");
        return;
    }

    let mut sections = Vec::new();
    let mut section_ord_by_index = HashMap::new();
    for (ordinal, section) in file.sections().enumerate() {
        section_ord_by_index.insert(section.index(), ordinal);
        let kind = section.kind();
        if kind == SectionKind::Text {
            builder.stats.physical_text_sections += 1;
        }
        builder.stats.physical_sections += 1;
        sections.push(SectionInfo {
            ordinal,
            name: section
                .name()
                .unwrap_or("<invalid-section-name>")
                .to_string(),
            segment_name: section
                .segment_name()
                .ok()
                .flatten()
                .unwrap_or("<no-segment>")
                .to_string(),
            kind,
            address: section.address(),
            data: section.data().unwrap_or(&[]).to_vec(),
        });
    }

    let mut symbols_by_section: BTreeMap<usize, Vec<SymbolCandidate>> = BTreeMap::new();
    for symbol in file.symbols() {
        if symbol.is_undefined() || matches!(symbol.kind(), SymbolKind::File | SymbolKind::Section)
        {
            continue;
        }
        let Some(section_index) = symbol.section_index() else {
            continue;
        };
        let Some(&section_ordinal) = section_ord_by_index.get(&section_index) else {
            continue;
        };
        let section = &sections[section_ordinal];
        let address = symbol.address();
        if address < section.address {
            continue;
        }
        let offset = address - section.address;
        if offset > section.data.len() as u64 {
            continue;
        }
        let raw_name = symbol.name().unwrap_or("<invalid-symbol-name>").to_string();
        if raw_name.is_empty() {
            continue;
        }
        symbols_by_section
            .entry(section_ordinal)
            .or_default()
            .push(SymbolCandidate {
                display_name: demangle_symbol(&raw_name),
                raw_name,
                offset,
                is_global: symbol.is_global(),
                kind: symbol.kind(),
            });
    }

    let mut atom_ranges_by_section: BTreeMap<usize, Vec<AtomRange>> = BTreeMap::new();
    for section in &sections {
        let mut symbols = symbols_by_section
            .remove(&section.ordinal)
            .unwrap_or_default();
        symbols.sort_by(|a, b| {
            a.offset
                .cmp(&b.offset)
                .then_with(|| b.is_global.cmp(&a.is_global))
                .then_with(|| a.raw_name.cmp(&b.raw_name))
        });
        let mut starts = symbols
            .iter()
            .map(|symbol| symbol.offset)
            .collect::<Vec<_>>();
        starts.sort_unstable();
        starts.dedup();

        let mut ranges = Vec::new();
        if starts.is_empty() {
            if should_keep_symbolless_section(section.kind, &section.data) {
                let id = format!("{}::section{}@0", label, section.ordinal);
                let alias = format!("{}::__section_{}", label, section.ordinal);
                insert_atom(
                    builder,
                    section,
                    id.clone(),
                    0,
                    section.data.len() as u64,
                    vec![alias],
                );
                ranges.push(AtomRange {
                    id,
                    start: 0,
                    end: section.data.len() as u64,
                });
            }
            atom_ranges_by_section.insert(section.ordinal, ranges);
            continue;
        }

        if starts[0] > 0 && should_keep_symbolless_section(section.kind, &section.data) {
            let id = format!("{}::section{}@0", label, section.ordinal);
            let alias = format!("{}::__section_{}_prefix", label, section.ordinal);
            let end = starts[0];
            insert_atom(builder, section, id.clone(), 0, end, vec![alias]);
            ranges.push(AtomRange { id, start: 0, end });
        }

        for (start_index, start) in starts.iter().enumerate() {
            let end = starts
                .get(start_index + 1)
                .copied()
                .unwrap_or(section.data.len() as u64);
            if end <= *start {
                continue;
            }
            let aliases = symbols
                .iter()
                .filter(|symbol| symbol.offset == *start)
                .cloned()
                .collect::<Vec<_>>();
            let primary = choose_primary_alias(&aliases);
            let id = format!(
                "{}::section{}@{}:{}",
                label, section.ordinal, start, primary.raw_name
            );
            let atom_aliases = aliases
                .iter()
                .map(|symbol| symbol.raw_name.clone())
                .collect::<Vec<_>>();
            insert_atom(builder, section, id.clone(), *start, end, atom_aliases);
            if section.kind == SectionKind::Text {
                builder.stats.text_atoms += 1;
            }
            ranges.push(AtomRange {
                id,
                start: *start,
                end,
            });
        }
        atom_ranges_by_section.insert(section.ordinal, ranges);
    }

    for section in &sections {
        if let Some(first_atom) = atom_ranges_by_section
            .get(&section.ordinal)
            .and_then(|ranges| ranges.first())
        {
            let alias = section_alias(label, section.ordinal);
            builder
                .defs_by_name
                .entry(alias.clone())
                .or_default()
                .insert(first_atom.id.clone());
            builder.section_aliases.insert(alias, first_atom.id.clone());
        }
    }

    for section in file.sections() {
        let Some(&section_ordinal) = section_ord_by_index.get(&section.index()) else {
            continue;
        };
        let Some(ranges) = atom_ranges_by_section.get(&section_ordinal) else {
            continue;
        };
        for (offset, relocation) in section.relocations() {
            let Some(source_atom) = ranges.iter().find(|range| {
                (offset >= range.start && offset < range.end)
                    || (offset == range.start && range.start == range.end)
            }) else {
                continue;
            };
            let (target_name, target_display) = relocation_target_name(
                &file,
                label,
                &sections,
                &section_ord_by_index,
                relocation.target(),
            );
            let flags = relocation.flags();
            let reloc_ref = RelocRef {
                offset,
                width: relocation_width(relocation.size()),
                target_display,
                target_name,
                kind: relocation.kind(),
                encoding: relocation.encoding(),
                size_bits: relocation.size(),
                addend: relocation.addend(),
                flags: format!("{flags:?}"),
            };
            if let Some(atom) = builder.atoms.get_mut(&source_atom.id) {
                atom.relocations.push(reloc_ref);
            }
            let target_debug = relocation_target_debug(&file, relocation.target());
            if target_debug == "<unknown-relocation-target>" {
                builder
                    .input_unknowns
                    .push(UnknownReason::UnknownRelocationTarget {
                        atom: source_atom.id.clone(),
                        target: target_debug.clone(),
                    });
            }
            if !is_supported_relocation(flags) {
                builder
                    .input_unknowns
                    .push(UnknownReason::UnknownRelocationKind {
                        atom: source_atom.id.clone(),
                        target: format!("{target_debug} {flags:?}"),
                    });
            }
            if relocation.subtractor().is_some() {
                builder
                    .input_unknowns
                    .push(UnknownReason::RelocationSubtractor {
                        atom: source_atom.id.clone(),
                        target: target_debug,
                    });
            }
        }
    }
}

fn insert_atom(
    builder: &mut AnalysisBuilder,
    section: &SectionInfo,
    id: AtomId,
    start: u64,
    end: u64,
    aliases: Vec<String>,
) {
    let bytes = section.data[start as usize..end as usize].to_vec();
    let display_name = aliases
        .first()
        .map(|alias| demangle_symbol(alias))
        .unwrap_or_else(|| id.clone());
    let atom = Atom {
        id: id.clone(),
        object_label: id
            .split_once("::section")
            .map(|(label, _)| label.to_string())
            .unwrap_or_else(|| id.clone()),
        section_name: section.name.clone(),
        segment_name: section.segment_name.clone(),
        section_kind: section.kind,
        aliases: aliases.clone(),
        display_name,
        range_start: start,
        range_end: end,
        bytes,
        relocations: Vec::new(),
        hash: String::new(),
        masked_location_metadata: false,
    };
    for alias in aliases {
        builder
            .defs_by_name
            .entry(alias)
            .or_default()
            .insert(id.clone());
    }
    builder.atoms.insert(id, atom);
}

fn choose_primary_alias(aliases: &[SymbolCandidate]) -> SymbolCandidate {
    aliases
        .iter()
        .min_by_key(|symbol| {
            let temp = symbol.raw_name.contains("Ltmp")
                || symbol.raw_name.contains("ltmp")
                || symbol.raw_name.contains("GCC_except_table");
            let kind_rank = match symbol.kind {
                SymbolKind::Text => 0,
                SymbolKind::Data => 1,
                SymbolKind::Label => 2,
                SymbolKind::Unknown => 3,
                _ => 4,
            };
            (
                temp,
                !symbol.is_global,
                kind_rank,
                symbol.display_name.len(),
                symbol.raw_name.clone(),
            )
        })
        .expect("atom aliases are non-empty")
        .clone()
}

fn relocation_target_name<'data>(
    file: &object::File<'data>,
    label: &str,
    sections: &[SectionInfo],
    section_ord_by_index: &HashMap<SectionIndex, usize>,
    target: RelocationTarget,
) -> (String, String) {
    match target {
        RelocationTarget::Symbol(index) => match file.symbol_by_index(index) {
            Ok(symbol) => {
                let name = symbol
                    .name()
                    .unwrap_or("<invalid-relocation-symbol>")
                    .to_string();
                let display = demangle_symbol(&name);
                (name, display)
            }
            Err(_) => (
                "<bad-relocation-symbol-index>".to_string(),
                "<bad-relocation-symbol-index>".to_string(),
            ),
        },
        RelocationTarget::Section(index) => {
            if let Some(&ordinal) = section_ord_by_index.get(&index) {
                let name = section_alias(label, ordinal);
                let display = sections
                    .get(ordinal)
                    .map(|section| format!("{}:{}", section.segment_name, section.name))
                    .unwrap_or_else(|| name.clone());
                (name, display)
            } else {
                (
                    "<bad-relocation-section-index>".to_string(),
                    "<bad-relocation-section-index>".to_string(),
                )
            }
        }
        RelocationTarget::Absolute => ("<absolute>".to_string(), "<absolute>".to_string()),
        _ => (
            "<unknown-relocation-target>".to_string(),
            "<unknown-relocation-target>".to_string(),
        ),
    }
}

fn relocation_target_debug<'data>(file: &object::File<'data>, target: RelocationTarget) -> String {
    match target {
        RelocationTarget::Symbol(index) => file
            .symbol_by_index(index)
            .ok()
            .and_then(|symbol| symbol.name().ok().map(str::to_string))
            .unwrap_or_else(|| format!("symbol-index-{index:?}")),
        RelocationTarget::Section(index) => format!("section-index-{index:?}"),
        RelocationTarget::Absolute => "<absolute>".to_string(),
        _ => "<unknown-relocation-target>".to_string(),
    }
}

fn compute_atom_hashes(builder: &mut AnalysisBuilder) {
    for atom in builder.atoms.values_mut() {
        atom.relocations.sort_by(|a, b| {
            a.offset
                .cmp(&b.offset)
                .then_with(|| a.target_name.cmp(&b.target_name))
        });
        let mut normalized = atom.bytes.clone();
        for relocation in &atom.relocations {
            if relocation.offset < atom.range_start {
                continue;
            }
            let local_offset = (relocation.offset - atom.range_start) as usize;
            let end = normalized.len().min(local_offset + relocation.width);
            for byte in normalized.get_mut(local_offset..end).unwrap_or_default() {
                *byte = 0;
            }
        }

        let mut hash = Fnv64::new();
        hash.write(b"vix-reloc-selection-atom-v2");
        hash.write(atom.section_name.as_bytes());
        hash.write(format!("{:?}", atom.section_kind).as_bytes());
        hash.write_u64(normalized.len() as u64);
        hash.write(&normalized);
        for relocation in &atom.relocations {
            hash.write_u64(relocation.offset - atom.range_start);
            hash.write(relocation.target_name.as_bytes());
            hash.write(format!("{:?}", relocation.kind).as_bytes());
            hash.write(format!("{:?}", relocation.encoding).as_bytes());
            hash.write(&[relocation.size_bits]);
            hash.write(&relocation.addend.to_le_bytes());
            hash.write(relocation.flags.as_bytes());
        }
        atom.hash = hash.finish_hex();
    }
}

fn is_supported_relocation(flags: RelocationFlags) -> bool {
    match flags {
        RelocationFlags::MachO {
            r_type, r_length, ..
        } => {
            const KNOWN_TYPES: &[u8] = &[
                macho::ARM64_RELOC_UNSIGNED,
                macho::ARM64_RELOC_BRANCH26,
                macho::ARM64_RELOC_PAGE21,
                macho::ARM64_RELOC_PAGEOFF12,
                macho::ARM64_RELOC_GOT_LOAD_PAGE21,
                macho::ARM64_RELOC_GOT_LOAD_PAGEOFF12,
                macho::ARM64_RELOC_POINTER_TO_GOT,
                macho::ARM64_RELOC_TLVP_LOAD_PAGE21,
                macho::ARM64_RELOC_TLVP_LOAD_PAGEOFF12,
                macho::X86_64_RELOC_UNSIGNED,
                macho::X86_64_RELOC_SIGNED,
                macho::X86_64_RELOC_BRANCH,
                macho::X86_64_RELOC_GOT_LOAD,
                macho::X86_64_RELOC_GOT,
                macho::X86_64_RELOC_SIGNED_1,
                macho::X86_64_RELOC_SIGNED_2,
                macho::X86_64_RELOC_SIGNED_4,
                macho::X86_64_RELOC_TLV,
            ];
            r_length <= 3 && KNOWN_TYPES.contains(&r_type)
        }
        RelocationFlags::Generic {
            kind,
            encoding,
            size,
        } => kind != RelocationKind::Unknown && encoding != RelocationEncoding::Unknown && size > 0,
        RelocationFlags::Elf { .. }
        | RelocationFlags::Coff { .. }
        | RelocationFlags::Xcoff { .. } => false,
        _ => false,
    }
}

fn mark_panic_location_metadata(builder: &mut AnalysisBuilder) {
    let mut masked = BTreeSet::new();
    for (id, atom) in &builder.atoms {
        if looks_like_panic_location(atom, builder) {
            masked.insert(id.clone());
        }
    }
    for id in masked {
        if let Some(atom) = builder.atoms.get_mut(&id) {
            atom.masked_location_metadata = true;
        }
    }
}

fn looks_like_panic_location(atom: &Atom, builder: &AnalysisBuilder) -> bool {
    if atom.section_kind == SectionKind::Text || !is_loadable_kind(atom.section_kind) {
        return false;
    }
    if atom.bytes.len() < 16 || atom.bytes.len() > 96 {
        return false;
    }
    let has_path_relocation = atom.relocations.iter().any(|relocation| {
        builder
            .defs_by_name
            .get(&relocation.target_name)
            .into_iter()
            .flatten()
            .filter_map(|id| builder.atoms.get(id))
            .any(is_path_string_atom)
    });
    has_path_relocation && has_line_col_words(&atom.bytes)
}

fn is_path_string_atom(atom: &Atom) -> bool {
    if atom.section_kind != SectionKind::ReadOnlyString {
        return false;
    }
    let text = String::from_utf8_lossy(&atom.bytes);
    text.contains(".rs") && (text.contains('/') || text.contains('\\') || text.contains("src"))
}

fn has_line_col_words(bytes: &[u8]) -> bool {
    let words = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>();
    words.windows(2).any(|pair| {
        let line = pair[0];
        let col = pair[1];
        (1..1_000_000).contains(&line) && col < 100_000
    })
}

fn build_analysis(builder: AnalysisBuilder) -> Analysis {
    let mut graph = BTreeMap::new();
    let mut unknown_edges: BTreeMap<AtomId, Vec<UnknownReason>> = BTreeMap::new();
    let mut masked_edges = Vec::new();

    for reason in builder.input_unknowns {
        let atom = match &reason {
            UnknownReason::UnknownRelocationKind { atom, .. }
            | UnknownReason::UnknownRelocationEncoding { atom, .. }
            | UnknownReason::UnknownRelocationTarget { atom, .. }
            | UnknownReason::RelocationSubtractor { atom, .. }
            | UnknownReason::UnresolvedRelocation { atom, .. } => Some(atom.clone()),
            UnknownReason::NonMachOInput { .. }
            | UnknownReason::ParseObject { .. }
            | UnknownReason::MissingTestRoot { .. } => None,
        };
        if let Some(atom) = atom {
            unknown_edges.entry(atom).or_default().push(reason);
        }
    }

    for atom in builder.atoms.values() {
        let mut targets = BTreeSet::new();
        if atom.masked_location_metadata {
            graph.insert(atom.id.clone(), targets);
            continue;
        }

        for relocation in &atom.relocations {
            if relocation.target_name == "<absolute>" {
                continue;
            }
            let Some(defs) = builder.defs_by_name.get(&relocation.target_name) else {
                if !is_known_terminal_external(&relocation.target_display) {
                    unknown_edges.entry(atom.id.clone()).or_default().push(
                        UnknownReason::UnresolvedRelocation {
                            atom: atom.id.clone(),
                            target: relocation.target_display.clone(),
                        },
                    );
                }
                continue;
            };

            for target in defs {
                if builder
                    .atoms
                    .get(target)
                    .is_some_and(|target_atom| target_atom.masked_location_metadata)
                {
                    masked_edges.push(MaskedEdge {
                        source: atom.id.clone(),
                        target: target.clone(),
                        reason: MaskReason::PanicLocationMetadata,
                    });
                } else {
                    targets.insert(target.clone());
                }
            }
        }
        graph.insert(atom.id.clone(), targets);
    }

    Analysis {
        stats: builder.stats,
        atoms: builder.atoms,
        graph,
        defs_by_name: builder.defs_by_name,
        masked_edges,
        unknown_edges,
    }
}

fn is_known_terminal_external(target: &str) -> bool {
    target == "__Unwind_Resume" || (target.starts_with("core[") && target.contains("::panicking::"))
}

fn reachable_from(graph: &BTreeMap<AtomId, BTreeSet<AtomId>>, root: &str) -> BTreeSet<AtomId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(targets) = graph.get(&id) {
            stack.extend(targets.iter().cloned());
        }
    }
    seen
}

fn collect_files_with_ext(dir: &Path, ext: &OsStr, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|err| format!("read_dir {dir:?}: {err}"))? {
        let entry = entry.map_err(|err| format!("read_dir entry {dir:?}: {err}"))?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|err| format!("file_type {path:?}: {err}"))?
            .is_dir()
        {
            collect_files_with_ext(&path, ext, out)?;
        } else if path.extension() == Some(ext) {
            out.push(path);
        }
    }
    Ok(())
}

fn path_label(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn demangle_symbol(name: &str) -> String {
    let candidates = [
        name,
        name.strip_prefix('_').unwrap_or(name),
        name.strip_prefix("__").unwrap_or(name),
    ];
    for candidate in candidates {
        let demangled = rustc_demangle::demangle(candidate).to_string();
        if demangled != candidate {
            return demangled;
        }
    }
    name.to_string()
}

fn should_keep_symbolless_section(kind: SectionKind, data: &[u8]) -> bool {
    !data.is_empty() && (is_loadable_kind(kind) || kind == SectionKind::Debug)
}

fn is_loadable_kind(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::Text
            | SectionKind::Data
            | SectionKind::ReadOnlyData
            | SectionKind::ReadOnlyDataWithRel
            | SectionKind::ReadOnlyString
            | SectionKind::UninitializedData
            | SectionKind::Common
            | SectionKind::Tls
            | SectionKind::UninitializedTls
            | SectionKind::TlsVariables
    )
}

fn relocation_width(size_bits: u8) -> usize {
    if size_bits == 0 {
        8
    } else {
        usize::from(size_bits).div_ceil(8).max(1)
    }
}

fn section_alias(label: &str, ordinal: usize) -> String {
    format!("{label}::__section_{ordinal}")
}

fn fnv_hex(bytes: &[u8]) -> String {
    let mut hash = Fnv64::new();
    hash.write(bytes);
    hash.finish_hex()
}

struct Fnv64 {
    state: u64,
}

impl Fnv64 {
    fn new() -> Self {
        Self {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}", self.state)
    }
}
